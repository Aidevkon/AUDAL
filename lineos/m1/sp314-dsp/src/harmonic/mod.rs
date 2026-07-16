// allow: polyphase oversampled waveshaper, circular delay-line
// indexing and reversed-phase alignment; indices are the math.
#![allow(clippy::needless_range_loop)]

// src/harmonic/mod.rs
// HarmonicEngine with 2x polyphase oversampling.
// Eliminates aliasing from tanhf nonlinearity.
// Same FIR pattern as OversampledSoftClipper (v3.5).

const TAPS: usize = 32; // taps_per_phase from JSON
const N_PHASES: usize = 2; // 2x oversample

// Anti-imaging upsampler (scaled x2) from JSON antiimaging_upsampler.phases
const POLYPHASE_UP: [[f32; TAPS]; N_PHASES] = [
    // Phase 0: from JSON antiimaging_upsampler.phases[0]
    [
        -1.232_168_1e-6,
        1.534_842_2e-5,
        -7.811_12e-5,
        0.000_253_994_3,
        -0.000_665_432_1,
        0.001_511_619_9,
        -0.003_088_591,
        0.005_810_183,
        -0.010_235_056,
        0.017_118_536,
        -0.027_540_816,
        0.043_254_07,
        -0.067_705_415,
        0.109_627_06,
        -0.201_119_68,
        0.632_843_2,
        0.632_843_2,
        -0.201_119_68,
        0.109_627_06,
        -0.067_705_415,
        0.043_254_07,
        -0.027_540_816,
        0.017_118_536,
        -0.010_235_056,
        0.005_810_183,
        -0.003_088_591,
        0.001_511_619_9,
        -0.000_665_432_1,
        0.000_253_994_3,
        -7.811_12e-5,
        1.534_842_2e-5,
        -1.232_168_1e-6,
    ],
    // Phase 1: from JSON antiimaging_upsampler.phases[1]
    [
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.000_000_7,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
];

// Anti-aliasing decimator (no scaling) from JSON antialiasing_decimator.phases
const POLYPHASE_DOWN: [[f32; TAPS]; N_PHASES] = [
    // Phase 0
    [
        -6.160_840_4e-7,
        7.674_211e-6,
        -3.905_56e-5,
        0.000_126_997_15,
        -0.000_332_716_04,
        0.000_755_809_95,
        -0.001_544_295_5,
        0.002_905_091_5,
        -0.005_117_528,
        0.008_559_268,
        -0.013_770_408,
        0.021_627_035,
        -0.033_852_708,
        0.054_813_53,
        -0.100_559_84,
        0.316_421_6,
        0.316_421_6,
        -0.100_559_84,
        0.054_813_53,
        -0.033_852_708,
        0.021_627_035,
        -0.013_770_408,
        0.008_559_268,
        -0.005_117_528,
        0.002_905_091_5,
        -0.001_544_295_5,
        0.000_755_809_95,
        -0.000_332_716_04,
        0.000_126_997_15,
        -3.905_56e-5,
        7.674_211e-6,
        -6.160_840_4e-7,
    ],
    // Phase 1
    [
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.500_000_36,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
];

#[derive(Clone, Debug, Copy)]
pub struct HarmonicConfig {
    pub drive: f32,
    pub drive_compensation: f32,
    pub even_amount: f32,
    pub odd_amount: f32,
    pub mix: f32,
}

impl Default for HarmonicConfig {
    fn default() -> Self {
        Self {
            drive: 2.0,
            drive_compensation: 1.0,
            even_amount: 0.6,
            odd_amount: 0.2,
            mix: 0.3,
        }
    }
}

pub struct HarmonicEngine {
    config: HarmonicConfig,
    // Upsampler delay lines (one per channel)
    delay_up_m: [f32; TAPS],
    delay_up_s: [f32; TAPS],
    write_up: usize,
    // Downsampler delay lines (2 phases per channel)
    delay_down_m: [[f32; TAPS]; N_PHASES],
    delay_down_s: [[f32; TAPS]; N_PHASES],
    write_down: usize,
}

impl HarmonicEngine {
    pub fn new(config: HarmonicConfig) -> Self {
        Self {
            config,
            delay_up_m: [0.0_f32; TAPS],
            delay_up_s: [0.0_f32; TAPS],
            write_up: 0,
            delay_down_m: [[0.0_f32; TAPS]; N_PHASES],
            delay_down_s: [[0.0_f32; TAPS]; N_PHASES],
            write_down: 0,
        }
    }

    /// Update drive compensation at runtime.
    /// Called by the pipeline to match actual input pad.
    #[inline]
    pub fn set_drive_compensation(&mut self, compensation: f32) {
        self.config.drive_compensation = compensation;
    }

    #[inline]
    fn waveshape(&self, x: f32) -> f32 {
        if self.config.mix == 0.0 {
            return x;
        }
        let drive = self.config.drive * self.config.drive_compensation;
        let y_odd = libm::tanhf(drive * x);
        let y_even = x + (x * libm::fabsf(x)) * 0.5_f32;
        let wet = self.config.even_amount * y_even + self.config.odd_amount * y_odd;
        let wet_c = libm::tanhf(wet);
        x * (1.0_f32 - self.config.mix) + wet_c * self.config.mix
    }

    #[inline]
    pub fn process_frame(&mut self, mid: f32, side: f32) -> (f32, f32) {
        if self.config.mix == 0.0 {
            return (mid, side);
        }

        // Write input to upsampler delay line
        self.write_up = (self.write_up + TAPS - 1) % TAPS;
        self.delay_up_m[self.write_up] = mid;
        self.delay_up_s[self.write_up] = side;

        // Step A: Upsample → 2 samples, waveshape each
        let mut shaped_m = [0.0_f32; N_PHASES];
        let mut shaped_s = [0.0_f32; N_PHASES];
        for p in 0..N_PHASES {
            let mut acc_m = 0.0_f64;
            let mut acc_s = 0.0_f64;
            for tap in 0..TAPS {
                let idx = (self.write_up + tap) % TAPS;
                let c = POLYPHASE_UP[p][tap] as f64;
                acc_m += c * self.delay_up_m[idx] as f64;
                acc_s += c * self.delay_up_s[idx] as f64;
            }
            shaped_m[p] = self.waveshape(acc_m as f32);
            shaped_s[p] = self.waveshape(acc_s as f32);
        }

        // Step B: Downsample → 1 output sample
        self.write_down = (self.write_down + TAPS - 1) % TAPS;
        for p in 0..N_PHASES {
            self.delay_down_m[p][self.write_down] = shaped_m[N_PHASES - 1 - p];
            self.delay_down_s[p][self.write_down] = shaped_s[N_PHASES - 1 - p];
        }

        let mut out_m = 0.0_f64;
        let mut out_s = 0.0_f64;
        for p in 0..N_PHASES {
            for tap in 0..TAPS {
                let idx = (self.write_down + tap) % TAPS;
                let c = POLYPHASE_DOWN[p][tap] as f64;
                out_m += c * self.delay_down_m[p][idx] as f64;
                out_s += c * self.delay_down_s[p][idx] as f64;
            }
        }

        (out_m as f32, out_s as f32)
    }

    pub fn reset(&mut self) {
        self.delay_up_m = [0.0_f32; TAPS];
        self.delay_up_s = [0.0_f32; TAPS];
        self.write_up = 0;
        self.delay_down_m = [[0.0_f32; TAPS]; N_PHASES];
        self.delay_down_s = [[0.0_f32; TAPS]; N_PHASES];
        self.write_down = 0;
    }
}
