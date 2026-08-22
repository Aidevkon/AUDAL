use lineos_types::audio::ManagedPcm;
use m0d::blob_store::{StoredBlobV2, StoredBlobCore, BlobVariant, StoredLoudness, StoredProvenance, StoredQuality, StoredSpatial};
use m0d::dsp::signal_health::DeadAirSummary;
use m0d::handlers::export::export_flac;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

fn generate_test_pcm(path: &std::path::Path) {
    let mut file = std::fs::File::create(path).unwrap();
    let mut phase = 0.0f32;
    // Let's make it 1 second long
    for _ in 0..(48000) {
        let mut sample = (phase * std::f32::consts::TAU).sin();
        phase += 440.0 / 48000.0;
        if phase >= 1.0 {
            phase -= 1.0;
        }

        sample *= 0.5;

        let bytes = sample.to_le_bytes();
        file.write_all(&bytes).unwrap();
        file.write_all(&bytes).unwrap(); // left and right
    }
}

fn make_blob(pcm_path: PathBuf) -> StoredBlobV2 {
    StoredBlobV2 {
        core: StoredBlobCore {
            id: "test_flac".into(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "".into(),
            input_path_hash: "".into(),
            input_pcm_hash: None,
            seed: 1,
            pipeline_version: "".into(),
            schema_version: 1,
            preset_id: "acx".into(),
            pcm_blake3: None,
            cert_signature: None,
            audio_path: Arc::new(ManagedPcm::new(pcm_path)),
            sample_rate: 48000,
            channels: 2,
            num_frames: 48000,
        },
        variant: BlobVariant::Certified {
            loudness: StoredLoudness::default(),
            quality: StoredQuality::default(),
            provenance: StoredProvenance::default(),
            spatial: StoredSpatial::default(),
            stem_fingerprints: None,
            processing_timeline: vec![],
            dead_air: DeadAirSummary::default(),
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            qr_base64: None,
        },
    }
}

#[test]
fn test_export_flac_real() {
    let tmp = tempfile::TempDir::new().unwrap();
    let pcm_path = tmp.path().join("input.pcm");
    let flac_path = tmp.path().join("output.flac");

    generate_test_pcm(&pcm_path);
    let blob = make_blob(pcm_path);

    // Call the public export_flac
    export_flac(&blob, &flac_path).expect("export_flac failed");

    assert!(flac_path.exists(), "FLAC output file was not created");

    // Check with symphonia
    let decoded_frames = {
        let file = std::fs::File::open(&flac_path).unwrap();
        let mss = symphonia::core::io::MediaSourceStream::new(Box::new(file), Default::default());
        let hint = symphonia::core::probe::Hint::new();
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &Default::default(), &Default::default())
            .unwrap();

        let mut format = probed.format;
        let track = format.default_track().unwrap();

        assert_eq!(
            track.codec_params.codec,
            symphonia::core::codecs::CODEC_TYPE_FLAC
        );
        assert_eq!(track.codec_params.sample_rate, Some(48000));
        assert_eq!(track.codec_params.channels.map(|c| c.count()), Some(2));

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &Default::default())
            .unwrap();

        let mut total = 0;
        loop {
            match format.next_packet() {
                Ok(packet) => {
                    if let Ok(decoded) = decoder.decode(&packet) {
                        total += decoded.frames();
                    }
                }
                Err(symphonia::core::errors::Error::IoError(_)) => break,
                Err(_) => break, // EOF or other error
            }
        }
        total
    };

    assert!(
        decoded_frames >= blob.core.num_frames && decoded_frames < blob.core.num_frames + 4096,
        "Decoded frames ({}) mismatch vs rendered ({})",
        decoded_frames,
        blob.core.num_frames
    );
}

#[test]
#[ignore]
fn test_export_flac_ffprobe() {
    let tmp = tempfile::TempDir::new().unwrap();
    let pcm_path = tmp.path().join("input_ffprobe.pcm");
    let flac_path = tmp.path().join("output_ffprobe.flac");

    generate_test_pcm(&pcm_path);
    let blob = make_blob(pcm_path);

    export_flac(&blob, &flac_path).expect("export_flac failed");

    let output = std::process::Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("stream=codec_name,sample_rate")
        .arg("-of")
        .arg("csv=p=0")
        .arg(&flac_path)
        .output()
        .expect("ffprobe execution failed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("flac,48000"),
        "ffprobe output mismatch: {}",
        stdout
    );
}
