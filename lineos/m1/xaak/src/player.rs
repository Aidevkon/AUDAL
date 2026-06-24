//! player.rs — CpalPlayer: cpal audio output backend for xaak.
//! Authority: Amendment A-003 §8
//!
//! cpal sits BELOW xaak.rs (A-003 §8). It does not own PCM.
//! It receives a ring buffer consumer from XaakKernel::stream_from()
//! and drives the audio output callback via the ALSA backend on Linux.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::{Consumer, Split};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

/// cpal-backed audio output driver.
/// Does not own PCM — receives a ring buffer consumer from XaakKernel.
pub struct CpalPlayer {
    stream: Option<cpal::Stream>,
    position_ms: Arc<Mutex<u64>>,
    /// Set to true by stop().
    /// Signals the telemetry worker that this specific stream has ended.
    stream_ended: Option<Arc<AtomicBool>>,
    telem_tx: std::sync::mpsc::Sender<crate::telemetry_worker::TelemetryCommand>,
}

impl CpalPlayer {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        crate::telemetry_worker::spawn(rx);
        Self {
            stream: None,
            position_ms: Arc::new(Mutex::new(0)),
            stream_ended: None,
            telem_tx: tx,
        }
    }

    /// Start outputting PCM from the consumer.
    ///
    /// Accepts any type that implements Consumer<Item = f32> + Send + 'static,
    /// matching the opaque return from XaakKernel::stream_from().
    ///
    /// `raw_consumer` — optional consumer from kernel_raw (pre-mastering PCM).
    ///   When present, its samples are tapped in parallel and pushed to the
    ///   raw telemetry ring buffer for spectrum_before computation.
    pub fn play<C>(
        &mut self,
        mut consumer: C,
        mut raw_consumer: Option<C>,
        sample_rate: u32,
        channels: u16,
        position_ms: Arc<Mutex<u64>>,
    ) -> Result<(), String>
    where
        C: Consumer<Item = f32> + Send + 'static,
    {
        // Explicitly stop any active stream before doing anything else.
        self.stop();

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "cpal: no default audio output device found".to_string())?;

        tracing::debug!(
            device = ?device.name(),
            sample_rate,
            channels,
            "cpal: opening output stream"
        );

        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // Create a BRAND NEW stream_ended flag for this specific playback instance
        let new_stream_ended = Arc::new(AtomicBool::new(false));
        self.stream_ended = Some(new_stream_ended.clone());

        let pos = position_ms.clone();
        let sr = sample_rate as u64;
        let ch = channels as u64;
        // Clone new flag for capture into the cpal callback closure.
        let stream_ended_cb = new_stream_ended.clone();
        // Consecutive callbacks where mastered consumer returned 0 samples.
        // When this reaches UNDERRUN_STREAK_N, natural EOF is declared.
        // N=5: ≈53ms at 512-frame buffer (10.7ms/cb), ≈106ms at 1024-frame buffer.
        let mut zero_read_streak: u32 = 0;
        const UNDERRUN_STREAK_N: u32 = 5;

        // TB-P6: ring buffers for telemetry worker (mastered + raw).
        // Audio callback only pushes raw samples — no math, no syscalls.
        let telem_rb = ringbuf::HeapRb::<f32>::new(1024 * 16);
        let (telem_prod, telem_cons) = telem_rb.split();
        let mut telem_prod = telem_prod;

        let telem_rb_raw = ringbuf::HeapRb::<f32>::new(1024 * 16);
        let (telem_prod_raw, telem_cons_raw) = telem_rb_raw.split();
        let mut telem_prod_raw = telem_prod_raw;

        // Send StartStream to the persistent telemetry actor
        if let Err(e) = self.telem_tx.send(crate::telemetry_worker::TelemetryCommand::StartStream {
            mastered_cons: telem_cons,
            raw_cons: Some(telem_cons_raw),
            sample_rate,
            channels: channels as usize,
            position_ms: position_ms.clone(),
            stream_ended: new_stream_ended.clone(),
        }) {
            tracing::warn!("xaak: failed to send StartStream to telemetry worker: {}", e);
        }

        // Scratch buffer for raw tap (avoids heap allocation in callback).
        let mut raw_scratch = vec![0.0f32; 16384];

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Drain mastered consumer into speaker output buffer.
                    let mut filled = 0;
                    while filled < data.len() {
                        let n = consumer.pop_slice(&mut data[filled..]);
                        if n == 0 { break; }
                        filled += n;
                    }

                    // Underrun detection: track consecutive callbacks with zero PCM read.
                    // filled==0 means the XaakKernel ring buffer is exhausted (natural EOF).
                    if filled == 0 {
                        zero_read_streak += 1;
                        if zero_read_streak >= UNDERRUN_STREAK_N
                            && !stream_ended_cb.load(Ordering::Relaxed)
                        {
                            stream_ended_cb.store(true, Ordering::Release);
                        }
                    } else {
                        zero_read_streak = 0;
                    }

                    // Parallel raw tap — mirror the same number of samples from
                    // kernel_raw into the raw telemetry ring buffer.
                    if let Some(ref mut raw_cons) = raw_consumer {
                        let to_read = filled.min(raw_scratch.len());
                        let mut raw_filled = 0;
                        while raw_filled < to_read {
                            let n = raw_cons.pop_slice(&mut raw_scratch[raw_filled..to_read]);
                            if n == 0 { break; }
                            raw_filled += n;
                        }
                        let mut pushed = 0;
                        while pushed < raw_filled {
                            let n = ringbuf::traits::Producer::push_slice(
                                &mut telem_prod_raw,
                                &raw_scratch[pushed..raw_filled],
                            );
                            if n == 0 { break; }
                            pushed += n;
                        }
                    }

                    // Silence-pad any underrun frames.
                    for s in &mut data[filled..] {
                        *s = 0.0;
                    }

                    // Advance playback position.
                    let frames = filled as u64 / ch.max(1);
                    let delta_ms = frames.saturating_mul(1000) / sr.max(1);
                    if let Ok(mut p) = pos.lock() {
                        *p = p.saturating_add(delta_ms);
                    }

                    // TB-P6: push mastered samples to telemetry ring buffer.
                    // Lock-free — never blocks audio thread.
                    let mut pushed = 0;
                    while pushed < data.len() {
                        let n = ringbuf::traits::Producer::push_slice(
                            &mut telem_prod,
                            &data[pushed..],
                        );
                        if n == 0 { break; }
                        pushed += n;
                    }
                },
                |err| tracing::error!("cpal stream error: {err}"),
                None,
            )
            .map_err(|e| format!("cpal: build_output_stream failed: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("cpal: stream.play() failed: {e}"))?;

        self.stream = Some(stream);
        self.position_ms = position_ms;

        tracing::info!("cpal: playback started");
        Ok(())
    }

    /// Pause output stream without resetting position.
    pub fn pause(&mut self) {
        if let Some(ref s) = self.stream {
            if let Err(e) = s.pause() {
                tracing::warn!("cpal: pause failed: {e}");
            } else {
                tracing::debug!("cpal: paused");
            }
        }
    }

    /// Stop and drop the stream. Position reset handled by PlaybackEngine.
    pub fn stop(&mut self) {
        self.stream = None;
        if let Some(flag) = self.stream_ended.take() {
            flag.store(true, Ordering::Release);
        }
        tracing::debug!("cpal: stopped");
    }

    /// Current playback position in milliseconds.
    pub fn position_ms(&self) -> u64 {
        self.position_ms.lock().map(|p| *p).unwrap_or(0)
    }
}

impl Drop for CpalPlayer {
    fn drop(&mut self) {
        let _ = self.telem_tx.send(crate::telemetry_worker::TelemetryCommand::Shutdown);
    }
}

impl Default for CpalPlayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Decimate audio block to 32 (L, R) pairs for Lissajous goniometer.
/// Takes every N-th sample pair from interleaved stereo or mono data.
/// INV-TB-7: always returns exactly 32 pairs.
pub fn decimate_gonio(data: &[f32], channels: usize) -> [(f32, f32); 32] {
    let mut pairs = [(0.0f32, 0.0f32); 32];
    let ch = channels.max(1);
    let frames = data.len() / ch;
    let step = (frames / 32).max(1);
    for (i, pair) in pairs.iter_mut().enumerate() {
        let idx = (i * step * ch).min(data.len().saturating_sub(ch));
        let l = data.get(idx).copied().unwrap_or(0.0);
        let r = if ch > 1 {
            data.get(idx + 1).copied().unwrap_or(0.0)
        } else {
            l
        };
        *pair = (l, r);
    }
    pairs
}
