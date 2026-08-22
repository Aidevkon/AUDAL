use lineos_types::audio::ManagedPcm;
use m0d::blob_store::{StoredBlobV2, StoredBlobCore, BlobVariant, StoredLoudness, StoredProvenance, StoredQuality, StoredSpatial};
use m0d::dsp::signal_health::DeadAirSummary;
use m0d::handlers::export::export_mp3_acx;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;

fn make_blob(pcm_path: PathBuf) -> StoredBlobV2 {
    StoredBlobV2 {
        core: StoredBlobCore {
            id: "test_acx".into(),
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
            num_frames: 48000 * 12,
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

/// Ίδια δομή με το export_mp3_acx.rs (ημιτόνο 440Hz,
/// 2s κύκλος, 50% duty), αλλά με ΡΥΘΜΙΖΟΜΕΝΟ πλάτος
/// ώστε να πετύχουμε συγκεκριμένο RMS.
///
/// Το ποσοστό παύσης ΕΧΕΙ ΣΗΜΑΣΙΑ: χωρίς παύσεις το
/// noise floor ισούται με το RMS και το passes_acx()
/// κόβει για λάθος λόγο.
fn generate_pcm_at_amplitude(path: &Path, burst_amp: f32) {
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
            sample *= burst_amp;
        } else {
            sample *= 0.000316; // -70 dBFS
        }

        let bytes = sample.to_le_bytes();
        file.write_all(&bytes).unwrap();
        file.write_all(&bytes).unwrap(); // left and right
    }
}

/// Μετράει το RMS ΤΟΥ ΙΔΙΟΥ ΑΡΧΕΙΟΥ πριν την export,
/// με τον ΙΔΙΟ analyzer. Χωρίς αυτό δεν ξέρουμε αν
/// το fixture είναι εκεί που το σχεδιάσαμε.
fn measure_rms_before(pcm_path: &Path) -> f32 {
    let bytes = std::fs::read(pcm_path).unwrap();
    let frames = bytes.len() / 8; // 2 channels, 4 bytes each
    let mut mono = Vec::with_capacity(frames);
    for chunk in bytes.chunks_exact(8) {
        let l = f32::from_le_bytes(chunk[0..4].try_into().unwrap());
        let r = f32::from_le_bytes(chunk[4..8].try_into().unwrap());
        mono.push((l + r) * 0.5);
    }
    
    let mut acx = AcxCheckAnalyzer::new(48000);
    for chunk in mono.chunks(4096) {
        acx.feed_chunk(chunk);
    }
    let report = acx.finish();
    report.rms_db
}

#[test]
#[ignore = "writes mp3, runs LAME encoder"]
fn acx_export_lifts_quiet_signal_into_rms_window() {
    let burst_amp = 0.02; // ≈ -30 dBFS
    let pcm_path = PathBuf::from("/tmp/acx_rms_quiet.pcm");
    let out_path = PathBuf::from("/tmp/acx_rms_quiet.mp3");
    
    generate_pcm_at_amplitude(&pcm_path, burst_amp);
    let rms_before = measure_rms_before(&pcm_path);
    assert!(rms_before < -23.0, "fixture must start OUTSIDE the window, below it. Got: {}", rms_before);
    
    let blob = make_blob(pcm_path.clone());
    let outcome = export_mp3_acx(&blob, &out_path).unwrap();
    let report = outcome.report;
    
    let delta = report.rms_db - rms_before;
    println!("acx_export_lifts_quiet_signal_into_rms_window:");
    println!("rms_before: {:.2}, report.rms_db: {:.2}, delta: {:.2}", rms_before, report.rms_db, delta);
    
    assert!(report.rms_db >= -23.0, "rms too low: {}", report.rms_db);
    assert!(report.rms_db <= -18.0, "rms too high: {}", report.rms_db);
    assert!(report.sample_peak_db <= -3.0, "peak too high: {}", report.sample_peak_db);
}

#[test]
#[ignore = "writes mp3, runs LAME encoder"]
fn acx_export_leaves_compliant_signal_alone() {
    let burst_amp = 0.2; // ≈ -20 dBFS
    let pcm_path = PathBuf::from("/tmp/acx_rms_inside.pcm");
    let out_path = PathBuf::from("/tmp/acx_rms_inside.mp3");
    
    generate_pcm_at_amplitude(&pcm_path, burst_amp);
    let rms_before = measure_rms_before(&pcm_path);
    assert!(rms_before >= -23.0 && rms_before <= -18.0, "fixture must start INSIDE the window. Got: {}", rms_before);
    
    let blob = make_blob(pcm_path.clone());
    let outcome = export_mp3_acx(&blob, &out_path).unwrap();
    let report = outcome.report;
    
    let delta = report.rms_db - rms_before;
    println!("acx_export_leaves_compliant_signal_alone:");
    println!("rms_before: {:.2}, report.rms_db: {:.2}, delta: {:.2}", rms_before, report.rms_db, delta);
    
    assert!(delta.abs() < 0.6, "delta > 0.6 dB, minimal intervention violated. delta: {}", delta);
}

#[test]
#[ignore = "writes mp3, runs LAME encoder"]
fn acx_export_lowers_hot_signal_into_rms_window() {
    let burst_amp = 0.35; // ≈ -14 dBFS
    let pcm_path = PathBuf::from("/tmp/acx_rms_hot.pcm");
    let out_path = PathBuf::from("/tmp/acx_rms_hot.mp3");
    
    generate_pcm_at_amplitude(&pcm_path, burst_amp);
    let rms_before = measure_rms_before(&pcm_path);
    assert!(rms_before > -18.0, "fixture must start OUTSIDE the window, above it. Got: {}", rms_before);
    
    let blob = make_blob(pcm_path.clone());
    let outcome = export_mp3_acx(&blob, &out_path).unwrap();
    let report = outcome.report;
    
    let delta = report.rms_db - rms_before;
    println!("acx_export_lowers_hot_signal_into_rms_window:");
    println!("rms_before: {:.2}, report.rms_db: {:.2}, delta: {:.2}", rms_before, report.rms_db, delta);
    
    assert!(report.rms_db <= -18.0, "rms too high: {}", report.rms_db);
    assert!(report.rms_db >= -23.0, "rms too low: {}", report.rms_db);
    assert!(report.sample_peak_db <= -3.0, "peak too high: {}", report.sample_peak_db);
}
