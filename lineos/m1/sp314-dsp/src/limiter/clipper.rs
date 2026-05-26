const TAPS: usize = 16;

const POLYPHASE_UP: [[f64; TAPS]; 4] = [
    [
        -1.7425427174407935e-06,
        0.00011046553246435567,
        -0.000941059720917635,
        0.0043679118522559806,
        -0.014474504103225153,
        0.038948457640802604,
        -0.09574957759528517,
        0.2844251695308281,
        0.8949722466459078,
        -0.15503552492948255,
        0.061170277669582385,
        -0.024209180253285462,
        0.008216810773212088,
        -0.0021377457248467195,
        0.0003592009444519958,
        -2.1705869358414087e-05
    ],
    [
        -8.866036690746866e-06,
        0.0002916564199290538,
        -0.002036367615763512,
        0.008550367479972973,
        -0.02662391480997065,
        0.06916203100308367,
        -0.17096592427329604,
        0.621632950152724,
        0.621632950152724,
        -0.17096592427329604,
        0.06916203100308367,
        -0.02662391480997066,
        0.008550367479972976,
        -0.0020363676157635156,
        0.0002916564199290543,
        -8.866036690748265e-06
    ],
    [
        -2.1705869358414087e-05,
        0.0003592009444519958,
        -0.0021377457248467195,
        0.008216810773212091,
        -0.02420918025328547,
        0.061170277669582405,
        -0.15503552492948255,
        0.8949722466459078,
        0.2844251695308281,
        -0.09574957759528517,
        0.03894845764080258,
        -0.014474504103225146,
        0.004367911852255977,
        -0.000941059720917635,
        0.00011046553246435567,
        -1.7425427174407935e-06
    ],
    [
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.9999971356592491,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0
    ]
];

const POLYPHASE_DOWN: [[f64; TAPS]; 4] = [
    [
        -4.356356793601984e-07,
        2.7616383116088917e-05,
        -0.00023526493022940875,
        0.0010919779630639951,
        -0.0036186260258062883,
        0.009737114410200651,
        -0.023937394398821293,
        0.07110629238270702,
        0.22374306166147695,
        -0.03875888123237064,
        0.015292569417395596,
        -0.006052295063321366,
        0.002054202693303022,
        -0.0005344364312116799,
        8.980023611299895e-05,
        -5.426467339603522e-06
    ],
    [
        -2.2165091726867164e-06,
        7.291410498226346e-05,
        -0.000509091903940878,
        0.002137591869993243,
        -0.006655978702492663,
        0.017290507750770918,
        -0.04274148106832401,
        0.155408237538181,
        0.155408237538181,
        -0.04274148106832401,
        0.017290507750770918,
        -0.006655978702492665,
        0.002137591869993244,
        -0.0005090919039408789,
        7.291410498226358e-05,
        -2.2165091726870662e-06
    ],
    [
        -5.426467339603522e-06,
        8.980023611299895e-05,
        -0.0005344364312116799,
        0.002054202693303023,
        -0.006052295063321367,
        0.015292569417395601,
        -0.03875888123237064,
        0.22374306166147695,
        0.07110629238270702,
        -0.023937394398821293,
        0.009737114410200646,
        -0.0036186260258062866,
        0.0010919779630639943,
        -0.00023526493022940875,
        2.7616383116088917e-05,
        -4.356356793601984e-07
    ],
    [
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.24999928391481227,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0
    ]
];

pub struct OversampledSoftClipper {
    // Upsampler delay lines (low rate: 1 sample per step)
    delay_up_l: [f32; TAPS],
    delay_up_r: [f32; TAPS],
    write_up:   usize,

    // Downsampler delay lines (4 phases per channel)
    // Each phase has its own history buffer
    delay_down_l: [[f32; TAPS]; 4],
    delay_down_r: [[f32; TAPS]; 4],
    write_down:   usize,

    enabled: bool,
}

#[inline]
fn soft_clip(x: f32) -> f32 {
    // Clamp to polynomial maximum (ceiling at x=±1.5)
    let xc = if x > 1.5_f32 { 1.5_f32 }
             else if x < -1.5_f32 { -1.5_f32 }
             else { x };
    // Unity Gain Soft Clipper: f(x) = x - (4/27)*x^3
    // f'(0) = 1.0  — transparent at low volumes
    // f(1.5) = 1.0 — exact brickwall ceiling (analytically proven)
    // Pure f32 arithmetic — zero libm calls
    xc - (4.0_f32 / 27.0_f32) * (xc * xc * xc)
}

impl OversampledSoftClipper {
    pub fn new(enabled: bool) -> Self {
        Self {
            delay_up_l:   [0.0_f32; TAPS],
            delay_up_r:   [0.0_f32; TAPS],
            write_up:     0,
            delay_down_l: [[0.0_f32; TAPS]; 4],
            delay_down_r: [[0.0_f32; TAPS]; 4],
            write_down:   0,
            enabled,
        }
    }

    pub fn reset(&mut self) {
        self.delay_up_l   = [0.0_f32; TAPS];
        self.delay_up_r   = [0.0_f32; TAPS];
        self.write_up     = 0;
        self.delay_down_l = [[0.0_f32; TAPS]; 4];
        self.delay_down_r = [[0.0_f32; TAPS]; 4];
        self.write_down   = 0;
    }

    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if !self.enabled {
            return (left, right);
        }

        // Advance upsampler write index (circular, backwards)
        self.write_up = (self.write_up + TAPS - 1) % TAPS;
        self.delay_up_l[self.write_up] = left;
        self.delay_up_r[self.write_up] = right;

        // Step A — Upsample (4 output samples) + Clip:
        let mut clipped_l = [0.0_f32; 4];
        let mut clipped_r = [0.0_f32; 4];
        for p in 0..4 {
            let mut acc_l = 0.0_f64;
            let mut acc_r = 0.0_f64;
            for tap in 0..TAPS {
                let idx = (self.write_up + tap) % TAPS;
                acc_l += POLYPHASE_UP[p][tap] * self.delay_up_l[idx] as f64;
                acc_r += POLYPHASE_UP[p][tap] * self.delay_up_r[idx] as f64;
            }
            clipped_l[p] = soft_clip(acc_l as f32);
            clipped_r[p] = soft_clip(acc_r as f32);
        }

        // Step B — Downsample (all 4 phases, then sum):
        // Advance downsampler write index (circular, backwards)
        self.write_down = (self.write_down + TAPS - 1) % TAPS;

        // Load 4 clipped samples into 4 phase histories
        // Reversed order (3-p) for correct FIR alignment
        for p in 0..4 {
            self.delay_down_l[p][self.write_down] = clipped_l[3 - p];
            self.delay_down_r[p][self.write_down] = clipped_r[3 - p];
        }

        // Run all 4 POLYPHASE_DOWN phases and sum
        let mut out_l = 0.0_f64;
        let mut out_r = 0.0_f64;
        for p in 0..4 {
            for tap in 0..TAPS {
                let idx = (self.write_down + tap) % TAPS;
                out_l += POLYPHASE_DOWN[p][tap] * self.delay_down_l[p][idx] as f64;
                out_r += POLYPHASE_DOWN[p][tap] * self.delay_down_r[p][idx] as f64;
            }
        }
        (out_l as f32, out_r as f32)
    }
}
