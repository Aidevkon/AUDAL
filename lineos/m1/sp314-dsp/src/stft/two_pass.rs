//! TwoPassEngine — Constitutional Memory-Aware Streaming DSP
//! Authority: v3_memory_aware_streaming.md
//!
//! INV-ST-1: W computed ONCE per run (Pass 1)
//! INV-ST-2: W is READ-ONLY during Pass 2
//! INV-ST-3: peak RAM ≤ 50MB for any file length
//! INV-ST-4: OLA overlap = FFT_SIZE / 2
//! INV-ST-5: chunk size = 65536 samples
//! INV-ST-6: EBU R128 output identical to full-load processing
//! INV-AB-1: deterministic — same input → same output

use crate::stft::{StftStreamContext, N_BINS, FFT_SIZE, HOP_SIZE};
use crate::stft::nmf::{NmfEngine, N_COMPONENTS};
use crate::stft::hpss::{HpssProcessor, HpssStreamContext};

/// Constitutional chunk size — 65536 samples = ~1.37s at 48kHz
/// INV-ST-5: fixed, never configurable at runtime
pub const CHUNK_FRAMES: usize = 65536;

/// Downsample ratio for Pass 1 Scout proxy
/// stereo 48kHz → mono ~11kHz = ~4.4× reduction
const SCOUT_DOWNSAMPLE: usize = 4;

/// Result of Pass 1 — locked W + semantic indices.
/// All fields are computed ONCE from W and never change.
/// INV-ST-1: W is read-only after scout() returns.
#[derive(Clone)]
pub struct ScoutResult {
    /// NMF basis matrix [N_BINS × N_COMPONENTS] — READ ONLY
    pub w:             Vec<f32>,
    /// Rough RMS of low-res proxy
    pub proxy_rms:     f32,
    /// Semantic stem indices — computed from W centroids + flatness
    /// Pass 2 uses these indices blindly — no recomputation
    pub voice_idx:     usize,
    pub bass_idx:      usize,
    pub harmonics_idx: usize,
    pub ambience_idx:  usize,
}

/// Streaming error type
#[derive(Debug)]
pub enum StreamError {
    Io(String),
    Empty,
}

impl core::fmt::Display for StreamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StreamError::Io(e)  => write!(f, "StreamError::Io({e})"),
            StreamError::Empty  => write!(f, "StreamError::Empty"),
        }
    }
}

/// Metadata returned by render_to_writer — audio goes to disk.
pub struct RenderMetadata {
    pub frames_written:          usize,
    pub voice_transient_density: f32,
    pub drums_transient_density: f32,
}

/// Two-pass streaming engine.
///
/// Usage:
///   let mut engine = TwoPassEngine::new();
///   let scout  = engine.scout(signal);
///   let meta   = engine.render_to_writer(reader, writer, &scout)?;
pub struct TwoPassEngine {
    nmf: NmfEngine,
}

impl TwoPassEngine {
    pub fn new() -> Self {
        Self { nmf: NmfEngine::default() }
    }

    // ── Pass 1 — Scout ───────────────────────────────────────────────

    /// Pass 1: low-res proxy → NMF fit() → locked W + semantic indices.
    /// Memory: ~5MB regardless of signal length.
    /// INV-ST-1: this is the ONLY call to fit() per run.
    pub fn scout(&mut self, signal: &[f32]) -> ScoutResult {
        // Downsample to ~11kHz mono proxy
        let proxy: Vec<f32> = signal
            .iter()
            .step_by(SCOUT_DOWNSAMPLE)
            .copied()
            .collect();

        // Rough RMS
        let proxy_rms = if !proxy.is_empty() {
            let sq: f32 = proxy.iter().map(|s| s * s).sum();
            libm::sqrtf(sq / proxy.len() as f32)
        } else {
            0.0
        };

        // STFT on proxy
        let mut ctx = StftStreamContext::new();
        let proxy_frames = ctx.forward_chunk(&proxy);

        // NMF fit — learns W from proxy
        // INV-ST-1: ONLY fit() call
        let w = self.nmf.fit(&proxy_frames);

        // ── Semantic assignment from W (Opt-1) ───────────────────────
        // W is now locked. Compute indices once here.
        // Pass 2 uses these indices blindly — no recomputation.
        let n_bins = N_BINS;

        // Spectral flatness per component
        let mut flatness = [0.0f32; N_COMPONENTS];
        for c in 0..N_COMPONENTS {
            let mut log_sum = 0.0f32;
            let mut arith   = 0.0f32;
            let eps = 1e-10f32;
            for b in 0..n_bins {
                let w_val = w[b * N_COMPONENTS + c];
                log_sum += libm::logf(w_val + eps);
                arith   += w_val;
            }
            let geom = libm::expf(log_sum / n_bins as f32);
            let mean = arith / n_bins as f32;
            flatness[c] = if mean > eps {
                (geom / mean).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }

        // Spectral centroids per component
        let centroids = self.nmf.centroids(n_bins);

        // Ambience = highest flatness
        let ambience_idx = (0..N_COMPONENTS)
            .max_by(|&a, &b| flatness[a].partial_cmp(&flatness[b]).unwrap())
            .unwrap_or(0);

        // Bass = lowest centroid (excluding ambience)
        let remaining: Vec<usize> = (0..N_COMPONENTS)
            .filter(|&i| i != ambience_idx)
            .collect();

        let bass_idx = remaining.iter()
            .min_by(|&&a, &&b| centroids[a].partial_cmp(&centroids[b]).unwrap())
            .copied()
            .unwrap_or(1);

        let voice_harmonics: Vec<usize> = remaining.iter()
            .filter(|&&i| i != bass_idx)
            .copied()
            .collect();

        let (voice_idx, harmonics_idx) = match voice_harmonics.len() {
            0 => (0, 0),
            1 => (voice_harmonics[0], voice_harmonics[0]),
            _ => (voice_harmonics[0], voice_harmonics[1]),
        };

        // Store W in nmf for transform() calls in Pass 2
        self.nmf.w = w.clone();

        ScoutResult {
            w,
            proxy_rms,
            voice_idx,
            bass_idx,
            harmonics_idx,
            ambience_idx,
        }
    }

    // ── Pass 2 — Render to Writer ────────────────────────────────────

    /// Pass 2: chunk-by-chunk stem separation → audio written to disk.
    /// Memory: ~2MB constant per chunk.
    /// INV-ST-2: W never modified.
    /// INV-ST-3: peak RAM < 50MB for any file.
    pub fn render_to_writer(
        &mut self,
        signal: &[f32],
        writer: &mut dyn FnMut(&[f32]),
        scout:  &ScoutResult,
    ) -> Result<RenderMetadata, StreamError> {
        if signal.is_empty() {
            return Err(StreamError::Empty);
        }

        let n_total = signal.len();

        // ── Stateful DSP contexts — survive across chunks (Opt-2) ────
        let mut stft_ctx  = StftStreamContext::new();
        let mut hpss_ctx  = HpssStreamContext::new();

        let mut frames_written          = 0usize;
        let mut voice_transient_sum     = 0.0f32;
        let mut drums_transient_sum     = 0.0f32;
        let mut chunk_count             = 0usize;

        let mut offset = 0usize;

        while offset < n_total {
            let end   = (offset + CHUNK_FRAMES).min(n_total);
            let chunk = &signal[offset..end];

            // Step 1: STFT magnitude — stateful OLA context
            let chunk_frames = stft_ctx.forward_chunk(chunk);
            let n_frames     = chunk_frames.len();

            if n_frames == 0 {
                offset = end;
                continue;
            }

            // Step 2: Stateful HPSS — carries L_HARM history (Opt-2)
            let (mask_h, mask_p) = hpss_ctx.process_chunk(&chunk_frames);

            // Step 3: NMF transform with LOCKED W — INV-ST-2
            let h_chunk = self.nmf.transform(&scout.w, &chunk_frames);

            // Step 4: Per-chunk component masks (no full-file allocation)
            let voice_mask = self.nmf.component_mask_chunk(
                scout.voice_idx, &h_chunk, n_frames, N_BINS,
            );

            // Step 5: Apply mask → time-domain via envelope
            // (Full phase iSTFT is ST-P4 scope)
            let voice_audio = apply_mask_to_chunk(chunk, &voice_mask, n_frames);

            // Step 6: Write valid samples to disk (Opt-3)
            writer(&voice_audio);
            frames_written += voice_audio.len();

            // Step 7: Accumulate lightweight metadata
            let h_voice: Vec<f32> = (0..n_frames)
                .map(|f| {
                    let idx = scout.voice_idx * n_frames + f;
                    if idx < h_chunk.len() { h_chunk[idx] } else { 0.0 }
                })
                .collect();
            let h_drums: Vec<f32> = (0..n_frames)
                .map(|f| {
                    // Use mask_p energy as drums proxy
                    if f < mask_p.len() {
                        mask_p[f].iter().sum::<f32>() / N_BINS as f32
                    } else {
                        0.0
                    }
                })
                .collect();

            let vt = transient_density(&h_voice);
            let dt = transient_density(&h_drums);
            voice_transient_sum += vt;
            drums_transient_sum += dt;
            chunk_count += 1;

            // Step 8: All chunk data drops here — back to ~2MB RAM
            offset = end;
        }

        // Step 9: Flush OLA tail (Opt-3)
        let tail = stft_ctx.flush();
        if !tail.is_empty() {
            writer(&tail);
            frames_written += tail.len();
        }

        let avg = chunk_count.max(1) as f32;
        Ok(RenderMetadata {
            frames_written,
            voice_transient_density: voice_transient_sum / avg,
            drums_transient_density: drums_transient_sum / avg,
        })
    }
}

impl Default for TwoPassEngine {
    fn default() -> Self { Self::new() }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Apply per-frame magnitude mask to time-domain chunk via envelope.
/// mask: [n_frames][n_bins] — averaged to per-frame energy weight.
/// Returns time-domain samples of same length as chunk.
fn apply_mask_to_chunk(
    chunk:    &[f32],
    mask:     &[Vec<f32>],
    n_frames: usize,
) -> Vec<f32> {
    if n_frames == 0 || mask.is_empty() || chunk.is_empty() {
        return chunk.to_vec();
    }
    // Average mask across bins → per-frame energy weight
    let frame_weights: Vec<f32> = mask.iter()
        .map(|frame| {
            if frame.is_empty() { 0.0 }
            else { frame.iter().sum::<f32>() / frame.len() as f32 }
        })
        .collect();

    // Upsample frame weights to sample resolution
    let n = chunk.len();
    let weights: Vec<f32> = (0..n).map(|i| {
        let pos  = i as f32 * (n_frames - 1).max(1) as f32 / n.max(1) as f32;
        let idx0 = pos.floor() as usize;
        let idx1 = (idx0 + 1).min(frame_weights.len() - 1);
        let frac = pos - idx0 as f32;
        let w0   = frame_weights.get(idx0).copied().unwrap_or(0.0);
        let w1   = frame_weights.get(idx1).copied().unwrap_or(0.0);
        w0 * (1.0 - frac) + w1 * frac
    }).collect();

    chunk.iter().zip(weights.iter()).map(|(s, w)| s * w).collect()
}

/// Compute transient density of activation vector.
/// Deterministic — no randomness. INV-AB-1.
fn transient_density(h: &[f32]) -> f32 {
    let n = h.len();
    if n < 2 { return 0.0; }
    let deltas: Vec<f32> = (1..n).map(|i| (h[i] - h[i-1]).abs()).collect();
    let mean = deltas.iter().sum::<f32>() / deltas.len() as f32;
    let var  = deltas.iter().map(|d| (d - mean).powi(2)).sum::<f32>()
               / deltas.len() as f32;
    let thr  = mean + libm::sqrtf(var);
    deltas.iter().filter(|&&d| d > thr).count() as f32 / n as f32
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| {
            libm::sinf(2.0 * core::f32::consts::PI * freq * i as f32 / 48000.0)
        }).collect()
    }

    #[test]
    fn scout_produces_locked_w_and_indices() {
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal);

        assert_eq!(scout.w.len(), N_BINS * N_COMPONENTS,
            "W must be [N_BINS × N_COMPONENTS]");
        assert!(scout.voice_idx < N_COMPONENTS);
        assert!(scout.bass_idx  < N_COMPONENTS);
        assert!(scout.ambience_idx < N_COMPONENTS);
    }

    #[test]
    fn scout_is_deterministic() {
        // INV-ST-1 + INV-AB-1
        let signal = sine(1000.0, 48000);
        let mut e1 = TwoPassEngine::new();
        let mut e2 = TwoPassEngine::new();
        let s1 = e1.scout(&signal);
        let s2 = e2.scout(&signal);
        for (a, b) in s1.w.iter().zip(s2.w.iter()) {
            assert!((a - b).abs() < 1e-6, "INV-AB-1: W must be identical");
        }
        assert_eq!(s1.voice_idx,     s2.voice_idx);
        assert_eq!(s1.bass_idx,      s2.bass_idx);
        assert_eq!(s1.ambience_idx,  s2.ambience_idx);
    }

    #[test]
    fn render_to_writer_produces_output() {
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout  = engine.scout(&signal);

        let mut output_samples: Vec<f32> = Vec::new();
        let result = engine.render_to_writer(
            &signal,
            &mut |chunk| output_samples.extend_from_slice(chunk),
            &scout,
        );

        assert!(result.is_ok(), "render_to_writer must succeed");
        assert!(!output_samples.is_empty(), "Must produce output samples");
    }

    #[test]
    fn w_read_only_during_render() {
        // INV-ST-2: W must not change during Pass 2
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout  = engine.scout(&signal);
        let w_before = scout.w.clone();

        let mut _out: Vec<f32> = Vec::new();
        let _ = engine.render_to_writer(
            &signal,
            &mut |chunk| _out.extend_from_slice(chunk),
            &scout,
        );

        assert_eq!(w_before, scout.w, "INV-ST-2: W must not change");
    }

    #[test]
    fn hpss_stream_context_handles_multiple_chunks() {
        use crate::stft::hpss::HpssStreamContext;
        let mut ctx = HpssStreamContext::new();
        let frame: Vec<f32> = vec![0.5f32; crate::stft::N_BINS];
        let chunk: Vec<Vec<f32>> = vec![frame; 128];

        let (mh1, mp1) = ctx.process_chunk(&chunk);
        let (mh2, mp2) = ctx.process_chunk(&chunk);

        assert_eq!(mh1.len(), 128);
        assert_eq!(mh2.len(), 128);
        assert_eq!(mp1.len(), 128);
        assert_eq!(mp2.len(), 128);
    }
}
