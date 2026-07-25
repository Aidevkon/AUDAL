use crate::analysis::dynamics::rms_db;
use lineos_corpus::mfcc::MfccAnalyzer;

/// 10ms frame size at 48kHz system rate.
pub const FRAME_SAMPLES: usize = 480;

pub const TRANSIENT_THRESHOLD: f32 = 1.995_262_3; // +6dB
pub const MID_SIDE_SIDE_CAP: f32 = 100.0;

// ── 1. Spectral Flatness ────────────────────────────────────────────────────

pub fn frame_spectral_flatness(signal: &[f32]) -> f32 {
    let mut padded = vec![0.0_f32; 2048];
    let len = signal.len().min(2048);
    for i in 0..len {
        // 480-point Hann window to prevent rectangular cut-off leakage
        let w = 0.5 - 0.5 * libm::cosf(2.0 * core::f32::consts::PI * i as f32 / len as f32);
        padded[i] = signal[i] * w;
    }
    crate::analysis::spectral::spectral_flatness(&padded)
}

// ── 2. RMS Delta ────────────────────────────────────────────────────────────

pub struct RmsDeltaSensor {
    pub prev_rms_db: f32,
}

impl Default for RmsDeltaSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl RmsDeltaSensor {
    pub fn new() -> Self {
        Self {
            prev_rms_db: -144.0, // Match corpus/telemetry silence floor
        }
    }

    pub fn process(&mut self, frame: &[f32]) -> (f32, f32) {
        let r = rms_db(frame);
        let delta = r - self.prev_rms_db;
        self.prev_rms_db = r;
        (r, delta)
    }
}

// ── 3. Mid/Side Ratio ───────────────────────────────────────────────────────

pub fn mid_side_ratio(left: &[f32], right: &[f32]) -> f32 {
    let len = left.len().min(right.len());
    if len == 0 {
        return 0.0;
    }

    let mut mid_sq = 0.0_f32;
    let mut side_sq = 0.0_f32;

    for i in 0..len {
        let l = left[i];
        let r = right[i];
        let mid = (l + r) * 0.5;
        let side = (l - r) * 0.5;
        mid_sq += mid * mid;
        side_sq += side * side;
    }

    let mid_rms = libm::sqrtf(mid_sq / len as f32);
    let side_rms = libm::sqrtf(side_sq / len as f32);

    if side_rms < 1e-10 && mid_rms < 1e-10 {
        0.0 // Absolute silence
    } else if mid_rms < 1e-10 {
        MID_SIDE_SIDE_CAP // Pure side (anti-phase) -> cap ratio
    } else {
        side_rms / mid_rms
    }
}

// ── 4. Transient Density ────────────────────────────────────────────────────

pub struct TransientSensor {
    ring: Vec<f32>,
    pos: usize,
    fast_sum: f32,
    slow_sum: f32,
    was_above: bool,
}

impl Default for TransientSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl TransientSensor {
    pub fn new() -> Self {
        Self {
            ring: vec![0.0; 4800], // 100ms slow window history
            pos: 0,
            fast_sum: 0.0,
            slow_sum: 0.0,
            was_above: false,
        }
    }

    pub fn process(&mut self, frame: &[f32]) -> f32 {
        let mut count = 0;

        for &s in frame {
            let rect = libm::fabsf(s);
            let old_slow = self.ring[self.pos];
            // Fast window is 10ms (480 samples).
            let fast_idx = (self.pos + 4800 - 480) % 4800;
            let old_fast = self.ring[fast_idx];

            self.slow_sum += rect - old_slow;
            self.fast_sum += rect - old_fast;
            self.ring[self.pos] = rect;

            self.pos = (self.pos + 1) % 4800;

            let fast_ma = self.fast_sum / 480.0;
            let slow_ma = self.slow_sum / 4800.0;

            let is_above = fast_ma > slow_ma * TRANSIENT_THRESHOLD;
            if is_above && !self.was_above {
                count += 1;
            }
            self.was_above = is_above;
        }

        let duration = frame.len() as f32 / 48000.0;
        if duration <= 0.0 {
            0.0
        } else {
            count as f32 / duration
        }
    }
}

// ── 5. MFCC ─────────────────────────────────────────────────────────────────

pub struct MfccSensor {
    analyzer: MfccAnalyzer,
}

impl Default for MfccSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl MfccSensor {
    pub fn new() -> Self {
        Self {
            analyzer: MfccAnalyzer::new(),
        }
    }

    pub fn process(&mut self, frame: &[f32]) -> [f32; 13] {
        self.analyzer.compute(frame)
    }
}

// ── Oracles (Tests) ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_spectral_flatness() {
        // Real white noise via LCG -> high flatness
        let mut seed = 12345u32;
        let noise: Vec<f32> = (0..FRAME_SAMPLES)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
            })
            .collect();
        let flat_noise = frame_spectral_flatness(&noise);

        // Pure tone -> low flatness
        let tone: Vec<f32> = (0..FRAME_SAMPLES)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0))
            .collect();
        let flat_tone = frame_spectral_flatness(&tone);

        let flat_silence = frame_spectral_flatness(&vec![0.0; FRAME_SAMPLES]);

        assert!(
            flat_noise > 0.6,
            "Noise flatness should be high, was {}",
            flat_noise
        );
        assert!(
            flat_tone < 0.1,
            "Tone flatness should be low, was {}",
            flat_tone
        );
        assert!(flat_noise > flat_tone + 0.5, "Margin must be clear");
        assert_eq!(flat_silence, 0.5, "Silence default");
    }

    #[test]
    fn oracle_rms_delta() {
        let mut sensor = RmsDeltaSensor::new();
        let silence = vec![0.0; FRAME_SAMPLES];
        let tone: Vec<f32> = (0..FRAME_SAMPLES)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0) * 0.5)
            .collect();

        // 1. Silence -> Loud transition (from -144 floor)
        let (_, d1) = sensor.process(&tone);
        assert!(d1 > 100.0, "Large positive delta on onset");

        // 2. Steady tone
        let (_, d2) = sensor.process(&tone);
        assert!(libm::fabsf(d2) < 0.1, "Near zero delta on steady state");

        // 3. Loud -> Silence transition
        let (_, d3) = sensor.process(&silence);
        assert!(d3 < -100.0, "Large negative delta on offset");
    }

    #[test]
    fn oracle_mid_side() {
        let mono: Vec<f32> = (0..FRAME_SAMPLES).map(|i| i as f32 / 480.0).collect();
        let side: Vec<f32> = mono.iter().map(|&x| -x).collect();

        // Identical L/R (pure mid) -> side=0
        let r_mono = mid_side_ratio(&mono, &mono);
        assert_eq!(r_mono, 0.0);

        // Anti-phase (pure side) -> mid=0 -> ratio large
        let r_wide = mid_side_ratio(&mono, &side);
        assert_eq!(r_wide, MID_SIDE_SIDE_CAP);

        // Silence
        let r_silence = mid_side_ratio(&vec![0.0; 480], &vec![0.0; 480]);
        assert_eq!(r_silence, 0.0);
    }

    #[test]
    fn oracle_transient() {
        let mut sensor = TransientSensor::new();
        let silence = vec![0.0; FRAME_SAMPLES];
        let tone: Vec<f32> = (0..FRAME_SAMPLES)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0) * 0.1)
            .collect();

        // Feed context (10 frames = 100ms)
        for _ in 0..10 {
            let d = sensor.process(&silence);
            assert_eq!(d, 0.0);
        }

        // Impulse onset
        let mut impulse = silence.clone();
        impulse[0] = 1.0;
        let d_spike = sensor.process(&impulse);
        assert!(d_spike >= 100.0, "Spikes on onset frame: {}", d_spike);

        // Steady tone
        for _ in 0..5 {
            sensor.process(&tone);
        }
        let d_steady = sensor.process(&tone);
        assert_eq!(d_steady, 0.0, "No transient in steady tone");
    }

    #[test]
    fn oracle_mfcc() {
        let mut sensor = MfccSensor::new();
        let tone_low: Vec<f32> = (0..FRAME_SAMPLES)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 100.0 * i as f32 / 48000.0))
            .collect();
        let tone_high: Vec<f32> = (0..FRAME_SAMPLES)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 8000.0 * i as f32 / 48000.0))
            .collect();

        let v_low = sensor.process(&tone_low);
        let v_high = sensor.process(&tone_high);

        // Assert vectors differ
        let mut diff_sum = 0.0;
        for i in 0..13 {
            diff_sum += libm::fabsf(v_low[i] - v_high[i]);
        }
        assert!(diff_sum > 1.0, "Timbres must yield different MFCCs");

        // Assert stateless compute matches
        let v_low_2 = sensor.process(&tone_low);
        for i in 0..13 {
            assert!(
                (v_low[i] - v_low_2[i]).abs() < 1e-5,
                "Subsequent identical inputs yield identical MFCCs"
            );
        }
    }
}
