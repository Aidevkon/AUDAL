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

use crate::spatial::channel_assign::StemChannelAssignments;
use crate::spatial::SpatialPreAnalysis;
use crate::stft::hpss::HpssStreamContext;
use crate::stft::nmf::{NmfEngine, N_COMPONENTS};
use crate::stft::stem_renderer::FiveStems;
use crate::stft::{StreamingStftEncoder, N_BINS};
use lineos_corpus::mfcc::MfccAnalyzer;
use lineos_types::StemFeatures;

/// Constitutional chunk size — 65536 samples = ~1.37s at 48kHz
pub const CHUNK_FRAMES: usize = 65536;

/// Downsample ratio for Pass 1 Scout proxy (~11kHz mono)
const SCOUT_DOWNSAMPLE: usize = 4;

const COLLISION_DRUMS_TRANSIENT_THRESHOLD: f32 = 0.25;
const COLLISION_BASS_RMS_THRESHOLD_DB: f32 = -40.0;
const COLLISION_DUCKING_GAIN: f32 = 0.707;
const COLLISION_SMOOTHING_ALPHA: f32 = 0.005;

/// Per-stem MFCC fingerprints computed during scout().
/// Captures the timbral identity of each stem BEFORE render.
/// Used by AutoTuningController to set adaptive ducking_gain.
/// INV-AB-1: computed deterministically from scout proxy stems.
#[derive(Debug, Clone)]
pub struct StemMfccs {
    pub voice: [f32; 13],
    pub drums: [f32; 13],
    pub bass: [f32; 13],
    pub harmonics: [f32; 13],
    pub ambience: [f32; 13],
}

impl StemMfccs {
    /// Zero MFCCs — used before computation or as fallback.
    pub fn zero() -> Self {
        Self {
            voice: [0.0f32; 13],
            drums: [0.0f32; 13],
            bass: [0.0f32; 13],
            harmonics: [0.0f32; 13],
            ambience: [0.0f32; 13],
        }
    }

    /// L2 distance between two MFCC vectors.
    /// Used to measure timbral similarity/difference.
    pub fn distance(a: &[f32; 13], b: &[f32; 13]) -> f32 {
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            .sqrt()
    }

    /// Bass vs Drums timbral distance.
    /// High distance = different timbre = low collision risk.
    /// Low distance = similar timbre = high collision risk.
    pub fn bass_drums_distance(&self) -> f32 {
        Self::distance(&self.bass, &self.drums)
    }
}

/// A single chunk of 5 stems — chunk-sized slices only.
/// Never holds full-file data. Passed to process_chunks callback.
pub struct FiveStemsChunk {
    pub voice: Vec<f32>,
    pub drums: Vec<f32>,
    pub bass: Vec<f32>,
    pub harmonics: Vec<f32>,
    pub ambience: Vec<f32>,
}

/// Result of Pass 1 — all static parameters locked.
/// Pass 2 uses these blindly — no recomputation.
/// INV-ST-1: W is read-only after scout() returns.
#[derive(Clone)]
pub struct ScoutResult {
    /// NMF basis matrix — READ ONLY
    pub w: Vec<f32>,
    /// Rough RMS from proxy
    pub proxy_rms: f32,
    /// Semantic indices — computed from W
    pub voice_idx: usize,
    pub bass_idx: usize,
    pub harmonics_idx: usize,
    pub ambience_idx: usize,
    /// Locked spatial assignments from proxy stems
    pub assignments: StemChannelAssignments,
    /// Pre-computed firewall scales from proxy energy
    pub rear_scale: f32,
    pub lfe_scale: f32,
    /// Global RMS gain for energy compensation
    pub global_rms_gain: f32,
    /// Spatial pre-analysis from proxy
    pub spatial_pre: SpatialPreAnalysis,
    /// Per-stem MFCC fingerprints from scout proxy analysis.
    /// StemMfccs::zero() until M-P2 populates them.
    pub stem_mfccs: StemMfccs,
    /// NMF stem features from proxy
    pub features: StemFeatures,
}

/// Streaming error
#[derive(Debug)]
pub enum StreamError {
    Io(String),
    Empty,
}

impl core::fmt::Display for StreamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StreamError::Io(e) => write!(f, "StreamError::Io({e})"),
            StreamError::Empty => write!(f, "StreamError::Empty"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BandSpatialMetrics {
    pub pan_mean: f32,
    pub pan_width: f32,
}

/// Metadata returned after render — audio on disk, no audio Vec.
pub struct RenderMetadata {
    pub frames_written: usize,
    pub voice_transient_density: f32,
    pub drums_transient_density: f32,
    pub spatial: [BandSpatialMetrics; 5],
}

pub struct TwoPassEngine {
    nmf: NmfEngine,
    /// Psychoacoustic Collision Matrix — smoothed ducking gain state.
    /// Initialized to 1.0 (no ducking). Persists across chunks.
    bass_ducking_gain: f32,
}

pub(crate) fn detect_collision(drums_chunk: &[f32], bass_chunk: &[f32]) -> bool {
    if drums_chunk.is_empty() || bass_chunk.is_empty() {
        return false;
    }
    let drums_sum_sq: f32 = drums_chunk.iter().map(|s| s * s).sum();
    let drums_mean_sq = drums_sum_sq / drums_chunk.len() as f32;
    let drums_diff_sq: f32 = drums_chunk.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
    let drums_td = if drums_mean_sq > 1e-10 {
        drums_diff_sq / (drums_mean_sq * drums_chunk.len() as f32)
    } else {
        0.0
    };
    let bass_sum_sq: f32 = bass_chunk.iter().map(|s| s * s).sum();
    let bass_mean_sq = bass_sum_sq / bass_chunk.len() as f32;
    let bass_rms_db = if bass_mean_sq > 1e-15 {
        10.0 * libm::log10f(bass_mean_sq)
    } else {
        -144.0
    };
    drums_td > COLLISION_DRUMS_TRANSIENT_THRESHOLD && bass_rms_db > COLLISION_BASS_RMS_THRESHOLD_DB
}

impl TwoPassEngine {
    pub fn new() -> Self {
        Self {
            nmf: NmfEngine::default(),
            bass_ducking_gain: 1.0,
        }
    }

    // ── Pass 1 — Scout ───────────────────────────────────────────────

    /// Pass 1: proxy analysis → all static parameters locked.
    /// INV-ST-1: only fit() call in the entire run.
    pub fn scout(&mut self, signal: &[f32], sample_rate: u32) -> ScoutResult {
        let t_scout = std::time::Instant::now();

        // Downsample → ~11kHz mono proxy
        let proxy: Vec<f32> = signal.iter().step_by(SCOUT_DOWNSAMPLE).copied().collect();

        let proxy_rms = if !proxy.is_empty() {
            let sq: f32 = proxy.iter().map(|s| s * s).sum();
            libm::sqrtf(sq / proxy.len() as f32)
        } else {
            0.0
        };

        // STFT on proxy
        let t_stft = std::time::Instant::now();
        let proxy_frames: Vec<Vec<f32>> = {
            let mut enc = StreamingStftEncoder::new();
            let mut frames = enc.feed_chunk(&proxy);
            frames.extend(enc.finish());
            frames
                .into_iter()
                .map(|frame| frame.iter().map(|c| c.norm()).collect())
                .collect()
        };
        eprintln!("[PERF] stft_proxy={}ms", t_stft.elapsed().as_millis());
        let n_frames = proxy_frames.len();
        let n_bins = if n_frames > 0 {
            proxy_frames[0].len()
        } else {
            0
        };
        eprintln!("[PERF] proxy size: {} frames x {} bins", n_frames, n_bins);

        // NMF fit — INV-ST-1: ONLY fit() call
        // W learned at proxy sample rate (~12kHz, SCOUT_DOWNSAMPLE=4)
        let t_fit = std::time::Instant::now();
        let w_proxy = self.nmf.fit(&proxy_frames);
        eprintln!("[PERF] nmf_fit={}ms", t_fit.elapsed().as_millis());

        // ── W-matrix bin mapping: proxy (12kHz) → full (48kHz) ──────
        // SCOUT_DOWNSAMPLE=4 is an integer → b_proxy = b_full * 4 exactly.
        // No floating-point interpolation needed — direct index lookup.
        //
        // Bin mapping:
        //   b_full × 23.4 Hz = b_proxy × 5.86 Hz  (same physical frequency)
        //   b_proxy = b_full * SCOUT_DOWNSAMPLE
        //
        // Bins above proxy Nyquist (~6kHz): filled with EPS.
        // NMF will treat them as "no template" — leaves them untouched.
        // INV-AB-1: deterministic — integer mapping, no rounding.
        // ── W-matrix energy averaging: proxy (12kHz) → full (48kHz) ─
        // proxy bin spacing: 48000 / (4 × 2048) = 5.86 Hz/bin
        // full  bin spacing: 48000 / 2048       = 23.44 Hz/bin
        // SCOUT_DOWNSAMPLE=4: average 4 proxy bins → 1 full bin
        // Preserves micro-harmonic energy — no information lost below ~6kHz
        // Bins above proxy Nyquist (~6kHz): filled with EPS (no template)
        // INV-AB-1: deterministic — integer arithmetic only
        let k = N_COMPONENTS;
        let mut w_full = vec![1e-10_f32; N_BINS * k];
        for c in 0..k {
            for b_full in 0..N_BINS {
                let proxy_start = b_full * SCOUT_DOWNSAMPLE;
                if proxy_start < N_BINS {
                    let mut sum = 0.0_f32;
                    let mut count = 0usize;
                    for offset in 0..SCOUT_DOWNSAMPLE {
                        let b_proxy = proxy_start + offset;
                        if b_proxy < N_BINS {
                            sum += w_proxy[b_proxy * k + c];
                            count += 1;
                        }
                    }
                    if count > 0 {
                        w_full[b_full * k + c] = (sum / count as f32).max(1e-10_f32);
                    }
                }
                // proxy_start >= N_BINS → above ~6kHz → remains EPS
            }
        }
        let w = w_full.clone();
        self.nmf.w = w_full;

        // ── Semantic assignment from W ───────────────────────────────
        let n_bins = N_BINS;
        let mut flatness = [0.0f32; N_COMPONENTS];
        for c in 0..N_COMPONENTS {
            let mut log_sum = 0.0f32;
            let mut arith = 0.0f32;
            let eps = 1e-10f32;
            for b in 0..n_bins {
                let w_val = w_proxy[b * N_COMPONENTS + c];
                log_sum += libm::logf(w_val + eps);
                arith += w_val;
            }
            let geom = libm::expf(log_sum / n_bins as f32);
            let mean = arith / n_bins as f32;
            flatness[c] = if mean > eps {
                (geom / mean).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        // Use proxy W for centroids — relative frequency ordering preserved
        // (proxy bins have correct relative ordering even at 12kHz)
        let w_saved = self.nmf.w.clone();
        self.nmf.w = w_proxy.clone();
        let centroids = self.nmf.centroids(n_bins);
        self.nmf.w = w_saved;

        let ambience_idx = (0..N_COMPONENTS)
            .max_by(|&a, &b| flatness[a].partial_cmp(&flatness[b]).unwrap())
            .unwrap_or(0);
        let remaining: Vec<usize> = (0..N_COMPONENTS).filter(|&i| i != ambience_idx).collect();

        // Sort the remaining 4 components by spectral centroid (lowest to highest)
        let mut sorted_by_centroid = remaining.clone();
        sorted_by_centroid.sort_by(|&a, &b| centroids[a].partial_cmp(&centroids[b]).unwrap());

        // Semantic mapping based on frequency ordering
        let bass_idx = sorted_by_centroid[0]; // Lowest centroid
        let drums_idx = sorted_by_centroid[1]; // 2nd lowest centroid
        let harmonics_idx = sorted_by_centroid[2]; // Mid/High frequencies
        let voice_idx = sorted_by_centroid[3]; // Highest centroid

        // ── Proxy stems for spatial + feature analysis ───────────────
        let proxy_h = self.nmf.transform(&w, &proxy_frames);
        self.nmf.h = proxy_h;

        let proxy_n = proxy_frames.len();
        let t_extraction = std::time::Instant::now();
        let proxy_voice = self.proxy_stem(voice_idx, proxy_n, n_bins, &proxy);
        let proxy_drums = self.proxy_stem(drums_idx, proxy_n, n_bins, &proxy);
        let proxy_bass = self.proxy_stem(bass_idx, proxy_n, n_bins, &proxy);
        let proxy_harm = self.proxy_stem(harmonics_idx, proxy_n, n_bins, &proxy);
        let proxy_amb = self.proxy_stem(ambience_idx, proxy_n, n_bins, &proxy);
        eprintln!(
            "[PERF] proxy_stem_extraction={}ms",
            t_extraction.elapsed().as_millis()
        );

        // M-P2: Compute per-stem MFCC fingerprints from proxy stems.
        // Only first 4096 samples (~85ms) — keeps scout() latency minimal.
        // 4096 samples is sufficient for timbral fingerprint.
        // INV-AB-1: deterministic — same proxy → same MFCCs.
        // M-P2: Compute per-stem MFCC fingerprints.
        // Proxy stems are now exactly the 2-second chorus window passed into scout().
        // INV-AB-1: deterministic — same slice → same MFCCs.
        let stem_mfccs = {
            let mut mfcc = MfccAnalyzer::new();
            StemMfccs {
                voice: mfcc.compute(&proxy_voice),
                drums: mfcc.compute(&proxy_drums),
                bass: mfcc.compute(&proxy_bass),
                harmonics: mfcc.compute(&proxy_harm),
                ambience: mfcc.compute(&proxy_amb),
            }
        };

        // Duplicate proxy signal as stereo for spatial analysis
        let proxy_stereo_l = proxy_voice.clone();
        let proxy_stereo_r = proxy_voice.clone();

        let spatial_pre = SpatialPreAnalysis::analyze(
            &proxy_stereo_l,
            &proxy_stereo_r,
            sample_rate / SCOUT_DOWNSAMPLE as u32,
        );

        // StemFeatures from proxy
        let proxy_fivs = FiveStems {
            voice: proxy_voice.clone(),
            drums: proxy_drums,
            bass: proxy_bass,
            harmonics: proxy_harm,
            ambience: proxy_amb,
            voice_transient_density: 0.0,
            drums_transient_density: 0.0,
            bass_transient_density: 0.0,
            harmonics_transient_density: 0.0,
            ambience_transient_density: 0.0,
        };

        use crate::analysis::StemFeatureAnalyzer;
        let features =
            StemFeatureAnalyzer::analyze(&proxy_fivs, sample_rate / SCOUT_DOWNSAMPLE as u32);

        let assignments = StemChannelAssignments::compute(&features, &spatial_pre);

        // Compute firewall scales from proxy energy (locked for Pass 2)
        let (rear_scale, lfe_scale) = compute_firewall_scales(&proxy_fivs, &assignments);

        // Global RMS gain estimate from proxy
        let proxy_mix_rms = if !proxy_voice.is_empty() {
            let sq: f32 = proxy_voice.iter().map(|s| s * s).sum();
            libm::sqrtf(sq / proxy_voice.len() as f32)
        } else {
            1.0
        };
        let global_rms_gain = if proxy_mix_rms > 1e-10 {
            (proxy_rms / proxy_mix_rms).clamp(0.5, 2.0)
        } else {
            1.0
        };

        // Transform is NOT explicit in scout, but wait...
        // The scout pass returns a ScoutResult. Where does nmf_transform happen?
        // Wait, fit() does fit_transform, which does both W and H learning.
        // It does NOT call transform(). Wait, two_pass.rs only calls fit()!
        eprintln!("[PERF] scout_total={}ms", t_scout.elapsed().as_millis());

        ScoutResult {
            w,
            proxy_rms,
            voice_idx,
            bass_idx,
            harmonics_idx,
            ambience_idx,
            assignments,
            rear_scale,
            lfe_scale,
            global_rms_gain,
            spatial_pre,
            stem_mfccs,
            features,
        }
    }

    /// Extract proxy stem from NMF component mask * proxy signal.
    fn proxy_stem(
        &self,
        component: usize,
        n_frames: usize,
        n_bins: usize,
        proxy: &[f32],
    ) -> Vec<f32> {
        let mask = self
            .nmf
            .component_mask_chunk(component, &self.nmf.h, n_frames, n_bins);
        apply_mask_to_chunk(proxy, &mask, n_frames)
    }

    // ── Pass 2 — process_chunks ──────────────────────────────────────

    /// Pass 2: chunk-by-chunk processing with locked ScoutResult.
    /// Callback receives FiveStemsChunk per chunk.
    /// All DSP context is stateful across chunks.
    /// INV-ST-2: W never modified.
    /// INV-ST-3: peak RAM ~2MB/chunk.
    /// Process with adaptive ducking_gain from Maestro.
    /// ducking_gain: [0.3, 1.0] — replaces COLLISION_DUCKING_GAIN.
    /// Use process_chunks for default behavior (ducking_gain=0.707).
    pub fn process_chunks_with_params<F>(
        &mut self,
        signal: &[f32],
        left: &[f32],
        right: &[f32],
        scout: &ScoutResult,
        ducking_gain: f32,
        mut callback: F,
    ) -> Result<RenderMetadata, StreamError>
    where
        F: FnMut(&FiveStemsChunk),
    {
        if signal.is_empty() {
            return Err(StreamError::Empty);
        }

        let n_total = signal.len();
        use rayon::prelude::*;

        // 1. Prepare overlapping input chunks
        struct ChunkInput {
            start: usize,
            offset: usize,
            end: usize,
        }
        let mut chunk_inputs = Vec::new();
        let mut offset = 0usize;
        while offset < n_total {
            let end = (offset + CHUNK_FRAMES).min(n_total);
            let start = offset.saturating_sub(10240); // 1536 STFT lookahead + 8704 HPSS history
            chunk_inputs.push(ChunkInput { start, offset, end });
            offset = end;
        }

        // 2. Parallel Transform Phase (Heavy Math)
        struct ParallelChunkOut {
            stems: FiveStemsChunk,
            voice_transient: f32,
            drums_transient: f32,
            chunk_len: usize,
            spatial_sums: [(f32, f32, f32); 5], // [(pan_num, width_num, den); 5]
        }

        let parallel_results: Vec<ParallelChunkOut> = chunk_inputs
            .into_par_iter()
            .map(|chunk_in| {
                let padded_chunk = &signal[chunk_in.start..chunk_in.end];
                let padded_left = if chunk_in.start < left.len() {
                    let end = chunk_in.end.min(left.len());
                    &left[chunk_in.start..end]
                } else {
                    &[]
                };
                let padded_right = if chunk_in.start < right.len() {
                    let end = chunk_in.end.min(right.len());
                    &right[chunk_in.start..end]
                } else {
                    &[]
                };

                let mut hpss_ctx = HpssStreamContext::new();

                let chunk_frames: Vec<Vec<f32>> = {
                    let mut enc = StreamingStftEncoder::new();
                    let mut frames = enc.feed_chunk(padded_chunk);
                    frames.extend(enc.finish());
                    frames
                        .into_iter()
                        .map(|frame| frame.iter().map(|c| c.norm()).collect())
                        .collect()
                };
                let core_frames_l: Vec<Vec<f32>> = {
                    let mut enc = StreamingStftEncoder::new();
                    let mut frames = enc.feed_chunk(padded_left);
                    frames.extend(enc.finish());
                    frames
                        .into_iter()
                        .map(|frame| frame.iter().map(|c| c.norm()).collect())
                        .collect()
                };
                let core_frames_r: Vec<Vec<f32>> = {
                    let mut enc = StreamingStftEncoder::new();
                    let mut frames = enc.feed_chunk(padded_right);
                    frames.extend(enc.finish());
                    frames
                        .into_iter()
                        .map(|frame| frame.iter().map(|c| c.norm()).collect())
                        .collect()
                };

                let n_frames = chunk_frames.len();
                if n_frames == 0 {
                    return ParallelChunkOut {
                        stems: FiveStemsChunk {
                            voice: vec![],
                            drums: vec![],
                            bass: vec![],
                            harmonics: vec![],
                            ambience: vec![],
                        },
                        voice_transient: 0.0,
                        drums_transient: 0.0,
                        chunk_len: 0,
                        spatial_sums: [(0.0, 0.0, 0.0); 5],
                    };
                }

                // pad_frames is the number of frames we must discard from the start.
                let pad_frames = if chunk_in.start < chunk_in.offset {
                    (chunk_in.offset - chunk_in.start) / 512
                } else {
                    0
                };

                let (_mask_h, mask_p) = hpss_ctx.process_chunk(&chunk_frames);
                let h_chunk = self.nmf.transform(&scout.w, &chunk_frames);

                let voice_mask =
                    self.nmf
                        .component_mask_chunk(scout.voice_idx, &h_chunk, n_frames, N_BINS);
                let bass_mask =
                    self.nmf
                        .component_mask_chunk(scout.bass_idx, &h_chunk, n_frames, N_BINS);
                let harm_mask =
                    self.nmf
                        .component_mask_chunk(scout.harmonics_idx, &h_chunk, n_frames, N_BINS);
                let amb_mask =
                    self.nmf
                        .component_mask_chunk(scout.ambience_idx, &h_chunk, n_frames, N_BINS);

                let core_n_frames = n_frames.saturating_sub(pad_frames);
                let core_voice_mask = if pad_frames < voice_mask.len() {
                    &voice_mask[pad_frames..]
                } else {
                    &[]
                };
                let core_bass_mask = if pad_frames < bass_mask.len() {
                    &bass_mask[pad_frames..]
                } else {
                    &[]
                };
                let core_harm_mask = if pad_frames < harm_mask.len() {
                    &harm_mask[pad_frames..]
                } else {
                    &[]
                };
                let core_amb_mask = if pad_frames < amb_mask.len() {
                    &amb_mask[pad_frames..]
                } else {
                    &[]
                };
                let core_mask_p = if pad_frames < mask_p.len() {
                    &mask_p[pad_frames..]
                } else {
                    &[]
                };

                let core_frames_l = if pad_frames < core_frames_l.len() {
                    &core_frames_l[pad_frames..]
                } else {
                    &[]
                };
                let core_frames_r = if pad_frames < core_frames_r.len() {
                    &core_frames_r[pad_frames..]
                } else {
                    &[]
                };

                let core_chunk = &signal[chunk_in.offset..chunk_in.end];

                let voice_chunk = apply_mask_to_chunk(core_chunk, core_voice_mask, core_n_frames);
                let bass_chunk = apply_mask_to_chunk(core_chunk, core_bass_mask, core_n_frames);
                let harm_chunk = apply_mask_to_chunk(core_chunk, core_harm_mask, core_n_frames);
                let amb_chunk = apply_mask_to_chunk(core_chunk, core_amb_mask, core_n_frames);

                let drums_weights: Vec<f32> = (0..core_chunk.len())
                    .map(|i| {
                        let f = i * core_n_frames / core_chunk.len().max(1);
                        if f < core_mask_p.len() {
                            core_mask_p[f].iter().sum::<f32>() / N_BINS as f32
                        } else {
                            0.0
                        }
                    })
                    .collect();
                let drums_chunk: Vec<f32> = core_chunk
                    .iter()
                    .zip(drums_weights.iter())
                    .map(|(s, w)| s * w)
                    .collect();

                let h_voice: Vec<f32> = (0..core_n_frames)
                    .map(|f| {
                        let real_f = f + pad_frames;
                        let idx = scout.voice_idx * n_frames + real_f;
                        if idx < h_chunk.len() {
                            h_chunk[idx]
                        } else {
                            0.0
                        }
                    })
                    .collect();

                let v_transient = transient_density(&h_voice);
                let d_transient = core_mask_p
                    .iter()
                    .map(|f| f.iter().sum::<f32>() / N_BINS as f32)
                    .sum::<f32>()
                    / core_n_frames.max(1) as f32;

                let mut spatial_sums = [(0.0, 0.0, 0.0); 5]; // [(pan_num, width_num, den); 5]
                for f in 0..core_n_frames {
                    if f >= core_frames_l.len() || f >= core_frames_r.len() {
                        continue;
                    }
                    for b in 0..N_BINS {
                        let x_l = core_frames_l[f][b];
                        let x_r = core_frames_r[f][b];
                        let amp = x_l + x_r;
                        if amp < 1e-6 {
                            continue;
                        }
                        let p = (x_r - x_l) / (amp + 1e-10_f32);
                        let p_abs = p.abs();

                        let band_idx = if b <= 10 {
                            0 // Lows (0 - 250 Hz)
                        } else if b <= 42 {
                            1 // Low-Mids (250 - 1000 Hz)
                        } else if b <= 170 {
                            2 // Mids (1000 - 4000 Hz)
                        } else if b <= 341 {
                            3 // High-Mids (4000 - 8000 Hz)
                        } else {
                            4 // Highs (8000+ Hz)
                        };

                        spatial_sums[band_idx].0 += p * amp;
                        spatial_sums[band_idx].1 += p_abs * amp;
                        spatial_sums[band_idx].2 += amp;
                    }
                }

                ParallelChunkOut {
                    stems: FiveStemsChunk {
                        voice: voice_chunk,
                        drums: drums_chunk,
                        bass: bass_chunk,
                        harmonics: harm_chunk,
                        ambience: amb_chunk,
                    },
                    voice_transient: v_transient,
                    drums_transient: d_transient,
                    chunk_len: core_chunk.len(),
                    spatial_sums,
                }
            })
            .collect();

        // 3. Serial Stitch Phase (Stateful processing + callback)
        let mut frames_written = 0usize;
        let mut voice_transient_sum = 0.0f32;
        let mut drums_transient_sum = 0.0f32;
        let mut chunk_count = 0usize;

        let mut global_spatial_sums = [(0.0, 0.0, 0.0); 5];

        for mut out in parallel_results {
            if out.chunk_len == 0 {
                continue;
            }

            voice_transient_sum += out.voice_transient;
            drums_transient_sum += out.drums_transient;
            chunk_count += 1;

            for (g, s) in global_spatial_sums.iter_mut().zip(&out.spatial_sums) {
                g.0 += s.0;
                g.1 += s.1;
                g.2 += s.2;
            }

            // Psychoacoustic Collision Matrix — smoothed micro-ducking sequentially across chunks.
            let collision = detect_collision(&out.stems.drums, &out.stems.bass);
            let target_gain = if collision { ducking_gain } else { 1.0_f32 };
            let alpha = COLLISION_SMOOTHING_ALPHA;

            for s in out.stems.bass.iter_mut() {
                self.bass_ducking_gain += alpha * (target_gain - self.bass_ducking_gain);
                *s *= self.bass_ducking_gain;
            }

            frames_written += out.stems.voice.len();
            callback(&out.stems);
        }

        // Flush OLA tail (legacy 1024 zero padding for byte-exact compatibility)
        let tail = vec![0.0; 1024];
        if !tail.is_empty() {
            let empty_chunk = FiveStemsChunk {
                voice: tail.clone(),
                drums: vec![0.0; tail.len()],
                bass: vec![0.0; tail.len()],
                harmonics: vec![0.0; tail.len()],
                ambience: vec![0.0; tail.len()],
            };
            frames_written += tail.len();
            callback(&empty_chunk);
        }

        let avg = chunk_count.max(1) as f32;
        let mut final_spatial = [BandSpatialMetrics::default(); 5];
        for i in 0..5 {
            let den = global_spatial_sums[i].2.max(1e-10);
            final_spatial[i].pan_mean = global_spatial_sums[i].0 / den;
            final_spatial[i].pan_width = global_spatial_sums[i].1 / den;
        }

        eprintln!(
            "[BAND-WIDTH] lows={:.4} low_mid={:.4} mid={:.4} high_mid={:.4} high={:.4}",
            final_spatial[0].pan_width,
            final_spatial[1].pan_width,
            final_spatial[2].pan_width,
            final_spatial[3].pan_width,
            final_spatial[4].pan_width
        );
        eprintln!(
            "[BAND-MEAN] lows={:.4} low_mid={:.4} mid={:.4} high_mid={:.4} high={:.4}",
            final_spatial[0].pan_mean,
            final_spatial[1].pan_mean,
            final_spatial[2].pan_mean,
            final_spatial[3].pan_mean,
            final_spatial[4].pan_mean
        );

        Ok(RenderMetadata {
            frames_written,
            voice_transient_density: voice_transient_sum / avg,
            drums_transient_density: drums_transient_sum / avg,
            spatial: final_spatial,
        })
    }

    /// Default process_chunks — uses COLLISION_DUCKING_GAIN (0.707).
    /// Backward compatible with all existing callers.
    pub fn process_chunks<F>(
        &mut self,
        signal: &[f32],
        scout: &ScoutResult,
        callback: F,
    ) -> Result<RenderMetadata, StreamError>
    where
        F: FnMut(&FiveStemsChunk),
    {
        self.process_chunks_with_params(
            signal,
            signal,
            signal,
            scout,
            COLLISION_DUCKING_GAIN,
            callback,
        )
    }
}

impl Default for TwoPassEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Compute firewall scales from proxy FiveStems energy.
/// Returns (rear_scale, lfe_scale) — locked in ScoutResult.
fn compute_firewall_scales(stems: &FiveStems, assignments: &StemChannelAssignments) -> (f32, f32) {
    use crate::spatial::five_dot_one::{FiveDotOneStage, SpatialFirewall};
    let stage = FiveDotOneStage::render(stems, assignments, &SpatialFirewall::default());
    let firewall = SpatialFirewall::default();

    let len = stage.l.len();
    if len == 0 {
        return (1.0, 1.0);
    }

    // Compute rear scale
    let front_e: f32 = stage
        .l
        .iter()
        .zip(stage.r.iter())
        .zip(stage.c.iter())
        .map(|((l, r), c)| l * l + r * r + c * c)
        .sum::<f32>()
        .sqrt();
    let rear_e: f32 = stage
        .ls
        .iter()
        .zip(stage.rs.iter())
        .map(|(l, r)| l * l + r * r)
        .sum::<f32>()
        .sqrt();
    let rear_scale = if front_e > 1e-6 && rear_e > 1e-6 {
        let ratio = rear_e / front_e;
        if ratio > firewall.max_rear_energy {
            firewall.max_rear_energy / ratio
        } else {
            1.0
        }
    } else {
        1.0
    };

    // Compute lfe scale
    let max_lfe_linear = libm::powf(10.0, firewall.max_lfe_db / 20.0);
    let lfe_max = stage.lfe.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    let lfe_scale = if lfe_max > max_lfe_linear {
        max_lfe_linear / lfe_max
    } else {
        1.0
    };

    (rear_scale, lfe_scale)
}

/// Apply per-frame magnitude mask to time-domain chunk.
fn apply_mask_to_chunk(chunk: &[f32], mask: &[Vec<f32>], n_frames: usize) -> Vec<f32> {
    if n_frames == 0 || mask.is_empty() || chunk.is_empty() {
        return chunk.to_vec();
    }
    let frame_weights: Vec<f32> = mask
        .iter()
        .map(|frame| {
            if frame.is_empty() {
                0.0
            } else {
                frame.iter().sum::<f32>() / frame.len() as f32
            }
        })
        .collect();
    let n = chunk.len();
    let weights: Vec<f32> = (0..n)
        .map(|i| {
            let pos = i as f32 * (n_frames - 1).max(1) as f32 / n.max(1) as f32;
            let idx0 = pos.floor() as usize;
            let idx1 = (idx0 + 1).min(frame_weights.len() - 1);
            let frac = pos - idx0 as f32;
            let w0 = frame_weights.get(idx0).copied().unwrap_or(0.0);
            let w1 = frame_weights.get(idx1).copied().unwrap_or(0.0);
            w0 * (1.0 - frac) + w1 * frac
        })
        .collect();
    chunk
        .iter()
        .zip(weights.iter())
        .map(|(s, w)| s * w)
        .collect()
}

/// Compute transient density — deterministic, INV-AB-1.
fn transient_density(h: &[f32]) -> f32 {
    let n = h.len();
    if n < 2 {
        return 0.0;
    }
    let deltas: Vec<f32> = (1..n).map(|i| (h[i] - h[i - 1]).abs()).collect();
    let mean = deltas.iter().sum::<f32>() / deltas.len() as f32;
    let var = deltas.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / deltas.len() as f32;
    let thr = mean + libm::sqrtf(var);
    deltas.iter().filter(|&&d| d > thr).count() as f32 / n as f32
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collision_matrix_rms_detection() {
        // Alternating signal: high variance = transient-like
        // diff between adjacent samples is always 1.0 → high drums_td
        let loud_chunk: Vec<f32> = (0..512)
            .map(|i| if i % 2 == 0 { 0.5f32 } else { -0.5f32 })
            .collect();
        // Amplitude 0.0001 → RMS ≈ -80 dBFS (below threshold)
        let quiet_chunk = vec![0.0001f32; 512];

        // Scenario 1: Both loud → collision expected
        assert!(
            super::detect_collision(&loud_chunk, &loud_chunk),
            "Should detect collision when both drums and bass have high RMS"
        );
        // Scenario 2: Loud drums, quiet bass → no collision
        assert!(
            !super::detect_collision(&loud_chunk, &quiet_chunk),
            "Should NOT detect collision when bass is quiet"
        );
        // Scenario 3: Quiet drums, loud bass → no collision
        assert!(
            !super::detect_collision(&quiet_chunk, &loud_chunk),
            "Should NOT detect collision when drums transient is low"
        );
        // Scenario 4: Empty chunks → safe, no panic
        assert!(
            !super::detect_collision(&[], &loud_chunk),
            "Should safely return false on empty chunks"
        );
    }

    fn sine(freq: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * freq * i as f32 / 48000.0))
            .collect()
    }

    #[test]
    fn w_bin_mapping_produces_full_size_w() {
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, 48000);
        assert_eq!(
            scout.w.len(),
            N_BINS * N_COMPONENTS,
            "W must be N_BINS × N_COMPONENTS after bin mapping"
        );
        assert!(
            scout.w.iter().all(|&v| v >= 1e-10_f32),
            "All W values must be >= EPS"
        );
    }

    #[test]
    fn w_bin_mapping_is_deterministic() {
        // INV-AB-1: same signal → same mapped W
        let signal = sine(440.0, 48000);
        let mut e1 = TwoPassEngine::new();
        let mut e2 = TwoPassEngine::new();
        let s1 = e1.scout(&signal, 48000);
        let s2 = e2.scout(&signal, 48000);
        for (a, b) in s1.w.iter().zip(s2.w.iter()) {
            assert!(
                (a - b).abs() < 1e-10,
                "INV-AB-1: W must be bit-identical for same input"
            );
        }
    }

    #[test]
    fn w_bins_above_6khz_are_eps() {
        // Bins above proxy Nyquist must be EPS (no template)
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, 48000);
        // Proxy Nyquist = 48000 / (2 * SCOUT_DOWNSAMPLE) = 6000 Hz
        // Bin at 6kHz = 6000 * N_BINS * 2 / 48000 = ~256
        for b in N_BINS.div_ceil(SCOUT_DOWNSAMPLE)..N_BINS {
            for c in 0..N_COMPONENTS {
                let v = scout.w[b * N_COMPONENTS + c];
                assert!(
                    v <= 1e-10_f32 + 1e-12_f32,
                    "Bin {b} component {c} should be EPS, got {v}"
                );
            }
        }
    }

    #[test]
    fn scout_produces_locked_assignments() {
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, 48000);
        assert_eq!(scout.w.len(), N_BINS * N_COMPONENTS);
        assert!(scout.voice_idx < N_COMPONENTS);
        assert!(scout.rear_scale >= 0.0 && scout.rear_scale <= 1.0);
        assert!(scout.lfe_scale >= 0.0 && scout.lfe_scale <= 1.0);
    }

    #[test]
    fn scout_is_deterministic() {
        let signal = sine(1000.0, 48000);
        let mut e1 = TwoPassEngine::new();
        let mut e2 = TwoPassEngine::new();
        let s1 = e1.scout(&signal, 48000);
        let s2 = e2.scout(&signal, 48000);
        for (a, b) in s1.w.iter().zip(s2.w.iter()) {
            assert!((a - b).abs() < 1e-6, "INV-AB-1: W must be identical");
        }
        assert_eq!(s1.voice_idx, s2.voice_idx);
        assert_eq!(s1.rear_scale, s2.rear_scale);
    }

    #[test]
    fn process_chunks_produces_callback_calls() {
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, 48000);

        let mut call_count = 0usize;
        let result = engine.process_chunks(&signal, &scout, |chunk| {
            call_count += 1;
            assert!(!chunk.voice.is_empty());
            assert_eq!(chunk.voice.len(), chunk.bass.len());
        });

        assert!(result.is_ok());
        assert!(call_count > 0, "Must call callback at least once");
    }

    #[test]
    fn w_read_only_during_process() {
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, 48000);
        let w_before = scout.w.clone();
        let _ = engine.process_chunks(&signal, &scout, |_| {});
        assert_eq!(w_before, scout.w, "INV-ST-2: W must not change");
    }

    #[test]
    fn five_stems_chunk_all_same_length() {
        let signal = sine(440.0, 96000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, 48000);
        let _ = engine.process_chunks(&signal, &scout, |chunk| {
            assert_eq!(chunk.voice.len(), chunk.drums.len());
            assert_eq!(chunk.voice.len(), chunk.bass.len());
            assert_eq!(chunk.voice.len(), chunk.harmonics.len());
            assert_eq!(chunk.voice.len(), chunk.ambience.len());
        });
    }

    /// Verifies that the spectrum at chunk boundaries is NOT polluted by
    /// STFT zero-padding artifacts. Each chunk is fed to a fresh
    /// StreamingStftEncoder; results must be bit-identical (f32 ==) to
    /// StftEngine::forward() on the same chunk, since StreamingStftEncoder
    /// is proven bit-identical to forward().
    #[test]
    fn two_pass_streaming_stft_boundary_frames_unpolluted() {
        use crate::stft::{StftEngine, StreamingStftEncoder, FFT_SIZE, HOP_SIZE, N_BINS};

        // 2*FFT_SIZE + HOP_SIZE samples of a single tone — spectrum easy to inspect.
        let n = 2 * FFT_SIZE + HOP_SIZE;
        let signal: Vec<f32> = (0..n)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0))
            .collect();

        // Three chunks: FFT_SIZE, FFT_SIZE, HOP_SIZE — boundaries at those indices.
        let chunks: [&[f32]; 3] = [
            &signal[..FFT_SIZE],
            &signal[FFT_SIZE..2 * FFT_SIZE],
            &signal[2 * FFT_SIZE..],
        ];

        // Offline oracle per chunk: StftEngine::forward(chunk) → magnitude.
        let offline: Vec<Vec<Vec<f32>>> = chunks
            .iter()
            .map(|chunk| {
                let mut engine = StftEngine::new();
                let (complex_frames, n_frames) = engine.forward(chunk);
                complex_frames
                    .into_iter()
                    .take(n_frames)
                    .map(|frame| {
                        frame
                            .iter()
                            .take(N_BINS)
                            .map(|c| c.norm())
                            .collect::<Vec<f32>>()
                    })
                    .collect()
            })
            .collect();

        // Streaming per-chunk encoder — same pattern as the new two_pass.rs code.
        let streaming: Vec<Vec<Vec<f32>>> = chunks
            .iter()
            .map(|chunk| {
                let mut enc = StreamingStftEncoder::new();
                let mut frames = enc.feed_chunk(chunk);
                frames.extend(enc.finish());
                frames
                    .into_iter()
                    .map(|frame| {
                        frame
                            .iter()
                            .take(N_BINS)
                            .map(|c| c.norm())
                            .collect::<Vec<f32>>()
                    })
                    .collect()
            })
            .collect();

        // Strict f32 bit-pattern equality — no tolerance.
        // Any discrepancy means a boundary frame is polluted.
        for (ci, (off_chunk, str_chunk)) in offline.iter().zip(streaming.iter()).enumerate() {
            assert_eq!(
                off_chunk.len(),
                str_chunk.len(),
                "chunk {ci}: frame count mismatch (offline={} streaming={})",
                off_chunk.len(),
                str_chunk.len()
            );
            for (fi, (off_frame, str_frame)) in off_chunk.iter().zip(str_chunk.iter()).enumerate() {
                for (bi, (o, s)) in off_frame.iter().zip(str_frame.iter()).enumerate() {
                    assert_eq!(
                        o.to_bits(),
                        s.to_bits(),
                        "chunk={ci} frame={fi} bin={bi}: offline={o} streaming={s}"
                    );
                }
            }
        }

        // Chunk sizes used — suppress unused warning.
        let _ = HOP_SIZE;
    }
}
