use blake3::Hasher as Blake3Hasher;
use memmap2::MmapMut;
use sha2::{Digest, Sha256};
use sp314_dsp::limiter::{BrickwallLimiter, LimiterConfig};
use sp314_dsp::metering::LufsMeter;
use std::fs::OpenOptions;

use lineos_types::pre_analysis::PreAnalysisData;

const CHUNK_FRAMES: usize = 4096;
/// The DspGraph is built with a fixed
/// block_size of 512 (see build_graph_only).
/// process_block() asserts input length
/// <= block_size, so each CHUNK_FRAMES read
/// is fed to the graph in GRAPH_BLOCK-sized
/// sub-blocks. The graph keeps its own
/// filter/envelope state across calls, so
/// sub-blocking is bit-identical to one big
/// call (proven by the determinism test).
const GRAPH_BLOCK: usize = 512;
/// Lookahead flush for BrickwallLimiter
/// (5ms @ 48kHz = 240 samples).
const LIMITER_FLUSH_FRAMES: usize = 240;

pub struct EpisodeRenderResult {
    pub pcm_path: std::path::PathBuf,
    /// BLAKE3 of the left channel (f32 LE)
    /// — feeds the signed .m0sig, matches
    /// certificate::blake3_pcm().
    pub pcm_blake3: String,
    /// SHA-256 of interleaved L+R (f32 BE)
    /// — feeds the execution certificate,
    /// matches ExecutionProof::hash_pcm().
    pub output_sha256: String,
    pub frames_written: usize,
    pub sample_rate: u32,
    /// Measured output LUFS (post-DSP,
    /// post-gain-correction).
    pub output_lufs: f32,
    /// True peak after limiting (dBTP).
    pub true_peak_dbtp: f32,
    pub output_lra: f32,
    // TODO(wave-2): noise_floor_db + apple_podcasts_
    // compliant. (LRA now wired: streaming LraCalculator,
    // EBU Tech 3342.)
}

/// Stream-render an Episode/podcast file
/// with bounded RAM.
///
/// Three logical passes, two disk reads:
///   Pass 1 (external): scout 30s →
///     pre_analysis (caller's job)
///   Pass 2 (here): LazyReader →
///     DspGraph chunks → LufsMeter
///     → mmap write
///   Pass 3 (here): mmap re-read →
///     gain correction + BrickwallLimiter
///     → mmap in-place + BLAKE3
///
/// Peak RAM: O(CHUNK_FRAMES) ≈ 32 KB
/// regardless of file duration.
pub fn run<S: crate::dsp::audio_source::AudioSource, F>(
    source: &mut S,
    blob_id: &str,
    mut graph: sp314_nodes::graph::DspGraph,
    // None σημαίνει ΜΗΝ ΑΓΓΙΞΕΙΣ ΤΗ ΣΤΑΘΜΗ, όχι
    // "χρησιμοποίησε default". Το "raw" preset έχει
    // target_lufs: null στο schema ΡΗΤΑ — υπάρχει για
    // να μην κανονικοποιεί τίποτα.
    //
    // ΗΤΑΝ f32 με τον caller να κάνει unwrap_or(-16.0),
    // που ένωνε το ρητό null με το άγνωστο preset. Το
    // raw έβγαινε στα -16.
    target_lufs: Option<f32>,
    _pre_analysis: &PreAnalysisData,
    mut progress_hook: F,
) -> Result<EpisodeRenderResult, String>
where
    F: FnMut(&S) -> Result<(), String>,
{
    // Source is any AudioSource: a raw
    // LazyAudioReader, a resampling
    // StandardizedAudioStream, or a test mock.
    let sample_rate = source.sample_rate();
    let channels = source.channels();

    let total_frames = source.total_frames_hint().ok_or_else(|| {
        format!(
            "episode_render {}: \
             no frame count in metadata",
            blob_id
        )
    })? as usize;

    // ── Pre-allocate output PCM file ──
    let pcm_path = crate::blob_store::mastering_path(blob_id);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&pcm_path)
        .map_err(|e| format!("episode_render create: {e}"))?;
    file.set_len((total_frames * 2 * 4) as u64)
        .map_err(|e| format!("episode_render set_len: {e}"))?;
    let mut mmap =
        unsafe { MmapMut::map_mut(&file) }.map_err(|e| format!("episode_render mmap: {e}"))?;

    // ── Chunk buffers (reused) ────────
    let buf_len = CHUNK_FRAMES * channels;
    let mut interleaved = vec![0f32; buf_len];
    let mut left_buf = vec![0f32; CHUNK_FRAMES];
    let mut right_buf = vec![0f32; CHUNK_FRAMES];

    // ── PASS 2: DSP + measure ─────────
    let mut lufs_meter = LufsMeter::new();
    let mut lra_calc = lineos_telemetry::lra::LraCalculator::new(sample_rate);
    let mut peak_linear = 0f32;
    let mut frames_written = 0usize;

    {
        let mmap_f32: &mut [f32] = unsafe {
            std::slice::from_raw_parts_mut(mmap.as_mut_ptr() as *mut f32, total_frames * 2)
        };

        loop {
            let frames = source
                .fill_buffer(&mut interleaved)
                .map_err(|e| format!("episode_render read: {e}"))?;
            if frames == 0 {
                break;
            }
            // Progress hook (Wave 2 §2): fill_buffer
            // has already fed this chunk to the health
            // monitor, so the caller sees fresh data.
            // If the hook returns Err (e.g. tier-1
            // digital-silence abort once the early
            // window has filled), fail fast — skip the
            // remaining DSP and the rest of the file.
            progress_hook(source)?;

            // De-interleave
            for i in 0..frames {
                left_buf[i] = interleaved[i * channels];
                right_buf[i] = if channels >= 2 {
                    interleaved[i * channels + 1]
                } else {
                    left_buf[i]
                };
            }

            // DSP (stateful, maintains history
            // between calls). Fed in GRAPH_BLOCK
            // sub-blocks because process_block()
            // asserts len <= graph block_size.
            let mut f = 0;
            while f < frames {
                let e = (f + GRAPH_BLOCK).min(frames);
                graph.process_block(&mut left_buf[f..e], &mut right_buf[f..e]);
                f = e;
            }

            // Running LUFS + peak
            lufs_meter.process_chunk(&left_buf[..frames], &right_buf[..frames]);
            lra_calc.process_chunk(&left_buf[..frames], &right_buf[..frames]);
            for i in 0..frames {
                peak_linear = peak_linear.max(left_buf[i].abs()).max(right_buf[i].abs());
            }

            // Write to mmap
            let base = frames_written * 2;
            for i in 0..frames {
                mmap_f32[base + i * 2] = left_buf[i];
                mmap_f32[base + i * 2 + 1] = right_buf[i];
            }
            frames_written += frames;
        }
    }

    // LUFS from streaming meter (fallback to -144.0 if silence and no target)
    let output_lufs = lufs_meter.finish().unwrap_or(target_lufs.unwrap_or(-144.0));
    let output_lra = lra_calc.compute();

    // Static gain correction (dB → linear)
    // ΥΠΟ ΣΥΝΘΗΚΗ: αν None, καμία διόρθωση στάθμης (1.0).
    let correction_linear = match target_lufs {
        Some(t) => libm::powf(10.0_f32, (t - output_lufs) / 20.0),
        None => 1.0,
    };

    // ── PASS 3: gain + limit + hash ───
    let mut final_lufs_meter = LufsMeter::new();
    let limiter_config = LimiterConfig {
        release_ms: 15.0,
        blend_release_ms: 95.0,
        ceiling_db: -1.0,
        midside_eq_enabled: false,
        true_peak_enabled: true,
    };
    let mut limiter = BrickwallLimiter::new(limiter_config, sample_rate);
    let mut blake3 = Blake3Hasher::new();
    // Interleaved L+R, BIG ENDIAN — matches
    // ExecutionProof::hash_pcm() so the
    // execution certificate verifies. This
    // is a DIFFERENT encoding from the blake3
    // above (left mono, LE). Both required.
    let mut sha256 = Sha256::new();
    let mut true_peak_linear = 0f32;

    {
        let total_with_flush = frames_written + LIMITER_FLUSH_FRAMES;
        let mmap_f32: &mut [f32] = unsafe {
            std::slice::from_raw_parts_mut(mmap.as_mut_ptr() as *mut f32, total_frames * 2)
        };

        let mut pos = 0usize;
        while pos < total_with_flush {
            let chunk = (CHUNK_FRAMES).min(total_with_flush - pos);

            // Read from mmap (or zeros
            // for limiter flush tail)
            for i in 0..chunk {
                let src = pos + i;
                if src < frames_written {
                    left_buf[i] = mmap_f32[src * 2] * correction_linear;
                    right_buf[i] = mmap_f32[src * 2 + 1] * correction_linear;
                } else {
                    // Flush zeros
                    left_buf[i] = 0.0;
                    right_buf[i] = 0.0;
                }
            }

            // Limit
            limiter.process_block(&mut left_buf[..chunk], &mut right_buf[..chunk]);

            // Real (post-gain/limiter) LUFS measurement
            // We only measure the actual frames, not the limiter flush tail
            let valid_frames = chunk.min(frames_written.saturating_sub(pos));
            if valid_frames > 0 {
                final_lufs_meter
                    .process_chunk(&left_buf[..valid_frames], &right_buf[..valid_frames]);
            }

            // Peak after limiting
            for i in 0..chunk {
                true_peak_linear = true_peak_linear
                    .max(left_buf[i].abs())
                    .max(right_buf[i].abs());
            }

            // Write back in-place
            // (only real frames, not flush)
            for i in 0..chunk {
                let dst = pos + i;
                if dst < frames_written {
                    // BLAKE3 certified hash
                    // (left channel, LE bytes
                    //  — matches blake3_pcm())
                    blake3.update(&left_buf[i].to_le_bytes());
                    // SHA-256 execution hash
                    // (interleaved L+R, BE bytes
                    //  — matches ExecutionProof::
                    //  hash_pcm()). Order must be
                    //  left-then-right to match
                    //  the interleaved layout.
                    sha256.update(left_buf[i].to_be_bytes());
                    sha256.update(right_buf[i].to_be_bytes());
                    mmap_f32[dst * 2] = left_buf[i];
                    mmap_f32[dst * 2 + 1] = right_buf[i];
                }
            }
            pos += chunk;
        }
    }

    mmap.flush()
        .map_err(|e| format!("episode_render flush: {e}"))?;

    let true_peak_dbtp = if true_peak_linear > 0.0 {
        20.0 * true_peak_linear.log10()
    } else {
        f32::NEG_INFINITY
    };

    Ok(EpisodeRenderResult {
        pcm_path,
        pcm_blake3: blake3.finalize().to_hex().to_string(),
        // Same {:x} formatting as
        // ExecutionProof::sha256_hex() so
        // the hex string matches byte-for-byte.
        output_sha256: format!("{:x}", sha256.finalize()),
        frames_written,
        sample_rate,
        output_lufs: final_lufs_meter.finish().unwrap_or(output_lufs),
        output_lra,
        true_peak_dbtp,
    })
}
