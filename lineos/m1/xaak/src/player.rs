//! player.rs — CpalPlayer: cpal audio output backend for xaak.
//! Authority: Amendment A-003 §8
//!
//! cpal sits BELOW xaak.rs (A-003 §8). It does not own PCM.
//! It receives a ring buffer consumer from XaakKernel::stream_from()
//! and drives the audio output callback via the ALSA backend on Linux.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::{Split, Consumer};
use std::sync::{Arc, Mutex};

/// cpal-backed audio output driver.
/// Does not own PCM — receives a ring buffer consumer from XaakKernel.
pub struct CpalPlayer {
    stream:      Option<cpal::Stream>,
    position_ms: Arc<Mutex<u64>>,
}

impl CpalPlayer {
    pub fn new() -> Self {
        Self {
            stream:      None,
            position_ms: Arc::new(Mutex::new(0)),
        }
    }

    /// Start outputting PCM from the consumer.
    ///
    /// Accepts any type that implements Consumer<Item = f32> + Send + 'static,
    /// matching the opaque return from XaakKernel::stream_from().
    pub fn play<C>(
        &mut self,
        mut consumer: C,
        sample_rate:  u32,
        channels:     u16,
        position_ms:  Arc<Mutex<u64>>,
    ) -> Result<(), String>
    where
        C: Consumer<Item = f32> + Send + 'static,
    {
        let host   = cpal::default_host();
        let device = host.default_output_device()
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

        let pos = position_ms.clone();
        let sr  = sample_rate as u64;
        let ch  = channels as u64;

        // TB-P6: ring buffer for telemetry worker
        // Audio callback only pushes raw samples — no math, no syscalls
        let telem_rb = ringbuf::HeapRb::<f32>::new(1024 * 16);
        let (telem_prod, telem_cons) = telem_rb.split();
        let mut telem_prod = telem_prod;  // explicit binding

        // Spawn telemetry worker — FFT + UDP off audio thread
        crate::telemetry_worker::spawn(
            telem_cons,
            sample_rate,
            channels as usize,
            position_ms.clone(),
        );

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let filled = consumer.pop_slice(data);
                // Fill any remaining frames with silence
                for s in &mut data[filled..] { *s = 0.0; }
                // Advance playback position
                let frames   = filled as u64 / ch.max(1);
                let delta_ms = frames.saturating_mul(1000) / sr.max(1);
                if let Ok(mut p) = pos.lock() {
                    *p = p.saturating_add(delta_ms);
                }
                // TB-P6: push raw samples to telemetry ring buffer
                // Lock-free push — never blocks audio thread
                let _ = ringbuf::traits::Producer::push_slice(&mut telem_prod, data);
            },
            |err| tracing::error!("cpal stream error: {err}"),
            None,
        ).map_err(|e| format!("cpal: build_output_stream failed: {e}"))?;

        stream.play().map_err(|e| format!("cpal: stream.play() failed: {e}"))?;

        self.stream      = Some(stream);
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
        tracing::debug!("cpal: stopped");
    }

    /// Current playback position in milliseconds.
    pub fn position_ms(&self) -> u64 {
        self.position_ms.lock().map(|p| *p).unwrap_or(0)
    }
}

impl Default for CpalPlayer {
    fn default() -> Self { Self::new() }
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
        } else { l };
        *pair = (l, r);
    }
    pairs
}
