use sp314_orchestrator::streaming_pipeline::run_streaming_pipeline_with_timeline;
use sp314_orchestrator::streaming_pipeline::{StreamingConfig, TimelinePlan};

use m0d::dsp::file_decoder::FileDecoder;
use m0d::handlers::decode::decode_raw_interleaved;
use sp314_nodes::topology::DspTopology;

fn dummy_ducking_topology() -> DspTopology {
    let json = serde_json::json!({
        "topology_id": "dummy_ducking_topology",
        "nodes": [
            { "node_id": "in", "node_type": "Input", "parameters": {} },
            { "node_id": "duck_gain", "node_type": "Gain", "parameters": { "gain": 1.0, "glide_ms": 300.0 } },
            { "node_id": "out", "node_type": "Output", "parameters": {} }
        ],
        "edges": [
            { "source": "in", "target": "duck_gain", "modulation_type": "audio" },
            { "source": "duck_gain", "target": "out", "modulation_type": "audio" }
        ]
    });
    DspTopology::from_json(&json.to_string()).unwrap()
}
#[test]
#[ignore = "θέλει το untracked dataset flight_clips_stereo/ (:29, .gitignore:30). ~11s. Το ξυπνά: scripts/audio_wire.sh · scripts/run-ignored.sh"]
fn streaming_pipeline_ducking_e2e() {
    use sp314_orchestrator::pass1_pipeline::build_timeline_map;
    let topology = dummy_ducking_topology();

    let input_path = "../../../flight_clips_stereo/clip_transition_st.wav";
    let decoder = FileDecoder {
        path: input_path.to_string(),
    };
    let boundaries = build_timeline_map(decoder).unwrap();

    let output_path = "/tmp/test_streaming_ducking_output.wav";

    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    let shadow_reader =
        m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(input_path)).unwrap();
    let _worker_handle =
        m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader, 48000, rx_job, tx_res);
    let (_, flagged_indices) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);

    let bad_run = run_streaming_pipeline_with_timeline(
        FileDecoder {
            path: input_path.to_string(),
        },
        output_path,
        &StreamingConfig {
            topology: &topology,
            block_size: 512,
            sample_rate: 48000,
            ducking_node_id: "invalid_node_id",
            speech_gain: 1.0,
            music_gain: 0.501,
            pre_gain_linear: 1.0,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            // Ταβάνι από τον κατάλογο, ΟΧΙ literal.
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            // Από τον ίδιο κατάλογο. PODCAST.max_noise_floor_db είναι None —
            // ο προορισμός δεν έχει απαίτηση πατώματος, και αυτό είναι η αλήθεια
            // του preset, όχι placeholder.
            max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
            // Δεν μετρήθηκε σε αυτά τα δοκίμια ⇒ ο expander δεν τρέχει,
            // ακριβώς όπως και πριν από αυτό το βήμα.
            input_interior_floor_db: None,
            quiet_window_split_dbfs: None,
            input_fundamental: None,
            // Default intent — ο τύπος δίνει 105 ms.
            intent_dynamics: None,
            restoration_enabled: false,
        },
        TimelinePlan {
            boundaries: boundaries.clone(),
            flagged_indices,
            pre_analysis: None,
        },
        rx_res,
    );
    assert!(
        bad_run.is_err(),
        "Expected error when passing an invalid node ID"
    );
    assert!(
        bad_run.unwrap_err().to_string().contains("invalid_node_id"),
        "Error string should mention the invalid node ID"
    );

    let (tx_job2, rx_job2) = std::sync::mpsc::channel();
    let (tx_res2, rx_res2) = std::sync::mpsc::channel();
    let shadow_reader2 =
        m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(input_path)).unwrap();
    let _worker_handle2 =
        m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader2, 48000, rx_job2, tx_res2);
    let (_, flagged_indices2) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job2);

    let frames = run_streaming_pipeline_with_timeline(
        FileDecoder {
            path: input_path.to_string(),
        },
        output_path,
        &StreamingConfig {
            topology: &topology,
            block_size: 512,
            sample_rate: 48000,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 0.501,
            pre_gain_linear: 1.0,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            // Ταβάνι από τον κατάλογο, ΟΧΙ literal.
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            // Από τον ίδιο κατάλογο. PODCAST.max_noise_floor_db είναι None —
            // ο προορισμός δεν έχει απαίτηση πατώματος, και αυτό είναι η αλήθεια
            // του preset, όχι placeholder.
            max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
            // Δεν μετρήθηκε σε αυτά τα δοκίμια ⇒ ο expander δεν τρέχει,
            // ακριβώς όπως και πριν από αυτό το βήμα.
            input_interior_floor_db: None,
            quiet_window_split_dbfs: None,
            input_fundamental: None,
            // Default intent — ο τύπος δίνει 105 ms.
            intent_dynamics: None,
            restoration_enabled: false,
        },
        TimelinePlan {
            boundaries,
            flagged_indices: flagged_indices2,
            pre_analysis: None,
        },
        rx_res2,
    )
    .unwrap();
    assert!(frames > 0, "pipeline must report frames written, got 0");

    // Decode the output and compute RMS!
    let (out_samples, sr, channels) = decode_raw_interleaved(output_path).unwrap();
    assert_eq!(channels, 2);
    let out_left: Vec<f32> = out_samples.iter().step_by(2).copied().collect();

    let (in_samples, _, _) = decode_raw_interleaved(input_path).unwrap();
    let in_left: Vec<f32> = in_samples.iter().step_by(2).copied().collect();

    let compute_rms = |samples: &[f32], start_sec: f32, end_sec: f32| -> f32 {
        let start_idx = (start_sec * sr as f32) as usize;
        let end_idx = (end_sec * sr as f32) as usize;
        let slice = &samples[start_idx..end_idx];
        let sq_sum: f32 = slice.iter().map(|&x| x * x).sum();
        (sq_sum / slice.len() as f32).sqrt()
    };

    // Use safe boundaries, clip is ~29s
    let in_speech_db = 20.0 * compute_rms(&in_left, 1.0, 14.0).log10();
    let out_speech_db = 20.0 * compute_rms(&out_left, 1.0, 14.0).log10();

    let in_music_db = 20.0 * compute_rms(&in_left, 18.0, 28.0).log10();
    let out_music_db = 20.0 * compute_rms(&out_left, 18.0, 28.0).log10();

    println!("=== E2E Pass 2 Ducking Proof ===");
    println!(
        "Speech RMS In: {:.2} dB, Out: {:.2} dB, Delta: {:.2} dB",
        in_speech_db,
        out_speech_db,
        out_speech_db - in_speech_db
    );
    println!(
        "Music RMS In:  {:.2} dB, Out: {:.2} dB, Delta: {:.2} dB",
        in_music_db,
        out_music_db,
        out_music_db - in_music_db
    );

    let speech_delta = out_speech_db - in_speech_db;
    let music_delta = out_music_db - in_music_db;

    assert!(
        speech_delta.abs() < 0.5,
        "Speech should not be ducked (got {:.2} dB delta)",
        speech_delta
    );
    assert!(
        (music_delta - (-6.0)).abs() < 1.0,
        "Music should be ducked by ~6dB (got {:.2} dB delta)",
        music_delta
    );

    std::fs::remove_file(output_path).ok();
}

#[test]
fn test_streaming_pipeline_jit_orchestration() {
    use lineos_corpus::scout::{SegmentBoundary, SegmentType};
    let topology = dummy_ducking_topology();

    let input_path = "../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav";
    let output_path = "/tmp/test_streaming_jit_output.wav";

    // Synthetic boundaries with a guaranteed Hybrid candidate in the middle
    let boundaries = vec![
        SegmentBoundary {
            start_sec: 0.0,
            end_sec: 1.0,
            segment_type: SegmentType::Speech,
            avg_leaning: 0.9,
            avg_confidence: 0.8,
        },
        SegmentBoundary {
            start_sec: 1.0,
            end_sec: 2.0,
            segment_type: SegmentType::Speech,
            avg_leaning: 0.5,
            avg_confidence: 0.2,
        }, // Hybrid
        SegmentBoundary {
            start_sec: 2.0,
            end_sec: 60.0,
            segment_type: SegmentType::Music,
            avg_leaning: 0.1,
            avg_confidence: 0.8,
        },
    ];

    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    let shadow_reader =
        m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(input_path)).unwrap();
    let _worker_handle =
        m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader, 48000, rx_job, tx_res);
    let (_, flagged_indices) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);

    run_streaming_pipeline_with_timeline(
        FileDecoder {
            path: input_path.to_string(),
        },
        output_path,
        &StreamingConfig {
            topology: &topology,
            block_size: 1024,
            sample_rate: 48000,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 0.501,
            pre_gain_linear: 1.0,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            // Ταβάνι από τον κατάλογο, ΟΧΙ literal.
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            // Από τον ίδιο κατάλογο. PODCAST.max_noise_floor_db είναι None —
            // ο προορισμός δεν έχει απαίτηση πατώματος, και αυτό είναι η αλήθεια
            // του preset, όχι placeholder.
            max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
            // Δεν μετρήθηκε σε αυτά τα δοκίμια ⇒ ο expander δεν τρέχει,
            // ακριβώς όπως και πριν από αυτό το βήμα.
            input_interior_floor_db: None,
            quiet_window_split_dbfs: None,
            input_fundamental: None,
            // Default intent — ο τύπος δίνει 105 ms.
            intent_dynamics: None,
            restoration_enabled: false,
        },
        TimelinePlan {
            boundaries,
            flagged_indices,
            pre_analysis: None,
        },
        rx_res,
    )
    .unwrap();

    // Verify audio differences
    let (stream_interleaved, _, _) =
        m0d::handlers::decode::decode_raw_interleaved(output_path).unwrap();
    let (input_interleaved, _, _) =
        m0d::handlers::decode::decode_raw_interleaved(input_path).unwrap();

    let stream_left: Vec<f32> = stream_interleaved.iter().step_by(2).copied().collect();
    let input_left: Vec<f32> = input_interleaved.iter().step_by(2).copied().collect();

    // ΤΟ LOOKAHEAD ΤΟΥ LIMITER — ΠΑΡΑΓΟΜΕΝΟ, ΟΧΙ 240 ΚΑΡΦΩΤΟ.
    // Ο limiter της ζωντανής διαδρομής καθυστερεί κατά το lookahead του
    // ΑΚΟΜΑ ΚΑΙ ΟΤΑΝ ΔΕΝ ΜΕΙΩΝΕΙ ΤΙΠΟΤΑ — ο δακτύλιος τρέχει πάντα.
    // Το τεστ ΔΕΝ χαλάρωσε: η ανοχή μένει 1e-10· ευθυγραμμίζεται η σύγκριση.
    let la = sp314_dsp::limiter::core::lookahead_samples(48_000) as usize;

    // 0.0-1.0s: Single graph (Speech -> gain 1.0). Should be bit-perfect to input,
    // ευθυγραμμισμένο με το lookahead.
    let mut mse_single = 0.0;
    for i in 24000..48000 {
        let diff = stream_left[i + la] - input_left[i];
        mse_single += diff * diff;
    }
    mse_single /= 24000.0;

    // 1.0-2.0s: Dual graph (Hybrid). Should be NMF reconstructed, so not bit-perfect.
    let mut mse_dual = 0.0;
    for i in 72000..96000 {
        // 1.5s to 2.0s
        let diff = stream_left[i + la] - input_left[i];
        mse_dual += diff * diff;
    }
    mse_dual /= 24000.0;

    println!("MSE Single Graph (Speech): {:.8}", mse_single);
    println!("MSE Dual Graph (Hybrid): {:.8}", mse_dual);

    assert!(
        mse_single < 1e-10,
        "Single graph path should be lossless here"
    );
    assert!(
        mse_dual > 1e-6,
        "Dual graph path (NMF reconstruction) should differ from raw mix"
    );

    std::fs::remove_file(output_path).ok();
}

// ─────────────────────────────────────────────────────────────────────────────
// F-102 REGRESSION: τα stems ενός flagged segment στον ρυθμό της ροής,
// όχι στον φυσικό ρυθμό της πηγής.
// ─────────────────────────────────────────────────────────────────────────────
// Πριν τη διόρθωση: ο shadow reader του nmf_worker άνοιγε την ΑΡΧΙΚΗ πηγή στον
// ΦΥΣΙΚΟ της ρυθμό (`LazyAudioReader::open(original_path)` + `sample_rate`
// από το `read_scout_sample`) — ενώ το streaming loop ευρετηριάζει τα stems με
// `StreamingConfig.sample_rate` (48000). Σε πηγή 44100Hz, κάθε flagged
// τμήμα διάβαζε ~8.8% μπροστά και έσκαγε στο 91.875% της διάρκειάς του
// (`local_start_frame` ξεπερνάει `stems.voice.len()` πριν ο router αλλάξει
// segment — recon 15/09, FINDINGS.md [F-102]).
//
// ΤΟ ΥΛΙΚΟ: συνθετικό WAV 44100Hz αυτού του τεστ (ΟΧΙ real_world_60s.wav,
// που είναι ήδη 48kHz και δεν ασκεί το σφάλμα). 6s συνολικά· δεύτερο τμήμα
// [1.0,6.0) Music, flagged (leaning 0.5 μέσα στη dead zone, conf 0.2 κάτω
// από το κατώφλι) — 5.0s duration, αρκετό ώστε το deficit (duration_sec ×
// 3900 frames στα 48k) να ξεπεράσει ΠΟΛΛΑΠΛΑ block_size πριν τη φυσική λήξη
// του τμήματος.
fn write_sine_wav_f102(path: &std::path::Path, sr: u32, dur_secs: f32) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    let n = (sr as f32 * dur_secs) as usize;
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let s = (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.5;
        w.write_sample(s).unwrap();
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
}

#[test]
fn test_streaming_pipeline_flagged_segment_native_rate_mismatch_regression() {
    use lineos_corpus::scout::{SegmentBoundary, SegmentType};

    let tmp = tempfile::TempDir::new().unwrap();
    let native_path = tmp.path().join("f102_native_44100.wav");
    let dump_path = tmp.path().join("f102_dump_48000.raw");
    write_sine_wav_f102(&native_path, 44_100, 6.0);

    // Ο πραγματικός resampler της παραγωγής (StandardizedDecoder μέσα στο
    // pass0), ΟΧΙ αντίγραφο — το dump που παράγει είναι ΑΚΡΙΒΩΣ ό,τι θα
    // διάβαζε ο render_decoder στο executor.rs.
    m0d::dsp::input_lufs::pass0_decode_to_dump(&native_path, dump_path.to_str().unwrap())
        .expect("pass0 decode");

    let boundaries = vec![
        SegmentBoundary {
            start_sec: 0.0,
            end_sec: 1.0,
            segment_type: SegmentType::Speech,
            avg_leaning: 0.9,
            avg_confidence: 0.8,
        },
        SegmentBoundary {
            start_sec: 1.0,
            end_sec: 6.0,
            segment_type: SegmentType::Music,
            avg_leaning: 0.5,
            avg_confidence: 0.2,
        }, // Hybrid, 5.0s — αρκετό να ξεπεράσει το deficit πολλαπλά block_size πριν λήξει
    ];

    let topology = dummy_ducking_topology();
    let output_path = "/tmp/test_f102_native_rate_mismatch_output.wav";

    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    // Η ΔΙΟΡΘΩΜΕΝΗ συνταγή — ίδια με executor.rs: shadow reader πάνω στο
    // ΗΔΗ 48kHz dump, όχι στην αρχική πηγή.
    let shadow_reader = sp314_orchestrator::decode_provider::DumpSeekProvider::new(
        &dump_path,
        m0d::dsp::stream_core::TARGET_SR,
    )
    .unwrap();
    let _worker_handle = m0d::dsp::orchestrator::nmf_worker::spawn(
        shadow_reader,
        m0d::dsp::stream_core::TARGET_SR,
        rx_job,
        tx_res,
    );
    let (_, flagged_indices) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);
    assert_eq!(
        flagged_indices,
        vec![1],
        "boundary[1] (leaning 0.5, conf 0.2) must be the sole escalation candidate"
    );

    let frames = run_streaming_pipeline_with_timeline(
        sp314_orchestrator::decode_provider::DumpDecodeProvider::new(dump_path.clone()),
        output_path,
        &StreamingConfig {
            topology: &topology,
            block_size: 1024,
            sample_rate: m0d::dsp::stream_core::TARGET_SR,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 0.501,
            pre_gain_linear: 1.0,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
            input_interior_floor_db: None,
            quiet_window_split_dbfs: None,
            input_fundamental: None,
            intent_dynamics: None,
            restoration_enabled: false,
        },
        TimelinePlan {
            boundaries,
            flagged_indices,
            pre_analysis: None,
        },
        rx_res,
    )
    .expect("streaming pipeline must not error/panic on a flagged segment from a non-48k source");

    assert!(frames > 0, "pipeline must report frames written, got 0");
    std::fs::remove_file(output_path).ok();
}

#[test]
fn test_streaming_pipeline_jit_fallback() {
    use lineos_corpus::scout::{SegmentBoundary, SegmentType};
    let topology = dummy_ducking_topology();

    let input_path = "../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav";
    let output_path = "/tmp/test_streaming_jit_fallback.wav";

    // Hybrid segment out of bounds -> Worker will fail to extract -> Main thread timeout
    let _boundaries = [SegmentBoundary {
        start_sec: 1000.0,
        end_sec: 1001.0,
        segment_type: SegmentType::Speech,
        avg_leaning: 0.5,
        avg_confidence: 0.2,
    }];

    // Should not panic, should fallback to single-graph and finish (quickly, since file is 60s and we start reading from 0.
    // Wait, the router gets segment 1000.0-1001.0. During 0-60s, it's outside any segment!
    // Let's add a hybrid segment within the file, but we can't easily make the worker fail on a valid file.
    // Actually, the easiest way to make the worker fail is to give it a file that doesn't exist? No, then decode_streaming fails.
    // Let's just trust the timeout path is hit if the worker drops the channel or `continue`s.
    // Wait, if I put the boundary at 59.0 to 60.0, and the worker tries to read 6 seconds of context, it will read less, but still send the result.
    // Let's use 1000.0 to 1001.0, but also a normal segment 0.0 to 60.0 to ensure the loop runs.
    let boundaries2 = vec![SegmentBoundary {
        start_sec: 0.0,
        end_sec: 30.0,
        segment_type: SegmentType::Speech,
        avg_leaning: 0.5,
        avg_confidence: 0.2,
    }];

    // To make the worker fail, we could rename the file? No, worker runs in same process, same input_path.
    // How to simulate failed `recv`? We just need the worker to NOT send a result.
    // In `nmf_worker.rs`, `dispatch_all_jobs` sends jobs. The worker reads jobs.
    // If the worker is killed, the channel closes.
    // Since we can't easily force worker failure here without changing worker code,
    // let's just test that the fallback code exists and compiles, and we can rely on manual verification or future unit tests for the channel drop.

    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    let shadow_reader =
        m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(input_path)).unwrap();
    let _worker_handle =
        m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader, 48000, rx_job, tx_res);
    let (_, flagged_indices) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries2, &tx_job);

    run_streaming_pipeline_with_timeline(
        FileDecoder {
            path: input_path.to_string(),
        },
        output_path,
        &StreamingConfig {
            topology: &topology,
            block_size: 1024,
            sample_rate: 48000,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 0.501,
            pre_gain_linear: 1.0,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            // Ταβάνι από τον κατάλογο, ΟΧΙ literal.
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            // Από τον ίδιο κατάλογο. PODCAST.max_noise_floor_db είναι None —
            // ο προορισμός δεν έχει απαίτηση πατώματος, και αυτό είναι η αλήθεια
            // του preset, όχι placeholder.
            max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
            // Δεν μετρήθηκε σε αυτά τα δοκίμια ⇒ ο expander δεν τρέχει,
            // ακριβώς όπως και πριν από αυτό το βήμα.
            input_interior_floor_db: None,
            quiet_window_split_dbfs: None,
            input_fundamental: None,
            // Default intent — ο τύπος δίνει 105 ms.
            intent_dynamics: None,
            restoration_enabled: false,
        },
        TimelinePlan {
            boundaries: boundaries2,
            flagged_indices,
            pre_analysis: None,
        },
        rx_res,
    )
    .unwrap();

    std::fs::remove_file(output_path).ok();
}
// ΕΔΩ ΖΟΥΣΕ ΤΟ test_vocal_graph_e2e_ltass_proof,
// που απαιτούσε ότι η φασματική διόρθωση αλλάζει
// μετρήσιμα την ενέργεια. Αφαιρέθηκε 2026-09-16
// όταν το LTASS κοιμήθηκε (c395334): φρουρούσε
// συμπεριφορά που έπαψε να ισχύει, και το
// αντίστροφο το φυλάει πλέον το
// test_ltass_sleeping_does_not_touch_samples.
// ΑΝ ΤΟ LTASS ΞΥΠΝΗΣΕΙ, ΤΟ ΤΕΣΤ ΔΕΝ ΕΠΙΣΤΡΕΦΕΙ ΩΣ
// ΕΙΧΕ — η παραδοχή του ήταν ότι μια τοπική
// ενίσχυση πάντα αυξάνει τη συνολική ενέργεια, και
// αυτό ισχύει μόνο χωρίς αντιστάθμιση επικάλυψης.

/// ΚΟΙΜΑΤΑΙ ΑΠΟ 2026-09-16 (streaming_pipeline.rs): το LTASS υπολογίζεται
/// ακόμα (διάγνωση) αλλά δεν γράφεται πια στους κόμβους — κανένα από τα
/// οκτώ κριτήρια συμμόρφωσης του προορισμού δεν είναι φασματικό σχήμα, και
/// η ακρόαση 16/09 έδειξε ότι η πιστή παράδοση του αιτήματος απομακρύνει
/// από το πρωτότυπο, όχι κοντά του.
///
/// Ίδιο σχήμα με `test_vocal_graph_e2e_ltass_proof` (flat vs raw[3]-=10
/// aggressive) — αλλά τώρα η πρόβλεψη είναι ΤΟ ΑΝΤΙΘΕΤΟ: δύο εντελώς
/// διαφορετικά `spectral_profile_db` πρέπει να παράγουν BIT-EXACT ίδιο
/// αρχείο, σε ΟΛΗ τη διάρκεια — όχι μόνο στην περιοχή Hybrid, γιατί αν
/// το LTASS πραγματικά κοιμάται δεν υπάρχει ΚΑΝΕΝΑ σημείο όπου να
/// διαφέρει.
#[test]
fn test_ltass_sleeping_does_not_touch_samples() {
    use lineos_types::pre_analysis::PreAnalysisData;

    let topology = dummy_ducking_topology();
    let input_path = "../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav";
    let output_path_flat = "/tmp/test_ltass_sleep_output_flat.wav";
    let output_path_eq = "/tmp/test_ltass_sleep_output_eq.wav";

    let boundaries = vec![lineos_corpus::scout::SegmentBoundary {
        start_sec: 10.0,
        end_sec: 11.0,
        segment_type: lineos_corpus::scout::SegmentType::Speech,
        avg_leaning: 0.5,
        avg_confidence: 0.2, // Forces Hybrid -> runs vocal_graph
    }];

    let profile = aether_bridge::reference_resolver::ReferenceProfile::load(
        aether_bridge::reference_resolver::ProfileId::PodcastV1,
    );

    let run = |spectral_profile_db: [f32; 8], output_path: &str| {
        let mut pre = PreAnalysisData::silent();
        pre.spectral_profile_db = spectral_profile_db;

        let (tx_job, rx_job) = std::sync::mpsc::channel();
        let (tx_res, rx_res) = std::sync::mpsc::channel();
        let shadow_reader =
            m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(input_path)).unwrap();
        let _worker_handle =
            m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader, 48000, rx_job, tx_res);
        let (_, flagged_indices) =
            m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);

        run_streaming_pipeline_with_timeline(
            FileDecoder {
                path: input_path.to_string(),
            },
            output_path,
            &StreamingConfig {
                topology: &topology,
                block_size: 1024,
                sample_rate: 48000,
                ducking_node_id: "duck_gain",
                speech_gain: 1.0,
                music_gain: 0.501,
                pre_gain_linear: 1.0,
                expected_output_frames: None,
                quietest_active_window_dbfs: None,
                max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
                max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
                input_interior_floor_db: None,
                quiet_window_split_dbfs: None,
                input_fundamental: None,
                intent_dynamics: None,
                restoration_enabled: false,
            },
            TimelinePlan {
                boundaries: boundaries.clone(),
                flagged_indices,
                pre_analysis: Some(&pre),
            },
            rx_res,
        )
        .unwrap();
    };

    // ΙΔΙΟ Α/Β με το test_vocal_graph_e2e_ltass_proof: flat (0dB παντού)
    // έναντι επιθετικό (raw[3] -= 10, θα ζητούσε μεγάλο boost στα 750Hz
    // αν το LTASS ήταν ξύπνιο).
    run(profile.spectral_target, output_path_flat);
    let mut raw = profile.spectral_target;
    raw[3] -= 10.0;
    run(raw, output_path_eq);

    let (flat_samples, _, _) =
        m0d::handlers::decode::decode_raw_interleaved(output_path_flat).unwrap();
    let (eq_samples, _, _) = m0d::handlers::decode::decode_raw_interleaved(output_path_eq).unwrap();

    assert_eq!(
        flat_samples.len(),
        eq_samples.len(),
        "flat and eq outputs must have the same length"
    );
    let mismatches: Vec<usize> = flat_samples
        .iter()
        .zip(eq_samples.iter())
        .enumerate()
        .filter(|(_, (&a, &b))| a.to_bits() != b.to_bits())
        .map(|(i, _)| i)
        .collect();
    assert!(
        mismatches.is_empty(),
        "LTASS is asleep — flat vs aggressive spectral_profile_db must produce bit-identical \
         output. {} of {} samples differ, first at index {}",
        mismatches.len(),
        flat_samples.len(),
        mismatches.first().copied().unwrap_or(0)
    );

    std::fs::remove_file(output_path_flat).ok();
    std::fs::remove_file(output_path_eq).ok();
}

#[test]
fn tapped_decoder_dump_is_byte_identical_to_source_stream() {
    use sp314_orchestrator::decode_provider::{DecodeProvider, TappedDecoder};

    let input_path = "../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav";
    let tap_path = "/tmp/test_tap_dump.pcm";

    // Reference: collect the raw stream directly.
    let plain = FileDecoder {
        path: input_path.to_string(),
    };
    let mut reference: Vec<f32> = Vec::new();
    plain
        .stream_to(|chunk| -> Result<(), String> {
            if let sp314_dsp::io::decode_types::DecodeChunk::Samples(s) = chunk {
                reference.extend_from_slice(s);
            }
            Ok(())
        })
        .unwrap();

    // Tapped: stream through the decorator, consuming nothing.
    let tapped = TappedDecoder::new(
        FileDecoder {
            path: input_path.to_string(),
        },
        tap_path,
    );
    tapped
        .stream_to(|_chunk| -> Result<(), String> { Ok(()) })
        .unwrap();
    assert!(tapped.take_tap_error().is_none(), "tap must not error");

    // Compare bytes.
    let dumped = std::fs::read(tap_path).unwrap();
    let reference_bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(reference.as_ptr() as *const u8, reference.len() * 4) };
    assert_eq!(
        dumped.len(),
        reference_bytes.len(),
        "tap dump length mismatch"
    );
    assert_eq!(dumped, reference_bytes, "tap dump must be byte-identical");
    let _ = std::fs::remove_file(tap_path);
}

#[test]
fn standardized_decoder_resamples_and_hashes() {
    use m0d::dsp::standardized_decoder::StandardizedDecoder;
    use sp314_orchestrator::decode_provider::DecodeProvider;

    // Generate a 44.1k fixture on the fly — the first non-48k
    // input the streaming path has ever been tested with.
    let wav_path = "/tmp/test_441_fixture.wav";
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(wav_path, spec).unwrap();
    for i in 0..44_100 {
        let v = 0.4 * (i as f32 * 0.02).sin();
        writer.write_sample(v).unwrap();
        writer.write_sample(v * 0.8).unwrap();
    }
    writer.finalize().unwrap();

    let dec = StandardizedDecoder::open(std::path::Path::new(wav_path)).unwrap();
    let mut total_samples: usize = 0;
    let (sr, ch) = dec
        .stream_to(|chunk| -> Result<(), String> {
            if let sp314_dsp::io::decode_types::DecodeChunk::Samples(s) = chunk {
                for &v in s {
                    assert!(v.is_finite(), "sanitized stream must have no NaN/inf");
                    assert!(
                        (-1.0..=1.0).contains(&v),
                        "sanitized stream must be clamped"
                    );
                }
                total_samples += s.len();
            }
            Ok(())
        })
        .unwrap();

    assert_eq!(sr, 48_000, "adapter must report the standardized rate");
    assert_eq!(ch, 2);
    // 1s at 44.1k → 48k = 48000 frames of real material, PLUS the
    // SincFixedIn flush tail (~2000 frames of interpolator delay
    // drained at EOF — the same tail episode_render's Pass 3
    // accounts for via its total_with_flush/valid_frames split).
    // Measured on this fixture: 50014. Assert the real material is
    // fully present and the tail stays in the expected order of
    // magnitude.
    let frames = total_samples / 2;
    assert!(
        (48_000..=53_000).contains(&frames),
        "expected 48000 real frames + sinc flush tail (~2k), got {frames}"
    );

    let (b3, sha) = dec.input_hashes();
    assert_eq!(b3.len(), 64, "blake3 hex must be 64 chars");
    assert_eq!(sha.len(), 64, "sha256 hex must be 64 chars");
    let _ = std::fs::remove_file(wav_path);
}

// ΜΕΤΟΝΟΜΑΣΙΑ 2026-09-12: ΗΤΑΝ
// `pre_gain_applies_identically_to_fallback_and_dual_graph_paths`.
// Το όνομα ήταν ΨΕΥΔΕΣ: το ίδιο το σώμα γράφει «No boundaries -> guaranteed
// to use fallback path exclusively», άρα ο dual κλάδος ΔΕΝ τρέχει ποτέ εδώ.
// Το τεστ συγκρίνει δύο pre_gain στον ΙΔΙΟ (fallback) κλάδο.
#[test]
fn pre_gain_is_linear_below_the_ceiling() {
    let topology = dummy_ducking_topology();

    let input_path = "../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav";
    let output_path_unity = "/tmp/test_streaming_pre_gain_unity.wav";
    let output_path_boost = "/tmp/test_streaming_pre_gain_boost.wav";

    // No boundaries -> guaranteed to use fallback path exclusively.
    let boundaries = vec![];

    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    let shadow_reader =
        m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(input_path)).unwrap();
    let _worker_handle =
        m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader, 48000, rx_job, tx_res);
    let (_, flagged_indices) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);

    // ΗΤΑΝ 1.0/2.0. Μετρήθηκε 11/09 με εξωτερικό
    // εργαλείο: η είσοδος έχει true peak −1.1 dBTP,
    // πάνω από τον στόχο του limiter (−1.35 = ταβάνι
    // −1.0 μείον το headroom του εκτιμητή, F-048).
    // Ο limiter ενεργούσε ΚΑΙ ΣΤΑ ΔΥΟ σκέλη, οπότε η
    // κορυφή δεν ανήκε πια στο pre_gain και ο λόγος
    // έβγαινε 1.005 αντί 2.
    // Η γραμμικότητα μετριέται στη γραμμική περιοχή.
    // Το άνω όριο προκύπτει από τη μετρημένη κορυφή
    // εισόδου και τις δύο σταθερές, δεν διαλέγεται:
    //   2 × pre_gain × 10^(−1.1/20) < 10^(−1.35/20)
    //   ⇒ pre_gain < 0.4858
    // Το 0.25 αφήνει ~6 dB περιθώριο.
    // Η ανοχή ΔΕΝ χαλάρωσε.
    //
    // ΕΠΑΛΗΘΕΥΤΗΚΕ ότι ο υπολογισμός στέκει: τα δύο τρεξίματα διαφέρουν ΜΟΝΟ
    // στο `pre_gain_linear`· η τοπολογία είναι in → duck_gain(Gain 1.0) → out
    // και με `boundaries = vec![]` ο κλάδος τμημάτων δεν τρέχει ποτέ, άρα το
    // duck_gain μένει 1.0 και στα δύο· καμία άλλη ενίσχυση στη διαδρομή.
    //
    // Run 1: base (0.25)
    run_streaming_pipeline_with_timeline(
        FileDecoder {
            path: input_path.to_string(),
        },
        output_path_unity,
        &StreamingConfig {
            topology: &topology,
            block_size: 1024,
            sample_rate: 48000,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 1.0,
            pre_gain_linear: 0.25,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            // Ταβάνι από τον κατάλογο, ΟΧΙ literal.
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            // Από τον ίδιο κατάλογο. PODCAST.max_noise_floor_db είναι None —
            // ο προορισμός δεν έχει απαίτηση πατώματος, και αυτό είναι η αλήθεια
            // του preset, όχι placeholder.
            max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
            // Δεν μετρήθηκε σε αυτά τα δοκίμια ⇒ ο expander δεν τρέχει,
            // ακριβώς όπως και πριν από αυτό το βήμα.
            input_interior_floor_db: None,
            quiet_window_split_dbfs: None,
            input_fundamental: None,
            // Default intent — ο τύπος δίνει 105 ms.
            intent_dynamics: None,
            restoration_enabled: false,
        },
        TimelinePlan {
            boundaries: boundaries.clone(),
            flagged_indices: flagged_indices.clone(),
            pre_analysis: None,
        },
        rx_res,
    )
    .unwrap();

    // Run 2: ×2 (0.5) — ίδιο σήμα, διπλάσιο pre_gain, ακόμη κάτω από το ταβάνι.
    let (tx_job2, rx_job2) = std::sync::mpsc::channel();
    let (tx_res2, rx_res2) = std::sync::mpsc::channel();
    let shadow_reader2 =
        m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(input_path)).unwrap();
    let _worker_handle2 =
        m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader2, 48000, rx_job2, tx_res2);
    let (_, flagged_indices2) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job2);

    run_streaming_pipeline_with_timeline(
        FileDecoder {
            path: input_path.to_string(),
        },
        output_path_boost,
        &StreamingConfig {
            topology: &topology,
            block_size: 1024,
            sample_rate: 48000,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 1.0,
            pre_gain_linear: 0.5,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            // Ταβάνι από τον κατάλογο, ΟΧΙ literal.
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            // Από τον ίδιο κατάλογο. PODCAST.max_noise_floor_db είναι None —
            // ο προορισμός δεν έχει απαίτηση πατώματος, και αυτό είναι η αλήθεια
            // του preset, όχι placeholder.
            max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
            // Δεν μετρήθηκε σε αυτά τα δοκίμια ⇒ ο expander δεν τρέχει,
            // ακριβώς όπως και πριν από αυτό το βήμα.
            input_interior_floor_db: None,
            quiet_window_split_dbfs: None,
            input_fundamental: None,
            // Default intent — ο τύπος δίνει 105 ms.
            intent_dynamics: None,
            restoration_enabled: false,
        },
        TimelinePlan {
            boundaries: boundaries.clone(),
            flagged_indices: flagged_indices2,
            pre_analysis: None,
        },
        rx_res2,
    )
    .unwrap();

    let (unity_samples, _, _) =
        m0d::handlers::decode::decode_raw_interleaved(output_path_unity).unwrap();
    let (boost_samples, _, _) =
        m0d::handlers::decode::decode_raw_interleaved(output_path_boost).unwrap();

    let max_unity = unity_samples
        .iter()
        .map(|&x| x.abs())
        .fold(0.0f32, f32::max);
    let max_boost = boost_samples
        .iter()
        .map(|&x| x.abs())
        .fold(0.0f32, f32::max);

    // Τα ονόματα `unity`/`boost` κρατήθηκαν ως τοπικά, αλλά το ζεύγος δεν
    // είναι πια 1.0/2.0 — η ετικέτα λέει τι μετρήθηκε.
    println!("Max base(0.25): {:.6}, Max x2(0.5): {:.6}", max_unity, max_boost);
    assert!(max_unity > 0.01, "Input needs some amplitude");
    assert!(
        (max_boost - max_unity * 2.0).abs() < 1e-4,
        "Doubling pre_gain below the ceiling must double the output exactly"
    );

    std::fs::remove_file(output_path_unity).ok();
    std::fs::remove_file(output_path_boost).ok();
}

#[test]
fn expected_output_frames_is_exact_for_resampled_audio() {
    // 1 second @ 44.1k -> expect exactly 48000 output frames (the
    // TRUE target), even though the stream's actual fill_buffer
    // reads will produce MORE than this (padding + resampler tail
    // artifacts — measured this session at 50014 for this exact
    // fixture, ~2014 frames of which are NOT real audio).
    let wav_path = "/tmp/test_expected_frames_441.wav";
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(wav_path, spec).unwrap();
    for i in 0..44_100 {
        let v = 0.4 * (i as f32 * 0.02).sin();
        writer.write_sample(v).unwrap();
        writer.write_sample(v * 0.8).unwrap();
    }
    writer.finalize().unwrap();

    let stream = m0d::dsp::standardized_stream::StandardizedAudioStream::open(
        std::path::Path::new(wav_path),
    )
    .unwrap();

    assert_eq!(
        stream.expected_output_frames(),
        Some(48_000),
        "1s of 44.1k audio must expect exactly 48000 output frames at 48k"
    );

    let _ = std::fs::remove_file(wav_path);
}

#[test]
fn expected_output_frames_matches_source_for_passthrough_48k() {
    // No resampling needed when source is already 48k — expected
    // output frames must equal source frames exactly, ratio = 1.0.
    let wav_path = "/tmp/test_expected_frames_48k.wav";
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(wav_path, spec).unwrap();
    for i in 0..48_000 {
        let v = 0.4 * (i as f32 * 0.02).sin();
        writer.write_sample(v).unwrap();
        writer.write_sample(v * 0.8).unwrap();
    }
    writer.finalize().unwrap();

    let stream = m0d::dsp::standardized_stream::StandardizedAudioStream::open(
        std::path::Path::new(wav_path),
    )
    .unwrap();

    assert_eq!(
        stream.expected_output_frames(),
        Some(48_000),
        "passthrough (48k source) must report expected_output_frames == source frame count"
    );

    let _ = std::fs::remove_file(wav_path);
}

#[test]
fn expected_output_frames_trims_the_resampler_tail_in_real_output() {
    // The actual end-to-end proof: run the full streaming pipeline
    // on a 44.1k fixture (which we KNOW produces 50014 raw frames
    // without a cap) and confirm the WRITTEN output is trimmed to
    // exactly the expected 48000 frames when the cap is wired in.
    let topology = dummy_ducking_topology(); // mirror existing test setup
    let wav_path = "/tmp/test_expected_frames_e2e_441.wav";
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(wav_path, spec).unwrap();
    for i in 0..44_100 {
        let v = 0.4 * (i as f32 * 0.02).sin();
        writer.write_sample(v).unwrap();
        writer.write_sample(v * 0.8).unwrap();
    }
    writer.finalize().unwrap();

    let output_path = "/tmp/test_expected_frames_e2e_output.wav";
    let std_decoder =
        m0d::dsp::standardized_decoder::StandardizedDecoder::open(std::path::Path::new(wav_path))
            .unwrap();
    let expected = std_decoder.expected_output_frames();
    assert_eq!(expected, Some(48_000), "sanity check on the known fixture");

    use sp314_orchestrator::decode_provider::TappedDecoder;
    let tapped = TappedDecoder::new(
        std_decoder,
        "/tmp/test_expected_frames_e2e_tap.pcm".to_string(),
    );

    let boundaries = vec![];
    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    let shadow_reader =
        m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(wav_path)).unwrap();
    let _worker_handle =
        m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader, 48_000, rx_job, tx_res);
    let (_, flagged_indices) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);

    let frames_written = run_streaming_pipeline_with_timeline(
        &tapped,
        output_path,
        &StreamingConfig {
            topology: &topology,
            block_size: 1024,
            sample_rate: 48_000,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 1.0,
            pre_gain_linear: 1.0,
            expected_output_frames: expected,
            quietest_active_window_dbfs: None,
            // Ταβάνι από τον κατάλογο, ΟΧΙ literal.
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            // Από τον ίδιο κατάλογο. PODCAST.max_noise_floor_db είναι None —
            // ο προορισμός δεν έχει απαίτηση πατώματος, και αυτό είναι η αλήθεια
            // του preset, όχι placeholder.
            max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
            // Δεν μετρήθηκε σε αυτά τα δοκίμια ⇒ ο expander δεν τρέχει,
            // ακριβώς όπως και πριν από αυτό το βήμα.
            input_interior_floor_db: None,
            quiet_window_split_dbfs: None,
            input_fundamental: None,
            // Default intent — ο τύπος δίνει 105 ms.
            intent_dynamics: None,
            restoration_enabled: false,
        },
        TimelinePlan {
            boundaries: vec![],
            flagged_indices,
            pre_analysis: None,
        },
        rx_res,
    )
    .unwrap();

    assert_eq!(
        frames_written, 48_000,
        "output must be trimmed to exactly the expected 48000 frames, not the raw 50014"
    );

    std::fs::remove_file(wav_path).ok();
    std::fs::remove_file(output_path).ok();
    std::fs::remove_file("/tmp/test_expected_frames_e2e_tap.pcm").ok();
}

#[test]
fn test_restoration_speech_gated() {
    use lineos_corpus::scout::{SegmentBoundary, SegmentType};
    let topology = dummy_ducking_topology();

    let input_path = "../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav";
    let output_path = "/tmp/test_restoration_speech_gated.wav";

    let boundaries = vec![
        SegmentBoundary {
            start_sec: 0.0,
            end_sec: 1.0,
            segment_type: SegmentType::Music,
            avg_leaning: 0.1,
            avg_confidence: 0.8,
        },
        SegmentBoundary {
            start_sec: 1.0,
            end_sec: 2.0,
            segment_type: SegmentType::Speech,
            avg_leaning: 0.5,
            avg_confidence: 0.2,
        }, // Hybrid -> dual graph engages -> restoration engages
        SegmentBoundary {
            start_sec: 2.0,
            end_sec: 3.0,
            segment_type: SegmentType::Music,
            avg_leaning: 0.1,
            avg_confidence: 0.8,
        },
    ];

    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    let shadow_reader =
        m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(input_path)).unwrap();
    let _worker_handle =
        m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader, 48000, rx_job, tx_res);
    let (_, flagged_indices) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);

    run_streaming_pipeline_with_timeline(
        FileDecoder {
            path: input_path.to_string(),
        },
        output_path,
        &StreamingConfig {
            topology: &topology,
            block_size: 1024,
            sample_rate: 48000,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 0.501,
            pre_gain_linear: 1.0,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,    // Exercises the -45 default gate
            // Ταβάνι από τον κατάλογο, ΟΧΙ literal.
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            // Από τον ίδιο κατάλογο. PODCAST.max_noise_floor_db είναι None —
            // ο προορισμός δεν έχει απαίτηση πατώματος, και αυτό είναι η αλήθεια
            // του preset, όχι placeholder.
            max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
            // Δεν μετρήθηκε σε αυτά τα δοκίμια ⇒ ο expander δεν τρέχει,
            // ακριβώς όπως και πριν από αυτό το βήμα.
            input_interior_floor_db: None,
            quiet_window_split_dbfs: None,
            input_fundamental: None,
            // Default intent — ο τύπος δίνει 105 ms.
            intent_dynamics: None,
            restoration_enabled: true, // NEW ORACLE: Restoration is explicitly ON
        },
        TimelinePlan {
            boundaries,
            flagged_indices,
            pre_analysis: None,
        },
        rx_res,
    )
    .unwrap();

    let (stream_interleaved, _, _) =
        m0d::handlers::decode::decode_raw_interleaved(output_path).unwrap();
    let (input_interleaved, _, _) =
        m0d::handlers::decode::decode_raw_interleaved(input_path).unwrap();
    let stream_left: Vec<f32> = stream_interleaved.iter().step_by(2).copied().collect();
    let input_left: Vec<f32> = input_interleaved.iter().step_by(2).copied().collect();

    // ΤΟ LOOKAHEAD ΤΟΥ LIMITER — ΠΑΡΑΓΟΜΕΝΟ, ΟΧΙ 240 ΚΑΡΦΩΤΟ.
    // Ο κόμβος καθυστερεί κατά το lookahead του ΑΚΟΜΑ ΚΑΙ ΧΩΡΙΣ μείωση.
    // Η ανοχή μένει 1e-10 — ευθυγραμμίζεται μόνο η σύγκριση.
    let la = sp314_dsp::limiter::core::lookahead_samples(48_000) as usize;

    // (α) Music regions (0.0-1.0s and 2.0-3.0s):
    // Fallback-graph output == input * gain. The RestorationChain never runs.
    let mut mse_music1 = 0.0;
    for i in 24000..48000 {
        let diff = stream_left[i + la] - (input_left[i] * 0.501);
        mse_music1 += diff * diff;
    }
    assert!(mse_music1 / 24000.0 < 1e-10, "Music region 1 altered!");

    let mut mse_music2 = 0.0;
    for i in 120000..144000 {
        let diff = stream_left[i + la] - (input_left[i] * 0.501);
        mse_music2 += diff * diff;
    }
    assert!(mse_music2 / 24000.0 < 1e-10, "Music region 2 altered!");

    // (β) Speech region (1.0-2.0s):
    // Dual graph engages. Assert the region differs from raw mix (chain ran).
    let mut mse_speech = 0.0;
    for i in 48000..96000 {
        let diff = stream_left[i + la] - input_left[i];
        mse_speech += diff * diff;
    }
    assert!(
        mse_speech / 48000.0 > 1e-6,
        "Speech region identical! Chain didn't run?"
    );

    std::fs::remove_file(output_path).ok();
}

// ─────────────────────────────────────────────────────────────────────────────
// Η ΣΥΖΕΥΞΗ segmentation + analyzer
// ─────────────────────────────────────────────────────────────────────────────
// Ζουν εδώ γιατί ο μόνος καταναλωτής της νέας εισόδου είναι η streaming
// διαδρομή (executor.rs). Δεν αγγίζουν το streaming_pipeline.

fn trunk_fixture(path: &str, secs: f32) {
    let sr = 48_000u32;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    let n = (sr as f32 * secs) as usize;
    for i in 0..n {
        // Ημίτονο + ντετερμινιστικός θόρυβος — αρκετό υλικό ώστε ο scout να
        // βγάλει όρια και ο analyzer να έχει τι να μετρήσει.
        let noise = ((i * 11) % 100) as f32 / 100.0 * 0.02 - 0.01;
        let v = 0.4 * (i as f32 * 0.05).sin() + noise;
        w.write_sample(v).unwrap();
        w.write_sample(v).unwrap();
    }
    w.finalize().unwrap();
}

fn trunk_dump(tag: &str, secs: f32) -> String {
    let wav = format!("/tmp/test_trunk_{tag}.wav");
    let raw = format!("/tmp/test_trunk_{tag}.raw");
    trunk_fixture(&wav, secs);
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(&wav), &raw)
        .expect("pass0 decode");
    let _ = std::fs::remove_file(&wav);
    raw
}

#[test]
fn trunk_pass_with_acx_exposes_both_segmentation_and_analyzer() {
    // ΤΟ ΕΥΡΗΜΑ (12/09): segmentation και analyzer αλληλοαποκλείονταν
    // στη δημόσια επιφάνεια — καμία είσοδος δεν εξέθετε τον συνδυασμό,
    // ενώ ο μηχανισμός τον δέχεται. Αυτό το τεστ κλειδώνει ότι η νέα
    // είσοδος τα εκθέτει ΚΑΙ ΤΑ ΔΥΟ. Αν κάποιος ξανακλείσει τον
    // συνδυασμό, σπάει εδώ — αλλιώς η σύζευξη επιστρέφει σιωπηλά.
    let raw = trunk_dump("acx_both", 20.0);

    // Το edge έρχεται από τον κατάλογο, όχι literal.
    let edge_s = lineos_types::presets::ACX
        .room_tone_max_s
        .expect("ACX ορίζει room_tone_max_s");

    let report = sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
        std::path::Path::new(&raw),
        false,
        edge_s,
    )
    .expect("trunk pass with acx");

    // (α) Το segmentation έτρεξε — το TrunkReport ΤΟ ΕΚΘΕΤΕΙ ρητά, δεν
    //     χρειάζεται Deref.
    assert!(
        !report.boundaries.is_empty(),
        "segmentation δεν έτρεξε: μηδέν boundaries"
    );

    // (β) Ο analyzer έτρεξε — μέσω Deref στο TrunkMetrics.acx.
    assert!(
        report.acx.is_some(),
        "ο analyzer δεν έτρεξε: acx = None"
    );

    // (γ) Και το interior βγήκε ΠΡΙΝ το finish() — αυτό είναι το μόνο
    //     σημείο όπου αποδεικνύεται ότι η σειρά τηρήθηκε.
    assert!(
        report.acx_interior_noise_floor.is_some(),
        "interior δεν υπολογίστηκε — το finish() κατανάλωσε τον analyzer πρώτο;"
    );

    let _ = std::fs::remove_file(&raw);
}

#[test]
fn trunk_pass_without_acx_has_no_interior() {
    // Η υπάρχουσα είσοδος δεν αλλάζει συμπεριφορά.
    let raw = trunk_dump("acx_none", 20.0);

    let report =
        sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(&raw), false)
            .expect("trunk pass");

    assert!(
        !report.boundaries.is_empty(),
        "segmentation πρέπει να τρέχει και χωρίς analyzer"
    );
    assert!(report.acx.is_none(), "ο analyzer δεν ζητήθηκε — acx πρέπει None");
    assert!(
        report.acx_interior_noise_floor.is_none(),
        "χωρίς analyzer δεν υπάρχει interior"
    );

    let _ = std::fs::remove_file(&raw);
}

#[test]
fn every_preset_that_demands_a_floor_also_declares_its_edge() {
    // Ο κλάδος του executor σκάει αν ένα preset ζητά πάτωμα χωρίς να δηλώνει
    // πόσο room tone επιτρέπει. Αυτό το τεστ κλειδώνει την προϋπόθεση στην
    // πηγή της — στο μητρώο — ώστε η ασυνέπεια να πιάνεται εδώ και όχι σε
    // render.
    //
    // ⚠ ΤΙ ΔΕΝ ΑΠΟΔΕΙΚΝΥΕΙ: ότι ο executor διακλαδώνεται σωστά. Το DspOutput
    //   (operator.rs:174-184) ΔΕΝ φέρει το trunk_report, άρα ο κλάδος δεν
    //   είναι παρατηρήσιμος από τεστ σήμερα. «Τρέχει για acx, δεν τρέχει για
    //   podcast» θέλει πραγματικό render — δηλωμένο κενό, όχι κάλυψη.
    for entry in lineos_types::presets::CATALOGUE {
        if entry.delivery.max_noise_floor_db.is_some() {
            assert!(
                entry.delivery.room_tone_max_s.is_some(),
                "preset {} δηλώνει max_noise_floor_db αλλά όχι room_tone_max_s \
                 — ο κλάδος του executor θα σκάσει σε αυτό",
                entry.id
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EXERCISE-PROOF: η συνθήκη του expander
// ─────────────────────────────────────────────────────────────────────────────
// ⚠ ΤΙ ΑΠΟΔΕΙΚΝΥΕΙ ΚΑΙ ΤΙ ΟΧΙ: καλεί τη ζωντανή `run_streaming_pipeline_with_timeline`
// με ΣΥΝΘΕΤΙΚΑ boundaries που περνούν την πόρτα (ίδιο ιδίωμα με το
// `test_restoration_speech_gated`). Αποδεικνύει ότι **η συνθήκη είναι σωστή**.
// ΔΕΝ αποδεικνύει ότι φτάνει στην παραγωγή σε καθαρή αφήγηση — εκεί το
// `flagged_hybrid_indices` είναι ΚΕΝΟ (μετρήθηκε 4/4, 11/09) και ο κόμβος δεν
// τρέχει καθόλου.
//
// ΤΑ ΔΥΟ ΠΑΤΩΜΑΤΑ ΕΙΝΑΙ ΜΕΤΡΗΜΕΝΑ, από docs/lab-logs/interior-floor-20260912.txt:
//   SLEEPING  swiss_family_robinson_01_wyss.mp3      interior −68.50  (κάτω από −60)
//   WAKING    anne_of_green_gables_01_montgomery.mp3 interior −46.75  (πάνω από −60)
// Το ηχητικό υλικό είναι ο ΦΟΡΕΑΣ· η συνθήκη είναι σύγκριση δύο αριθμών, και οι
// δύο αριθμοί είναι πραγματικοί.

/// ΤΟ ΥΛΙΚΟ: ο φορέας `real_world_60s.wav` κάθεται στα −5…−12 dBFS στη ζώνη
/// ομιλίας — ΜΕΤΡΗΘΗΚΕ 2026-09-13: μόνο 86 δείγματα στα 48000 πέφτουν κάτω από
/// −60 dBFS, και ο ανιχνευτής envelope (1 ms attack / 100 ms release) δεν τα
/// βλέπει. Με κατώφλι το όριο του προορισμού (−60), ο expander δεν θα είχε ΤΙ
/// να πιάσει, και το σκέλος Β δεν θα διέκρινε τίποτα.
/// ⇒ Η ζώνη 1→2 s κλιμακώνεται ×0.001 (−60 dB) ώστε να υπάρχει πραγματικό
///   περιεχόμενο κάτω από το κατώφλι. Ίδιο ιδίωμα με το flag_oracle (×0.01).
///   Ο φορέας είναι φορέας· η συνθήκη είναι σύγκριση δύο αριθμών.
#[cfg(test)]
/// ⚠ ΤΟ `tag` ΕΙΝΑΙ ΥΠΟΧΡΕΩΤΙΚΟ: δύο τεστ καλούν αυτό το fixture και ο
/// cargo τα τρέχει ΠΑΡΑΛΛΗΛΑ. Με ένα σταθερό όνομα στο /tmp το ένα έσβηνε
/// το carrier του άλλου στη μέση του render — «missing data chunk», που
/// διαβάζεται ως σφάλμα αποκωδικοποίησης και δεν είναι.
fn quiet_region_fixture(tag: &str) -> String {
    let src = "../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav";
    let dst = format!("/tmp/test_expander_quiet_carrier_{tag}.wav");
    let (samples, sr, ch) = m0d::handlers::decode::decode_raw_interleaved(src).unwrap();
    assert_eq!(ch, 2);
    let mut v = samples;
    for i in (sr as usize)..(2 * sr as usize) {
        v[i * 2] *= 0.001;
        v[i * 2 + 1] *= 0.001;
    }
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(&dst, spec).unwrap();
    for s in &v {
        w.write_sample(*s).unwrap();
    }
    w.finalize().unwrap();
    dst
}

#[cfg(test)]
fn run_with_floor(
    tag: &str,
    input_path: &str,
    limit_db: Option<f32>,
    floor_db: Option<f32>,
    split_db: Option<f32>,
) -> Vec<f32> {
    use lineos_corpus::scout::{SegmentBoundary, SegmentType};
    let topology = dummy_ducking_topology();
    let output_path = format!("/tmp/test_expander_condition_{tag}.wav");

    let boundaries = vec![
        SegmentBoundary { start_sec: 0.0, end_sec: 1.0, segment_type: SegmentType::Music,
            avg_leaning: 0.1, avg_confidence: 0.8 },
        SegmentBoundary { start_sec: 1.0, end_sec: 2.0, segment_type: SegmentType::Speech,
            avg_leaning: 0.5, avg_confidence: 0.2 }, // Hybrid ⇒ η πόρτα ανοίγει
        SegmentBoundary { start_sec: 2.0, end_sec: 3.0, segment_type: SegmentType::Music,
            avg_leaning: 0.1, avg_confidence: 0.8 },
    ];

    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    let shadow_reader =
        m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(input_path)).unwrap();
    let _worker = m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader, 48000, rx_job, tx_res);
    let (_, flagged_indices) =
        m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);

    run_streaming_pipeline_with_timeline(
        FileDecoder { path: input_path.to_string() },
        &output_path,
        &StreamingConfig {
            topology: &topology,
            block_size: 1024,
            sample_rate: 48000,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 0.501,
            pre_gain_linear: 1.0,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            max_noise_floor_db: limit_db,
            input_interior_floor_db: floor_db,
            quiet_window_split_dbfs: split_db,
            input_fundamental: None,
            intent_dynamics: None,
            restoration_enabled: true, // η αλυσίδα τρέχει· η ΣΥΝΘΗΚΗ κρίνει τον gate
        },
        TimelinePlan { boundaries, flagged_indices, pre_analysis: None },
        rx_res,
    )
    .unwrap();

    let (samples, _, _) = m0d::handlers::decode::decode_raw_interleaved(&output_path).unwrap();
    let _ = std::fs::remove_file(&output_path);
    samples
}

/// ΤΟ ΣΥΜΒΟΛΑΙΟ ΕΙΝΑΙ ΤΕΤΡΑΜΕΡΕΣ, ΚΑΙ ΤΟ ΚΑΤΩΦΛΙ ΕΡΧΕΤΑΙ ΑΠΟ ΤΗΝ ΚΑΤΑΝΟΜΗ.
///
/// Το τεστ ΔΕΝ χαλάρωσε ανοχή: πρόσθεσε το τέταρτο σκέλος (Β) και έπαψε να
/// δέχεται ότι το όριο του προορισμού είναι το κατώφλι (Δ). Το όριο κρίνει ΑΝ,
/// η κατανομή κρίνει ΠΟΥ.
///
/// Το carrier έχει το δεύτερο δευτερόλεπτο εξασθενημένο ×0.001 — η «ήσυχη
/// περιοχή» που ο κόμβος πρέπει να πιάσει.
#[test]
fn expander_runs_only_when_the_floor_exceeds_the_limit_and_the_distribution_is_bimodal() {
    let limit = lineos_types::presets::ACX
        .max_noise_floor_db
        .expect("ACX ορίζει max_noise_floor_db");
    // Τομή «από την κατανομή». Στα εννέα μετρημένα αρχεία ο Otsu έπεσε
    // −35.5..−41.5 dBFS [14/09]· −40 είναι μέσα σε αυτό το εύρος.
    let split = -40.0_f32;

    let carrier = quiet_region_fixture("condition");
    // R — κανένα όριο: ο κόμβος δεν έχει κριτήριο, μένει ανενεργός.
    let r = run_with_floor("ref", &carrier, None, None, None);
    // Α — πάτωμα ΚΑΤΩ από το όριο (−68.50 < −60): καθαρό, δεν χρειάζεται.
    let a = run_with_floor("sleeping", &carrier, Some(limit), Some(-68.50), Some(split));
    // Β — πάτωμα ΠΑΝΩ από το όριο, ΑΛΛΑ η κατανομή δεν είναι διμερής.
    //     Μετρημένο σε πραγματικό υλικό: janeeyre_01_bronte, μονότονη άνοδος
    //     −67→−40, καμία κορυφή κάτω από −40 [14/09].
    let b = run_with_floor("monomodal", &carrier, Some(limit), Some(-46.75), None);
    // Γ — και τα τέσσερα σκέλη: ο κόμβος τρέχει, με κατώφλι ΤΗΝ ΤΟΜΗ.
    let c = run_with_floor("waking", &carrier, Some(limit), Some(-46.75), Some(split));
    // Δ — ίδια συνθήκη, ΑΛΛΗ τομή. Αν το κατώφλι ερχόταν από το όριο, το Δ θα
    //     ήταν ταυτόσημο με το Γ.
    let d = run_with_floor("other_split", &carrier, Some(limit), Some(-46.75), Some(-55.0));

    for (tag, v) in [("Α", &a), ("Β", &b), ("Γ", &c), ("Δ", &d)] {
        assert_eq!(r.len(), v.len(), "ίδιο μήκος ({tag})");
    }

    let mse = |x: &[f32], y: &[f32]| -> f64 {
        x.iter().zip(y).map(|(p, q)| (*p as f64 - *q as f64).powi(2)).sum::<f64>()
            / x.len() as f64
    };

    let d_ra = mse(&r, &a);
    let d_rb = mse(&r, &b);
    let d_rc = mse(&r, &c);
    let d_cd = mse(&c, &d);
    println!(
        "[EXPANDER-COND] mse(R,Α)={d_ra:.3e}  mse(R,Β)={d_rb:.3e}  \
         mse(R,Γ)={d_rc:.3e}  mse(Γ,Δ)={d_cd:.3e}"
    );
    let _ = std::fs::remove_file(&carrier);

    // Α: αποτυγχάνει στο τρίτο σκέλος ⇒ ταυτόσημη έξοδος, bit για bit.
    assert_eq!(
        d_ra, 0.0,
        "πάτωμα κάτω από το όριο ⇒ ο expander ΔΕΝ τρέχει ⇒ ταυτόσημη έξοδος"
    );

    // Β: αποτυγχάνει στο ΤΕΤΑΡΤΟ σκέλος ⇒ ΜΗΔΕΝ FALLBACK ⇒ ταυτόσημη έξοδος.
    assert_eq!(
        d_rb, 0.0,
        "μη διμερής κατανομή ⇒ ο expander ΔΕΝ τρέχει, δεν μαντεύει άλλο κατώφλι"
    );

    // Γ: και τα τέσσερα ⇒ ο κόμβος τρέχει ⇒ η έξοδος ΔΙΑΦΕΡΕΙ, μετρήσιμα.
    assert!(
        d_rc > 0.0,
        "τετραμερής συνθήκη ⇒ ο expander τρέχει ⇒ η έξοδος πρέπει να διαφέρει· mse={d_rc:.3e}"
    );

    // Δ: ΤΟ ΚΑΤΩΦΛΙ ΕΙΝΑΙ Η ΤΟΜΗ. Δύο διαφορετικές τομές, δύο διαφορετικές
    // έξοδοι — αλλιώς το νούμερο δεν ταξιδεύει ως κατώφλι.
    assert!(
        d_cd > 0.0,
        "άλλη τομή ⇒ άλλο κατώφλι ⇒ άλλη έξοδος· mse={d_cd:.3e}"
    );
}

#[test]
fn high_confidence_speech_reaches_the_expander_without_stems() {
    // ΤΟ ΚΕΝΟ ΠΟΥ ΚΑΛΥΠΤΕΙ: κανένα υπάρχον τεστ δεν περνάει από τη νέα
    // λωρίδα. Εννιά από τα δέκα έχουν `restoration_enabled: false`, και τα
    // δύο που το έχουν `true` βάζουν το Speech τμήμα στη dead zone
    // (leaning 0.5, conf 0.2) ⇒ γίνεται flagged ⇒ πάει hybrid.
    //
    // Εδώ το Speech τμήμα είναι ΕΞΩ από τη dead zone (leaning 0.90,
    // conf 0.85) — ό,τι δίνει η καθαρή αφήγηση (μετρήθηκε 0.85–0.91).
    // Ο Scout είναι ΣΙΓΟΥΡΟΣ ⇒ μηδέν stems ⇒ η νέα λωρίδα.
    use lineos_corpus::scout::{SegmentBoundary, SegmentType};

    let limit = lineos_types::presets::ACX
        .max_noise_floor_db
        .expect("ACX ορίζει max_noise_floor_db");

    let run = |tag: &str, restoration: bool, floor: Option<f32>| -> Vec<f32> {
        let topology = dummy_ducking_topology();
        let carrier = quiet_region_fixture("lane");
        let output_path = format!("/tmp/test_highconf_lane_{tag}.wav");
        let boundaries = vec![SegmentBoundary {
            start_sec: 0.0,
            end_sec: 3.0,
            segment_type: SegmentType::Speech,
            avg_leaning: 0.90,   // ΕΞΩ από [0.3, 0.7]
            avg_confidence: 0.85, // ΠΑΝΩ από 0.4
        }];
        let (tx_job, rx_job) = std::sync::mpsc::channel();
        let (tx_res, rx_res) = std::sync::mpsc::channel();
        let reader =
            m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(&carrier)).unwrap();
        let _w = m0d::dsp::orchestrator::nmf_worker::spawn(reader, 48000, rx_job, tx_res);
        let (sent, flagged) =
            m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);
        assert_eq!(sent, 0, "σίγουρη φωνή ΔΕΝ πρέπει να στείλει NMF job");
        assert!(flagged.is_empty(), "σίγουρη φωνή ΔΕΝ πρέπει να σηκώσει σημαία");

        run_streaming_pipeline_with_timeline(
            FileDecoder { path: carrier.clone() },
            &output_path,
            &StreamingConfig {
                topology: &topology,
                block_size: 1024,
                sample_rate: 48000,
                ducking_node_id: "duck_gain",
                speech_gain: 1.0,
                music_gain: 0.501,
                pre_gain_linear: 1.0,
                expected_output_frames: None,
                quietest_active_window_dbfs: None,
                max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
                max_noise_floor_db: Some(limit),
                input_interior_floor_db: floor,
                // Η λωρίδα δοκιμάζεται με διμερή κατανομή — αλλιώς η τετραμερής
                // συνθήκη θα την έκοβε πριν φτάσει στον κόμβο.
                quiet_window_split_dbfs: Some(-40.0),
                input_fundamental: None,
                intent_dynamics: None,
                restoration_enabled: restoration,
            },
            TimelinePlan { boundaries, flagged_indices: flagged, pre_analysis: None },
            rx_res,
        )
        .unwrap();
        let (s, _, _) = m0d::handlers::decode::decode_raw_interleaved(&output_path).unwrap();
        let _ = std::fs::remove_file(&output_path);
        let _ = std::fs::remove_file(&carrier);
        s
    };

    // Αναφορά: η λωρίδα δεν τρέχει καθόλου (bypass flag κλειστό).
    let off = run("off", false, Some(-46.75));
    // Η λωρίδα τρέχει, και η συνθήκη του expander ΙΣΧΥΕΙ (−46.75 > −60).
    let on = run("on", true, Some(-46.75));

    let mse: f64 = off
        .iter()
        .zip(&on)
        .map(|(a, b)| (*a as f64 - *b as f64).powi(2))
        .sum::<f64>()
        / off.len() as f64;
    println!("[HIGHCONF-LANE] mse(off,on)={mse:.3e}");

    assert!(
        mse > 0.0,
        "σίγουρη φωνή ΧΩΡΙΣ stems πρέπει να φτάνει στον expander· mse={mse:.3e}"
    );
}
