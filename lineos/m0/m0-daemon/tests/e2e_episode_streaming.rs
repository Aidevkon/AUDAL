//! Episode streaming render — proof tests.
//!
//! These verify the FIRST FLOOR of the
//! streaming building: episode_render itself.
//! They call episode_render::run() DIRECTLY,
//! bypassing decode_node — so they measure our
//! new layer in isolation.
//!
//! The SECOND FLOOR (decode_node streaming) is
//! not built yet: decode_node still loads the
//! whole file into RAM before the pipeline
//! reaches episode_render. The full-pipeline
//! RAM test at the bottom is #[ignore]d and
//! documents exactly that gap.

use std::path::Path;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

// ── fixtures ──────────────────────────────
fn generate_podcast_fixture(sr: u32, dur_secs: f32) -> Vec<f32> {
    // Interleaved stereo: voice-like tone +
    // low noise floor. Content doesn't matter
    // for these tests — only length + validity.
    let n = (sr as f32 * dur_secs) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        // 150 Hz fundamental + harmonic,
        // amplitude ~ -18 dBFS
        let v = 0.12 * (2.0 * std::f32::consts::PI * 150.0 * t).sin()
            + 0.04 * (2.0 * std::f32::consts::PI * 450.0 * t).sin();
        out.push(v); // L
        out.push(v); // R (mono-ish voice)
    }
    out
}

fn write_wav(samples: &[f32], sr: u32, path: &str) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for &s in samples {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
}

// Build the first-floor render in isolation.
fn render_episode(
    wav_path: &str,
    blob_id: &str,
) -> m0d::domain::episode_render::EpisodeRenderResult {
    let intent = lineos_types::mastering::MasteringIntent::podcast();
    let dummy_scout = vec![0.0f32; 4800];
    let graph = m0d::dsp::DspAdapter::build_graph_only(
        &intent,
        &dummy_scout,
        &dummy_scout,
        48_000,
        &[0.2f32; 5],
        None,
        512,
    )
    .expect("graph build");
    let pre = lineos_types::pre_analysis::PreAnalysisData::silent();
    let mut source =
        m0d::dsp::lazy_reader::LazyAudioReader::open(Path::new(wav_path)).expect("open source");
    m0d::domain::episode_render::run(
        &mut source,
        blob_id,
        graph,
        -16.0,
        &pre,
        // No-op health hook: this test exercises the
        // O(1) streaming heap, not the early-abort
        // path. The hook must not affect memory.
        |_| Ok(()),
    )
    .expect("episode_render")
}

// ── TEST 1: first floor produces a valid
//    master ───────────────────────────────
#[test]
fn episode_render_produces_valid_master() {
    let sr = 48_000;
    let wav = "/tmp/ep_valid.wav";
    write_wav(&generate_podcast_fixture(sr, 5.0), sr, wav);

    let res = render_episode(wav, "ep-valid");

    // Produced audio
    assert!(res.frames_written > 0, "no frames written");
    // Certified hashes are present + non-empty
    assert!(!res.pcm_blake3.is_empty(), "empty blake3");
    assert_eq!(res.pcm_blake3.len(), 64, "blake3 hex should be 64 chars");
    assert!(!res.output_sha256.is_empty(), "empty sha256");
    assert_eq!(res.output_sha256.len(), 64, "sha256 hex should be 64 chars");
    // Loudness landed near the -16 target
    // (spoken-word). Wide tolerance — the
    // point is "the DSP ran", not exactness.
    assert!(
        res.output_lufs > -20.0 && res.output_lufs < -12.0,
        "output LUFS {} not near -16 target",
        res.output_lufs
    );
    // True peak respected the -1 dBTP ceiling
    // (small epsilon for measurement).
    assert!(
        res.true_peak_dbtp <= -0.5,
        "true peak {} exceeded ceiling",
        res.true_peak_dbtp
    );

    // --- NEO REGRESSION TEST BLOCK ---
    // 1. Διάβασμα των ΠΡΑΓΜΑΤΙΚΩΝ δειγμάτων (PCM) από το export του test
    let exported_bytes = std::fs::read(&res.pcm_path).expect("Failed to read PCM from disk");
    let mut actual_left = Vec::with_capacity(res.frames_written);
    let mut actual_right = Vec::with_capacity(res.frames_written);

    // Το αρχείο είναι f32 LE, interleaved. Διαβάζουμε με ασφάλεια (safe Rust).
    for chunk in exported_bytes.chunks_exact(8) {
        let l_bytes: [u8; 4] = chunk[0..4].try_into().unwrap();
        let r_bytes: [u8; 4] = chunk[4..8].try_into().unwrap();
        actual_left.push(f32::from_le_bytes(l_bytes));
        actual_right.push(f32::from_le_bytes(r_bytes));
    }

    // 2. Ανεξάρτητη μέτρηση στο τελικό Exported PCM
    let measured_pcm_lufs =
        sp314_dsp::metering::lufs::measure_integrated_lufs(&actual_left, &actual_right);

    // 3. Σύγκριση (Το Certificate/Output LUFS πρέπει να είναι απόλυτα ταυτόσημο με το PCM)
    let diff = (res.output_lufs - measured_pcm_lufs).abs();
    assert!(
        diff < 0.5,
        "FALSE ATTESTATION: Certificate says {} LUFS, but actual file is {} LUFS (diff: {})",
        res.output_lufs,
        measured_pcm_lufs,
        diff
    );
}

// ── TEST 2: first floor is deterministic
//    (certifiable) ─────────────────────────
#[test]
fn episode_render_is_deterministic() {
    let sr = 48_000;
    let wav = "/tmp/ep_determ.wav";
    write_wav(&generate_podcast_fixture(sr, 5.0), sr, wav);

    let a = render_episode(wav, "ep-det-a");
    let b = render_episode(wav, "ep-det-b");

    // Same audio in → same certified hashes.
    // This is the trust MOAT: the streaming
    // certificate is reproducible.
    assert_eq!(a.pcm_blake3, b.pcm_blake3, "blake3 not deterministic");
    assert_eq!(a.output_sha256, b.output_sha256, "sha256 not deterministic");
}

// ── TEST 3: first floor RAM is bounded
//    (scale-invariant heap) ────────────────
#[test]
fn episode_render_heap_is_scale_invariant() {
    let sr = 48_000;

    let wav_1m = "/tmp/ep_ram_1m.wav";
    let wav_2m = "/tmp/ep_ram_2m.wav";
    write_wav(&generate_podcast_fixture(sr, 60.0), sr, wav_1m);
    write_wav(&generate_podcast_fixture(sr, 120.0), sr, wav_2m);

    let peak_1m = {
        let _p = dhat::Profiler::builder().testing().build();
        let _ = render_episode(wav_1m, "ram-1m");
        dhat::HeapStats::get().max_bytes
    };

    let peak_2m = {
        let _p = dhat::Profiler::builder().testing().build();
        let _ = render_episode(wav_2m, "ram-2m");
        dhat::HeapStats::get().max_bytes
    };

    let diff = peak_2m as i64 - peak_1m as i64;
    let diff_mb = diff as f64 / 1_000_000.0;

    eprintln!(
        "episode_render heap: 1m={:.2}MB \
         2m={:.2}MB diff={:.2}MB",
        peak_1m as f64 / 1e6,
        peak_2m as f64 / 1e6,
        diff_mb
    );

    // The whole point: doubling the DURATION
    // must NOT double the HEAP. episode_render
    // works in fixed CHUNK_FRAMES buffers +
    // mmap (mmap is file-backed, not heap), so
    // heap should stay flat regardless of
    // length. Allow a small margin for
    // allocator noise.
    assert!(
        diff_mb.abs() < 5.0,
        "heap grew {:.2}MB when duration \
         doubled — first floor is NOT \
         scale-invariant",
        diff_mb
    );
}

// ── TEST 4 (IGNORED): full pipeline RAM
//    — documents the SECOND FLOOR gap ───────
//
// decode_node still loads the entire file into
// RAM before episode_render runs, so the full
// pipeline is O(N) in heap even though the
// render layer is O(1). This test is left
// #[ignore]d as executable documentation of
// the remaining work: streaming decode.
//
// When decode streaming lands (Phase 8 second
// floor), remove #[ignore] — it should pass.
#[test]
fn full_pipeline_heap_is_scale_invariant() {
    let sr = 48_000;

    let wav_1m = "/tmp/ep_full_1m.wav";
    let wav_2m = "/tmp/ep_full_2m.wav";
    write_wav(&generate_podcast_fixture(sr, 60.0), sr, wav_1m);
    write_wav(&generate_podcast_fixture(sr, 120.0), sr, wav_2m);

    let run_pipeline = |path: &str, id: &str| {
        let req = m0d::handlers::master::MasterRequest {
            audio_path: path.to_string(),
            preset_id: "podcast".to_string(),
            flavour_id: None,
            intent_tone: None,
            intent_dynamics: None,
            persona_id: None,
            tone: None,
            dynamics: None,
            chaos_seed: None,
            project_id: Some("default".to_string()),
            track_id: Some(id.to_string()),
            mix_levels: None,
            preview_id: None,
        restoration_enabled: None,
    };
        let state = std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
            xaak::repo::DspState::default(),
        ));
        let state_tmp = tempfile::TempDir::new().unwrap();
        m0d::domain::dsp_pipeline::run_dsp(
            &req,
            std::time::Instant::now(),
            state,
            None,
            None,
            id.to_string(),
            state_tmp.path().to_str().unwrap(),
        )
        .expect("run_dsp failed")
    };

    let peak_1m = {
        let _p = dhat::Profiler::builder().testing().build();
        let _ = run_pipeline(wav_1m, "run-1m");
        dhat::HeapStats::get().max_bytes
    };

    let peak_2m = {
        let _p = dhat::Profiler::builder().testing().build();
        let _ = run_pipeline(wav_2m, "run-2m");
        dhat::HeapStats::get().max_bytes
    };

    let diff = peak_2m as i64 - peak_1m as i64;
    let diff_mb = diff as f64 / 1_000_000.0;

    eprintln!(
        "full_pipeline heap: 1m={:.2}MB \
         2m={:.2}MB diff={:.2}MB",
        peak_1m as f64 / 1e6,
        peak_2m as f64 / 1e6,
        diff_mb
    );

    assert!(
        diff_mb.abs() < 5.0,
        "heap grew {:.2}MB when duration \
         doubled — full pipeline is NOT \
         scale-invariant",
        diff_mb
    );
}

#[test]
#[ignore = "PASSING since A3 Steps 3a/3b (scratch mmaps + F-043 borrow fix): \
1m=149.98MB 2m=149.98MB diff=0.00MB. Ignored for COST only (60s dhat \
run), like the seam test — run explicitly via --ignored (no CI target runs it \
yet — see register P36)."]
fn music_pipeline_heap_is_scale_invariant() {
    let sr = 48_000;

    let wav_1m = "/tmp/music_full_1m.wav";
    let wav_2m = "/tmp/music_full_2m.wav";
    write_wav(&generate_podcast_fixture(sr, 60.0), sr, wav_1m);
    write_wav(&generate_podcast_fixture(sr, 120.0), sr, wav_2m);

    let run_pipeline = |path: &str, id: &str| {
        let req = m0d::handlers::master::MasterRequest {
            audio_path: path.to_string(),
            preset_id: "spotify".to_string(),
            flavour_id: None,
            intent_tone: None,
            intent_dynamics: None,
            persona_id: None,
            tone: None,
            dynamics: None,
            chaos_seed: None,
            project_id: Some("default".to_string()),
            track_id: Some(id.to_string()),
            mix_levels: None,
            preview_id: None,
        restoration_enabled: None,
    };
        let state = std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
            xaak::repo::DspState::default(),
        ));
        let state_tmp = tempfile::TempDir::new().unwrap();
        m0d::domain::dsp_pipeline::run_dsp(
            &req,
            std::time::Instant::now(),
            state,
            None,
            None,
            id.to_string(),
            state_tmp.path().to_str().unwrap(),
        )
        .expect("run_dsp failed")
    };

    let peak_1m = {
        let _p = dhat::Profiler::builder().testing().build();
        let _ = run_pipeline(wav_1m, "run-music-1m");
        dhat::HeapStats::get().max_bytes
    };

    let peak_2m = {
        let _p = dhat::Profiler::builder().testing().build();
        let _ = run_pipeline(wav_2m, "run-music-2m");
        dhat::HeapStats::get().max_bytes
    };

    let diff = peak_2m as i64 - peak_1m as i64;
    let diff_mb = diff as f64 / 1_000_000.0;

    eprintln!(
        "music_pipeline heap (spotify): 1m={:.2}MB \
         2m={:.2}MB diff={:.2}MB",
        peak_1m as f64 / 1e6,
        peak_2m as f64 / 1e6,
        diff_mb
    );

    assert!(
        diff_mb.abs() < 5.0,
        "heap grew {:.2}MB when duration \
         doubled — music pipeline is NOT \
         scale-invariant",
        diff_mb
    );
}
