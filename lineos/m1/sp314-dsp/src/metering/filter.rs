// src/metering/filter.rs
// K-Weighting filter per ITU-R BS.1770-4.
// Two biquad stages: High-Shelf pre-filter + High-Pass filter.
// Hardcoded for 48000 Hz — engine is 48kHz-only by constitutional rule.
// Coefficients from ITU-R BS.1770-4 Table 1.

// Stage 1: High-Shelf pre-filter
// Models acoustic effect of the human head (+4dB boost around 1681 Hz)
const HS_B0: f32 =  1.535_124_9_f32;
const HS_B1: f32 = -2.691_696_2_f32;
const HS_B2: f32 =  1.198_392_9_f32;
const HS_A1: f32 = -1.690_659_3_f32;
const HS_A2: f32 =  0.732_480_76_f32;

// Stage 2: High-Pass filter (2nd order, 100 Hz)
// Removes sub-bass inaudible to human hearing
const HP_B0: f32 =  1.0_f32;
const HP_B1: f32 = -2.0_f32;
const HP_B2: f32 =  1.0_f32;
const HP_A1: f32 = -1.990_047_5_f32;
const HP_A2: f32 =  0.990_072_25_f32;

/// K-Weighting filter state for one channel.
/// Two TDF-II biquad stages in series.
pub struct KWeightingFilter {
    // Stage 1 (high shelf) TDF-II state
    hs_w1: f32,
    hs_w2: f32,
    // Stage 2 (high pass) TDF-II state
    hp_w1: f32,
    hp_w2: f32,
}

impl KWeightingFilter {
    pub const fn new() -> Self {
        Self { hs_w1: 0.0, hs_w2: 0.0, hp_w1: 0.0, hp_w2: 0.0 }
    }
}

impl Default for KWeightingFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl KWeightingFilter {
    /// Process one sample through both filter stages.
    /// All math: libm only.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        // Stage 1: High-shelf (TDF-II)
        let hs_y = HS_B0 * x + self.hs_w1;
        self.hs_w1 = HS_B1 * x - HS_A1 * hs_y + self.hs_w2;
        self.hs_w2 = HS_B2 * x - HS_A2 * hs_y;

        // Anti-denormal
        if libm::fabsf(self.hs_w1) < 1e-15 { self.hs_w1 = 0.0; }
        if libm::fabsf(self.hs_w2) < 1e-15 { self.hs_w2 = 0.0; }

        // Stage 2: High-pass (TDF-II)
        let hp_y = HP_B0 * hs_y + self.hp_w1;
        self.hp_w1 = HP_B1 * hs_y - HP_A1 * hp_y + self.hp_w2;
        self.hp_w2 = HP_B2 * hs_y - HP_A2 * hp_y;

        // Anti-denormal
        if libm::fabsf(self.hp_w1) < 1e-15 { self.hp_w1 = 0.0; }
        if libm::fabsf(self.hp_w2) < 1e-15 { self.hp_w2 = 0.0; }

        hp_y
    }

    pub fn reset(&mut self) {
        self.hs_w1 = 0.0; self.hs_w2 = 0.0;
        self.hp_w1 = 0.0; self.hp_w2 = 0.0;
    }
}
