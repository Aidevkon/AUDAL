//! ΑΠΟΔΕΙΞΗ 2026-08-24 — ποιος ελέγχει το peak, τι κοστίζει το RMS.
//! Τρέχει ΤΡΙΑ πραγματικά αρχεία αφήγησης μέσα από την ΠΛΗΡΗ ζωντανή
//! διαδρομή (execute_streaming_plan) ΚΑΙ το πραγματικό export_mp3_acx,
//! μετράει πριν/μετά. Καμία re-implementation.
//!
//! usage: who_controls_peak <out_dir> <file1.wav> <file2.wav> <file3.wav>

use m0d::agents::executor::execute_streaming_plan;
use m0d::agents::operator::StreamingPlan;
use m0d::dsp::lazy_reader::LazyAudioReader;
use m0d::handlers::export::export_mp3_acx;
use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use sp314_dsp::metering::true_peak_meter::TruePeakMeter;
use std::io::Write;

fn ffmpeg_ebur128_lufs_tp(path: &str) -> Option<(f64, f64)> {
    encoder_gap_speech::ffmpeg_ebur128_on_file(&[], std::path::Path::new(path)).ok()
}

fn measure_master(mono: &[f32], sr: u32) -> (f32, f32, f32) {
    // (sample_peak_db, true_peak_dbtp, rms_db)
    let mut acx = AcxCheckAnalyzer::new(sr);
    let mut tp = TruePeakMeter::new();
    for chunk in mono.chunks(4096) {
        acx.feed_chunk(chunk);
        tp.process_chunk(chunk, chunk);
    }
    let report = acx.finish();
    let true_peak = tp.finish();
    (report.sample_peak_db, true_peak, report.rms_db)
}

fn write_wav_mono(path: &str, sig: &[f32], sr: u32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("wav create");
    for &s in sig {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 4, "usage: who_controls_peak <out_dir> <f1> <f2> <f3>");
    let out_dir = std::path::PathBuf::from(&args[0]);
    std::fs::create_dir_all(&out_dir).unwrap();
    let files = &args[1..4];

    for f in files {
        let fname = std::path::Path::new(f).file_stem().unwrap().to_string_lossy().to_string();
        eprintln!("################ {fname} ################");

        // ── (a) ΠΛΗΡΗΣ streaming διαδρομή -> master ──
        let plan = StreamingPlan {
            audio_path: f.clone(),
            preset_id: "acx".to_string(),
            flavour_id: None,
            intent_tone: None,
            intent_dynamics: None,
            target_lufs_override: None,
            session_id: format!("peak-{fname}"),
        };
        let result = execute_streaming_plan(&plan, None);
        // ΚΡΑΤΑ το output ζωντανό — το pcm_data guard (ManagedPcm) διαγράφει
        // το αρχείο στο Drop· ένα _output θα το έσβηνε πριν προλάβουμε να
        // διαβάσουμε το master.
        let (output, blob) = match result {
            Ok((output, blob)) => (output, blob),
            Err(e) => {
                eprintln!("  FAILED streaming: {e:?}");
                continue;
            }
        };
        eprintln!("  blob_id={} sample_rate={}", blob.core.id, blob.core.sample_rate);
        eprintln!("  blob.core.audio_path = {}", blob.core.audio_path.path().display());
        let audio_path_copy = out_dir.join(format!("{fname}_audio_path_raw.bin"));
        std::fs::copy(blob.core.audio_path.path(), &audio_path_copy).ok();
        eprintln!(
            "  audio_path size={} bytes, αντιγράφηκε -> {}",
            std::fs::metadata(&audio_path_copy).map(|m| m.len()).unwrap_or(0),
            audio_path_copy.display()
        );

        // ── Διάβασε το master raw pcm, μέτρα ΠΡΙΝ το export ──
        let master_path = m0d::blob_store::mastered_path(&blob.core.id);
        let raw = std::fs::read(&master_path).expect("read master pcm");
        let n = raw.len() / 4;
        let mut mono_pre = Vec::with_capacity(n / 2);
        let mut i = 0usize;
        while i + 8 <= raw.len() {
            let l = f32::from_le_bytes([raw[i], raw[i + 1], raw[i + 2], raw[i + 3]]);
            let r = f32::from_le_bytes([raw[i + 4], raw[i + 5], raw[i + 6], raw[i + 7]]);
            mono_pre.push((l + r) * 0.5);
            i += 8;
        }
        let (peak_pre, tp_pre, rms_pre) = measure_master(&mono_pre, blob.core.sample_rate);

        let pre_wav = out_dir.join(format!("{fname}_master_48k.wav"));
        write_wav_mono(pre_wav.to_str().unwrap(), &mono_pre, blob.core.sample_rate);
        let lufs_pre = ffmpeg_ebur128_lufs_tp(pre_wav.to_str().unwrap()).map(|(l, _)| l);

        eprintln!(
            "  ΠΡΙΝ export (48k master): sample_peak={peak_pre:.3}dB true_peak={tp_pre:.3}dBTP rms={rms_pre:.3}dB lufs={:?}",
            lufs_pre
        );

        // ── (b) ΠΡΑΓΜΑΤΙΚΟ export_mp3_acx ──
        let mp3_path = out_dir.join(format!("{fname}_export.mp3"));
        match export_mp3_acx(&blob, &mp3_path) {
            Ok(outcome) => {
                eprintln!(
                    "  export_mp3_acx report (post-trim, pre-encode buffer): sample_peak={:.3}dB rms={:.3}dB noise_floor={:?}",
                    outcome.report.sample_peak_db,
                    outcome.report.rms_db,
                    outcome.report.noise_floor_db
                );

                // ── Μέτρα το ΠΡΑΓΜΑΤΙΚΟ παραδοτέο mp3 (μετά το lossy encode) ──
                let post = ffmpeg_ebur128_lufs_tp(mp3_path.to_str().unwrap());
                let (lufs_post, tp_post) = post.unwrap_or((f64::NAN, f64::NAN));

                eprintln!(
                    "  ΜΕΤΑ export (πραγματικό .mp3, decoded): lufs={lufs_post:.3} true_peak={tp_post:.3}dBTP"
                );

                let rms_post = outcome.report.rms_db;
                let in_window = (-23.0..=-18.0).contains(&rms_post);
                eprintln!(
                    "  RMS ΜΕΣΑ στο ACX [-23,-18]; rms_post={rms_post:.3}dB -> {}",
                    if in_window { "NAI" } else { "*** OXI ***" }
                );

                let rms_drop = rms_pre - rms_post;
                let peak_drop = peak_pre - outcome.report.sample_peak_db;
                eprintln!(
                    "  Δrms={rms_drop:+.3}dB  Δpeak={peak_drop:+.3}dB  (ίσα αν ΕΝΑ σταθερό trim· διαφορετικά = δύο ξεχωριστά περάσματα)"
                );
            }
            Err(e) => {
                eprintln!("  FAILED export: {e}");
            }
        }
        eprintln!();
        let _ = std::io::stderr().flush();
        drop(output); // τώρα είναι ασφαλές — τελειώσαμε με το master pcm
    }
}
