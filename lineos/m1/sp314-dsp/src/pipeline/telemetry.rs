#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Telemetry {
    pub peak_db: f32,
    pub rms_db:  f32,
    pub lufs:    f32,
}

pub fn analyze_offline_pre_pass(left: &[f32], right: &[f32]) -> Telemetry {
    debug_assert_eq!(left.len(), right.len());

    let len = left.len();
    if len == 0 {
        return Telemetry { peak_db: -144.0, rms_db: -144.0, lufs: -144.0 };
    }

    let mut max_peak = 0.0_f32;
    let mut sum_sq   = 0.0_f32;
    let mut comp     = 0.0_f32;

    for i in 0..len {
        let abs_l = libm::fabsf(left[i]);
        let abs_r = libm::fabsf(right[i]);
        if abs_l > max_peak { max_peak = abs_l; }
        if abs_r > max_peak { max_peak = abs_r; }

        let y = (left[i] * left[i] + right[i] * right[i]) - comp;
        let t = sum_sq + y;
        comp  = (t - sum_sq) - y;
        sum_sq = t;
    }

    let peak_db = if max_peak < 1e-9_f32 {
        -144.0_f32
    } else {
        20.0_f32 * libm::log10f(max_peak)
    };

    let mean_sq = sum_sq / (2.0_f32 * len as f32);
    let rms_db = if mean_sq < 1e-15_f32 {
        -144.0_f32
    } else {
        10.0_f32 * libm::log10f(mean_sq)
    };

    let lufs = crate::metering::measure_integrated_lufs(left, right);

    Telemetry { peak_db, rms_db, lufs }
}
