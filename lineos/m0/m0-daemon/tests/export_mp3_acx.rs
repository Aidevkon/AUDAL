use lineos_types::audio::ManagedPcm;
use m0d::blob_store::{StoredBlob, StoredLoudness, StoredProvenance, StoredQuality, StoredSpatial};
use m0d::dsp::signal_health::DeadAirSummary;
use m0d::handlers::export::export_mp3_acx;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

fn generate_test_pcm(path: &std::path::Path) {
    let mut file = std::fs::File::create(path).unwrap();
    let mut phase = 0.0f32;
    for frame in 0..(48000 * 12) {
        let sec = frame as f32 / 48000.0;
        let is_burst = (sec % 2.0) < 1.0;

        let mut sample = (phase * std::f32::consts::TAU).sin();
        phase += 440.0 / 48000.0;
        if phase >= 1.0 {
            phase -= 1.0;
        }

        if is_burst {
            sample *= 0.1; // -20 dBFS
        } else {
            sample *= 0.000316; // -70 dBFS
        }

        let bytes = sample.to_le_bytes();
        file.write_all(&bytes).unwrap();
        file.write_all(&bytes).unwrap(); // left and right
    }
}

fn make_blob(pcm_path: PathBuf) -> StoredBlob {
    StoredBlob {
        id: "test_acx".into(),
        version: "1.0".into(),
        blob_type: "audio".into(),
        created_at: "".into(),
        input_hash: "".into(),
        seed: 1,
        pipeline_version: "".into(),
        preset_id: "acx".into(),
        loudness: StoredLoudness::default(),
        quality: StoredQuality::default(),
        provenance: StoredProvenance::default(),
        spatial: StoredSpatial::default(),
        schema_version: 1,
        aether_cert: None,
        aether_persona: None,
        aether_config: None,
        stem_fingerprints: None,
        qr_base64: None,
        pcm_blake3: None,
        cert_signature: None,
        processing_timeline: vec![],
        dead_air: DeadAirSummary::default(),
        audio_path: Arc::new(ManagedPcm::new(pcm_path)),
        sample_rate: 48000,
        channels: 2,
        num_frames: 48000 * 12,
    }
}

#[test]
fn test_export_mp3_acx() {
    let pcm_path = std::path::PathBuf::from("/tmp/test_export_mp3_acx_in.pcm");
    let out_path = std::path::PathBuf::from("/tmp/test_export_mp3_acx_out.mp3");

    generate_test_pcm(&pcm_path);
    let blob = make_blob(pcm_path.clone());

    let report = export_mp3_acx(&blob, &out_path).expect("export_mp3_acx failed");

    assert!(
        report.noise_floor_db.is_some(),
        "noise floor should be populated"
    );

    let meta = std::fs::metadata(&out_path).expect("output file missing");
    assert!(meta.len() > 10000, "output too small");

    let mp3_bytes = std::fs::read(&out_path).unwrap();
    let mut offset = 0;
    let mut frame_count = 0;

    while offset < mp3_bytes.len() - 4 {
        if mp3_bytes[offset] == 0xFF && (mp3_bytes[offset + 1] & 0xE0) == 0xE0 {
            if frame_count > 0 {
                // skip frame 0 (Xing header)
                let bitrate_index = (mp3_bytes[offset + 2] & 0xF0) >> 4;
                assert_eq!(bitrate_index, 11, "frame {} is not 192kbps", frame_count);
            }
            frame_count += 1;

            let padding = (mp3_bytes[offset + 2] & 0x02) >> 1;
            let frame_len = 144 * 192000 / 44100 + padding as usize;
            offset += frame_len;
        } else {
            offset += 1;
        }

        if frame_count >= 50 {
            break;
        }
    }
    assert!(frame_count >= 50, "did not find enough mp3 frames");

    let _ = std::fs::remove_file(pcm_path);
    let _ = std::fs::remove_file(out_path);
}

#[test]
#[ignore]
fn test_export_mp3_acx_ffprobe() {
    let pcm_path = std::path::PathBuf::from("/tmp/test_export_mp3_acx_ffprobe_in.pcm");
    let out_path = std::path::PathBuf::from("/tmp/test_export_mp3_acx_ffprobe_out.mp3");
    generate_test_pcm(&pcm_path);
    let blob = make_blob(pcm_path.clone());
    export_mp3_acx(&blob, &out_path).unwrap();

    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=sample_rate,bit_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            out_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines[0], "44100");
    assert_eq!(lines[1], "192000");

    let _ = std::fs::remove_file(pcm_path);
    let _ = std::fs::remove_file(out_path);
}
