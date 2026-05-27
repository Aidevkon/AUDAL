// src/harmonic/mod.rs
// HarmonicEngine with 2x polyphase oversampling.
// Eliminates aliasing from tanhf nonlinearity.
// Same FIR pattern as OversampledSoftClipper (v3.5).

const TAPS:     usize = 32;  // taps_per_phase from JSON
const N_PHASES: usize = 2;   // 2x oversample

// Anti-imaging upsampler (scaled x2) from JSON antiimaging_upsampler.phases
const POLYPHASE_UP: [[f32; TAPS]; N_PHASES] = [
    // Phase 0: from JSON antiimaging_upsampler.phases[0]
    [
        -1.2321681303767633e-06, 1.5348421704599117e-05, -7.811120338420253e-05,
        0.0002539943220473859, -0.0006654320638973712, 0.0015116198453222725,
        -0.003088591015174136, 0.005810183168971523, -0.010235056208662337,
        0.017118536074946105, -0.027540815930647007, 0.04325407114349144,
        -0.06770541509778288, 0.1096270587701283, -0.201119677506444,
        0.6328431830402174, 0.6328431830402174, -0.201119677506444,
        0.1096270587701283, -0.06770541509778288, 0.04325407114349142,
        -0.027540815930646993, 0.0171185360749461, -0.010235056208662332,
        0.005810183168971522, -0.003088591015174134, 0.0015116198453222725,
        -0.0006654320638973712, 0.0002539943220473859, -7.811120338420253e-05,
        1.5348421704599117e-05, -1.2321681303767633e-06
    ],
    // Phase 1: from JSON antiimaging_upsampler.phases[1]
    [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        1.0000006728145863, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0
    ],
];

// Anti-aliasing decimator (no scaling) from JSON antialiasing_decimator.phases
const POLYPHASE_DOWN: [[f32; TAPS]; N_PHASES] = [
    // Phase 0
    [
        -6.160840651883816e-07, 7.674210852299558e-06, -3.9055601692101265e-05,
        0.00012699716102369295, -0.0003327160319486856, 0.0007558099226611363,
        -0.001544295507587068, 0.0029050915844857617, -0.005117528104331168,
        0.008559268037473053, -0.013770407965323504, 0.02162703557174572,
        -0.03385270754889144, 0.05481352938506415, -0.100559838753222,
        0.3164215915201087, 0.3164215915201087, -0.100559838753222,
        0.05481352938506415, -0.03385270754889144, 0.02162703557174571,
        -0.013770407965323497, 0.00855926803747305, -0.005117528104331166,
        0.002905091584485761, -0.001544295507587067, 0.0007558099226611363,
        -0.0003327160319486856, 0.00012699716102369295, -3.9055601692101265e-05,
        7.674210852299558e-06, -6.160840651883816e-07
    ],
    // Phase 1
    [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.5000003364072931, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0
    ],
];

#[derive(Clone, Debug, Copy)]
pub struct HarmonicConfig {
    pub drive:              f32,
    pub drive_compensation: f32,
    pub even_amount:        f32,
    pub odd_amount:         f32,
    pub mix:                f32,
}

impl Default for HarmonicConfig {
    fn default() -> Self {
        Self {
            drive:              2.0,
            drive_compensation: 1.0,
            even_amount:        0.6,
            odd_amount:         0.2,
            mix:                0.3,
        }
    }
}

pub struct HarmonicEngine {
    config:      HarmonicConfig,
    // Upsampler delay lines (one per channel)
    delay_up_m:  [f32; TAPS],
    delay_up_s:  [f32; TAPS],
    write_up:    usize,
    // Downsampler delay lines (2 phases per channel)
    delay_down_m: [[f32; TAPS]; N_PHASES],
    delay_down_s: [[f32; TAPS]; N_PHASES],
    write_down:   usize,
}

impl HarmonicEngine {
    pub fn new(config: HarmonicConfig) -> Self {
        Self {
            config,
            delay_up_m:   [0.0_f32; TAPS],
            delay_up_s:   [0.0_f32; TAPS],
            write_up:     0,
            delay_down_m: [[0.0_f32; TAPS]; N_PHASES],
            delay_down_s: [[0.0_f32; TAPS]; N_PHASES],
            write_down:   0,
        }
    }

    #[inline]
    fn waveshape(&self, x: f32) -> f32 {
        if self.config.mix == 0.0 { return x; }
        let drive = self.config.drive
                  * self.config.drive_compensation;
        let y_odd  = libm::tanhf(drive * x);
        let y_even = x + (x * libm::fabsf(x)) * 0.5_f32;
        let wet    = self.config.even_amount * y_even
                   + self.config.odd_amount  * y_odd;
        let wet_c  = libm::tanhf(wet);
        x * (1.0_f32 - self.config.mix)
            + wet_c * self.config.mix
    }

    #[inline]
    pub fn process_frame(&mut self, mid: f32, side: f32)
        -> (f32, f32)
    {
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
            self.delay_down_m[p][self.write_down] =
                shaped_m[N_PHASES - 1 - p];
            self.delay_down_s[p][self.write_down] =
                shaped_s[N_PHASES - 1 - p];
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
        self.delay_up_m   = [0.0_f32; TAPS];
        self.delay_up_s   = [0.0_f32; TAPS];
        self.write_up     = 0;
        self.delay_down_m = [[0.0_f32; TAPS]; N_PHASES];
        self.delay_down_s = [[0.0_f32; TAPS]; N_PHASES];
        self.write_down   = 0;
    }
}
