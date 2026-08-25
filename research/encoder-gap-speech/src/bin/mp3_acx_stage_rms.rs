//! F-083 follow-up: export_mp3_acx's own AcxCheckReport measured
//! rms_db=-57 on real narration, but export_flac (same file, no
//! resample/downmix) and an independent ffmpeg resample+downmix of the
//! same audio both measure ~-23.9dB. That isolates the collapse to
//! export_mp3_acx's resample-then-downmix stage specifically — this
//! binary stages that same sequence (rubato params copied VERBATIM from
//! export.rs, for diagnostic isolation only, not a redefinition of the
//! production formula) with RMS printed after each stage, on a
//! CORRECTLY header-skipped planar signal (so the negligible,
//! already-characterized header-parse bug doesn't confound this
//! measurement).

use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

fn rms_db(x: &[f32]) -> f32 {
    let sum_sq: f64 = x.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / x.len() as f64).sqrt();
    20.0 * (rms.max(1e-12)).log10() as f32
}

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
        session_id: "f083-stage-rms-recon".to_string(),
    };

    let (_output, blob) = m0d::agents::executor::execute_streaming_plan(&plan, None)
        .expect("execute_streaming_plan failed");

    let raw_bytes = std::fs::read(blob.core.audio_path.path()).expect("read blob audio_path");

    // CORRECT header skip (measured true header length via the file's
    // own RIFF "data" chunk marker — not the buggy zero-skip export.rs
    // uses today).
    let data_marker = raw_bytes
        .windows(4)
        .position(|w| w == b"data")
        .expect("no data chunk");
    let true_header_len = data_marker + 8;
    println!("MEASURED true_header_len = {}", true_header_len);

    let pcm: Vec<f32> = raw_bytes[true_header_len..]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    let half = pcm.len() / 2;
    let mut planar = [Vec::with_capacity(half), Vec::with_capacity(half)];
    for chunk in pcm.chunks_exact(2) {
        planar[0].push(chunk[0]);
        planar[1].push(chunk[1]);
    }
    println!(
        "STAGE 0 (correctly de-interleaved, pre-resample) L rms_db={:.3} R rms_db={:.3}",
        rms_db(&planar[0]),
        rms_db(&planar[1])
    );

    // ---- verbatim copy of export_mp3_acx's resample block (export.rs) ----
    let original_sr = 48000;
    let target_sr = 44100;
    let ratio = target_sr as f64 / original_sr as f64;
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler =
        SincFixedIn::<f32>::new(ratio, 2.0, params, 4096, 2).expect("resampler init");

    let total_frames = planar[0].len();
    let mut pos = 0usize;
    let mut resampled_planar: Vec<Vec<f32>> = vec![Vec::new(); 2];

    while pos < total_frames {
        let end = (pos + 4096).min(total_frames);
        let chunk_len = end - pos;
        let wave_in: Vec<Vec<f32>> = if chunk_len == 4096 {
            planar.iter().map(|ch| ch[pos..end].to_vec()).collect()
        } else {
            planar
                .iter()
                .map(|ch| {
                    let mut v = ch[pos..end].to_vec();
                    v.resize(4096, 0.0);
                    v
                })
                .collect()
        };

        let mut wave_out = resampler.process(&wave_in, None).expect("resample chunk");

        let valid_out_frames = (chunk_len as f64 * ratio).round() as usize;
        for c in 0..2 {
            wave_out[c].truncate(valid_out_frames);
            resampled_planar[c].extend_from_slice(&wave_out[c]);
        }
        pos += chunk_len;
    }
    println!(
        "STAGE 1 (after rubato resample 48k->44.1k, still stereo) L rms_db={:.3} R rms_db={:.3} (L frames={}, R frames={})",
        rms_db(&resampled_planar[0]),
        rms_db(&resampled_planar[1]),
        resampled_planar[0].len(),
        resampled_planar[1].len()
    );

    let mono: Vec<f32> = resampled_planar[0]
        .iter()
        .zip(resampled_planar[1].iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();
    println!(
        "STAGE 2 (after (L+R)*0.5 downmix to mono) rms_db={:.3} (frames={})",
        rms_db(&mono),
        mono.len()
    );

    println!("DONE");
}
