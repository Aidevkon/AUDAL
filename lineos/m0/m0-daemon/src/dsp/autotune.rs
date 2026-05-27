use lineos_types::{StereoBuffer, MasteringIntent};
use sp314_dsp::metering::measure_integrated_lufs;
use crate::dsp::DspAdapter;

pub const AUTOTUNE_MAX_ITERATIONS: usize = 8;
pub const AUTOTUNE_TOLERANCE_DB:   f32   = 0.2;
pub const AUTOTUNE_MIN_GAIN_DB:    f32   = -18.0;
pub const AUTOTUNE_MAX_GAIN_DB:    f32   =  18.0;
pub const FULL_FILE_THRESHOLD:     usize = 48_000 * 30; // 30sec

#[derive(Debug, Clone, Copy)]
pub struct AutotuneResult {
    pub input_gain_db:  f32,
    pub achieved_lufs:  f32,
    pub iterations:     usize,
    pub converged:      bool,
}

/// Find optimal input gain so DspAdapter hits target_lufs.
/// Uses binary search over the EXACT production pipeline.
/// No more engine divergence — same graph, same result.
pub fn autotune_dsp(
    audio:       &StereoBuffer,
    intent:      &MasteringIntent,
) -> AutotuneResult {
    let target_lufs = intent.target.target_lufs;
    let sample_rate = audio.sample_rate;

    // Use full file if ≤30sec, else highest energy 2sec chunk
    let (ref_l, ref_r) = if audio.left.len() <= FULL_FILE_THRESHOLD {
        (audio.left.clone(), audio.right.clone())
    } else {
        // Find highest energy 2-second window
        let chunk = 96_000_usize.min(audio.left.len());
        let hop   = 48_000_usize;
        let mut best_start = 0usize;
        let mut best_energy = 0.0_f32;
        let mut start = 0;
        while start + chunk <= audio.left.len() {
            let energy: f32 = audio.left[start..start+chunk].iter()
                .zip(audio.right[start..start+chunk].iter())
                .map(|(l, r)| l*l + r*r)
                .sum();
            if energy > best_energy {
                best_energy = energy;
                best_start  = start;
            }
            start += hop;
        }
        let end = (best_start + chunk).min(audio.left.len());
        (audio.left[best_start..end].to_vec(),
         audio.right[best_start..end].to_vec())
    };

    let mut min_gain = AUTOTUNE_MIN_GAIN_DB;
    let mut max_gain = AUTOTUNE_MAX_GAIN_DB;
    let mut best_gain = 0.0_f32;
    let mut best_lufs = -144.0_f32;
    let mut iterations = 0usize;

    for _ in 0..AUTOTUNE_MAX_ITERATIONS {
        iterations += 1;
        let mid = (min_gain + max_gain) / 2.0_f32;

        // Apply input gain to test buffer
        let gain_linear = 10.0_f32.powf(mid / 20.0_f32);
        let mut test_audio = StereoBuffer {
            left:       ref_l.iter().map(|s| s * gain_linear).collect(),
            right:      ref_r.iter().map(|s| s * gain_linear).collect(),
            sample_rate,
            num_frames: ref_l.len(),
        };

        // Run EXACT production pipeline
        let mut test_intent = intent.clone();
        test_intent.target_makeup_db = 0.0;
        let _ = DspAdapter::master(&test_intent, &mut test_audio);

        // Measure output LUFS
        let output_lufs = measure_integrated_lufs(
            &test_audio.left, &test_audio.right);

        if output_lufs < -69.0 {
            min_gain = mid;
            continue;
        }

        if output_lufs < target_lufs {
            // Too quiet — push up
            min_gain  = mid;
            best_gain = mid;
            best_lufs = output_lufs;
        } else {
            // Too loud — pull back
            max_gain = mid;
        }

        if max_gain - min_gain < AUTOTUNE_TOLERANCE_DB { break; }
    }

    AutotuneResult {
        input_gain_db: best_gain,
        achieved_lufs: best_lufs,
        iterations,
        converged: (max_gain - min_gain) < AUTOTUNE_TOLERANCE_DB,
    }
}
