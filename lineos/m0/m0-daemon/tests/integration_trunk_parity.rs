use m0d::dsp::file_decoder::FileDecoder;
use m0d::dsp::input_lufs::pass0_decode_to_dump;
use sp314_orchestrator::pass1_pipeline::build_timeline_map;
use sp314_orchestrator::trunk_pass::run_trunk_pass;
use std::path::Path;

fn generate_fixture(path: &str) {
    let sr = 48_000u32;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    let frames_half = (sr as f32 * 6.0) as usize; // 6s

    // First half: high variance, noisy (music-ish)
    for i in 0..frames_half {
        // combine a sine and some fake noise (high crest)
        let noise = ((i * 11) % 100) as f32 / 100.0 * 0.5 - 0.25;
        let v = 0.6 * (i as f32 * 0.1).sin() + noise;
        writer.write_sample(v).unwrap();
        writer.write_sample(v).unwrap();
    }

    // Second half: lower variance, smooth (speech-ish)
    for i in 0..frames_half {
        let v = 0.3 * (i as f32 * 0.02).sin();
        writer.write_sample(v).unwrap();
        writer.write_sample(v).unwrap();
    }
    writer.finalize().unwrap();
}

#[test]
fn test_trunk_parity() {
    let wav_path = "/tmp/test_trunk_parity.wav";
    let raw_path = "/tmp/test_trunk_parity.raw";

    generate_fixture(wav_path);

    // Path A: Old Full-RAM Decoder
    let decoder = FileDecoder {
        path: wav_path.to_string(),
    };
    let boundaries_old = build_timeline_map(decoder).expect("old path failed");

    // Path B: New P0 -> Trunk Dump Reader
    let (_metrics, _p0_decoder) = pass0_decode_to_dump(Path::new(wav_path), raw_path)
        .expect("P0 dump failed");
    let report = run_trunk_pass(Path::new(raw_path)).expect("trunk pass failed");
    let boundaries_new = report.boundaries;

    // Assert: Length parity
    assert_eq!(
        boundaries_old.len(),
        boundaries_new.len(),
        "Boundary count mismatch"
    );

    // Assert: Per-boundary parity
    for (i, (old, new)) in boundaries_old.iter().zip(boundaries_new.iter()).enumerate() {
        assert_eq!(
            old.start_sec, new.start_sec,
            "Boundary {} start mismatch: {} vs {}",
            i, old.start_sec, new.start_sec
        );
        assert_eq!(
            old.end_sec, new.end_sec,
            "Boundary {} end mismatch: {} vs {}",
            i, old.end_sec, new.end_sec
        );
        assert_eq!(
            old.segment_type, new.segment_type,
            "Boundary {} type mismatch: {:?} vs {:?}",
            i, old.segment_type, new.segment_type
        );
    }

    // Assert: Metrics sanity
    assert!(
        report.integrated_lufs.is_some(),
        "Expected non-empty LUFS for 12s signal"
    );
    assert!(
        report.noise_floor_dbfs.is_some(),
        "Expected non-empty noise floor for non-silent signal"
    );
}
