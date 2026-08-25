//! F-083 mechanism confirmation: reproduce export_mp3_acx's steps 4a/4b
//! (RMS-window auto-correction + true-peak trim) VERBATIM, using the
//! REAL sp314_dsp::analysis::acx_check::AcxCheckAnalyzer and
//! sp314_dsp::metering::true_peak_meter::TruePeakMeter (not
//! reimplementations), on the ACTUAL buggy (header-included) mono
//! downmix, to show the exact rms_before / tp_db / correction gains
//! export_mp3_acx computes internally and why they crush real speech.

use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use sp314_dsp::metering::true_peak_meter::TruePeakMeter;
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

fn main() {
    let input = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "research/w-speech/corpus/2277-149896-0000.flac".to_string());
    let input_abs = std::fs::canonicalize(&input).expect("resolve input");

    let plan = m0d::agents::operator::StreamingPlan {
        audio_path: input_abs.to_string_lossy().into_owned(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: "f083-mechanism-confirm".to_string(),
    };

    let (_output, blob) = m0d::agents::executor::execute_streaming_plan(&plan, None)
        .expect("execute_streaming_plan failed");

    let raw_bytes = std::fs::read(blob.core.audio_path.path()).expect("read blob audio_path");

    // ACTUAL production behavior: zero header skip (export.rs's
    // pcm_bytes_to_f32, verbatim logic).
    let pcm: Vec<f32> = raw_bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    let half = pcm.len() / 2;
    let mut planar = [Vec::with_capacity(half), Vec::with_capacity(half)];
    for chunk in pcm.chunks_exact(2) {
        planar[0].push(chunk[0]);
        planar[1].push(chunk[1]);
    }

    let target_sr = 44100;
    let original_sr = 48000;
    let ratio = target_sr as f64 / original_sr as f64;
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, 4096, 2).expect("resampler init");
    let total_frames = planar[0].len();
    let mut pos = 0usize;
    let mut resampled_planar: Vec<Vec<f32>> = vec![Vec::new(); 2];
    while pos < total_frames {
        let end = (pos + 4096).min(total_frames);
        let chunk_len = end - pos;
        let wave_in: Vec<Vec<f32>> = if chunk_len == 4096 {
            planar.iter().map(|ch| ch[pos..end].to_vec()).collect()
        } else {
            planar.iter().map(|ch| { let mut v = ch[pos..end].to_vec(); v.resize(4096, 0.0); v }).collect()
        };
        let mut wave_out = resampler.process(&wave_in, None).expect("resample chunk");
        let valid_out_frames = (chunk_len as f64 * ratio).round() as usize;
        for c in 0..2 {
            wave_out[c].truncate(valid_out_frames);
            resampled_planar[c].extend_from_slice(&wave_out[c]);
        }
        pos += chunk_len;
    }

    let mut mono: Vec<f32> = resampled_planar[0]
        .iter()
        .zip(resampled_planar[1].iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();

    // ---- verbatim export_mp3_acx step 4a: RMS window correction ----
    let mut acx_pre = AcxCheckAnalyzer::new(target_sr as u32);
    for chunk in mono.chunks(4096) {
        acx_pre.feed_chunk(chunk);
    }
    let report_first_pass = acx_pre.finish();
    let rms_before = report_first_pass.rms_db;
    println!("MEASURED rms_before (AcxCheckAnalyzer, real fn, on buggy mono) = {}", rms_before);

    const ACX_RMS_MIN: f32 = -23.0;
    const ACX_RMS_MAX: f32 = -18.0;
    const RMS_MARGIN_DB: f32 = 0.5;
    let rms_correction_db = if rms_before < ACX_RMS_MIN {
        ACX_RMS_MIN + RMS_MARGIN_DB - rms_before
    } else if rms_before > ACX_RMS_MAX {
        ACX_RMS_MAX - RMS_MARGIN_DB - rms_before
    } else {
        0.0
    };
    println!("MEASURED rms_correction_db computed = {}", rms_correction_db);

    if rms_correction_db != 0.0 {
        let gain = libm::powf(10.0, rms_correction_db / 20.0);
        println!("MEASURED gain applied to ENTIRE mono buffer = {:e}", gain);
        for s in mono.iter_mut() {
            *s *= gain;
        }
    }

    // ---- verbatim step 4b: true peak trim ----
    let mut tp_meter = TruePeakMeter::new();
    for chunk in mono.chunks(4096) {
        tp_meter.process_chunk(chunk, chunk);
    }
    let tp_db = tp_meter.finish();
    println!("MEASURED tp_db AFTER rms correction = {}", tp_db);

    let mut tp_trim_db = 0.0_f32;
    if tp_db > -3.0 {
        let diff_db = -3.05 - tp_db;
        tp_trim_db = diff_db;
        let gain = libm::powf(10.0, diff_db / 20.0);
        println!("MEASURED SECOND (peak trim) gain = {:e} (diff_db={})", gain, diff_db);
        for s in mono.iter_mut() {
            *s *= gain;
        }
    }
    println!("MEASURED tp_trim_db = {}", tp_trim_db);

    // Final report (what export_mp3_acx actually returns)
    let final_report = if rms_correction_db == 0.0 && tp_trim_db == 0.0 {
        report_first_pass
    } else {
        let mut acx = AcxCheckAnalyzer::new(target_sr as u32);
        for chunk in mono.chunks(4096) {
            acx.feed_chunk(chunk);
        }
        acx.finish()
    };
    println!("MEASURED FINAL AcxCheckReport = {:?}", final_report);

    // How much of the ORIGINAL real-speech energy survived, in isolation:
    // measure RMS of mono[20000..30000] (a real-speech-only slice, well
    // past the ~17-sample header region) after both corrections.
    let real_slice_rms_db = {
        let s = &mono[20000.min(mono.len())..30000.min(mono.len())];
        let sum_sq: f64 = s.iter().map(|&v| (v as f64) * (v as f64)).sum();
        20.0 * (sum_sq / s.len() as f64).sqrt().max(1e-300).log10()
    };
    println!(
        "MEASURED real-speech-region (samples 20000..30000) rms_db AFTER both corrections = {:.3}",
        real_slice_rms_db
    );

    println!("DONE");
}
