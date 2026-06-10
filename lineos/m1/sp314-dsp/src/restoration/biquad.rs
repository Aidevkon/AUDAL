// src/restoration/biquad.rs
// Standard Digital Biquad Filter — Audio EQ Cookbook formulas.
// Constitutional: libm only, no std::f32 math methods.

use libm::{cosf, sinf};

// core::f32::consts::PI is a compile-time constant — not a math function.
// Permitted by PHILOSOPHY.md (only math *functions* require libm).
const PI: f32 = core::f32::consts::PI;

pub enum FilterType {
    Notch,
    HighPass,
}

pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1_l: f32,
    z2_l: f32,
    z1_r: f32,
    z2_r: f32,
}

impl Biquad {
    pub fn new(filter_type: FilterType, freq: f32, q: f32, sample_rate: f32) -> Self {
        let omega = 2.0 * PI * freq / sample_rate;
        let alpha = sinf(omega) / (2.0 * q);
        let cos_w = cosf(omega);

        let (b0, b1, b2, a0, a1, a2) = match filter_type {
            FilterType::Notch => (
                1.0,
                -2.0 * cos_w,
                1.0,
                1.0 + alpha,
                -2.0 * cos_w,
                1.0 - alpha,
            ),
            FilterType::HighPass => (
                (1.0 + cos_w) / 2.0,
                -(1.0 + cos_w),
                (1.0 + cos_w) / 2.0,
                1.0 + alpha,
                -2.0 * cos_w,
                1.0 - alpha,
            ),
        };

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1_l: 0.0,
            z2_l: 0.0,
            z1_r: 0.0,
            z2_r: 0.0,
        }
    }

    #[inline(always)]
    pub fn process_stereo(&mut self, left: f32, right: f32) -> (f32, f32) {
        // Direct Form II transposed
        let out_l = self.b0 * left + self.z1_l;
        self.z1_l = self.b1 * left - self.a1 * out_l + self.z2_l;
        self.z2_l = self.b2 * left - self.a2 * out_l;

        let out_r = self.b0 * right + self.z1_r;
        self.z1_r = self.b1 * right - self.a1 * out_r + self.z2_r;
        self.z2_r = self.b2 * right - self.a2 * out_r;

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        self.z1_l = 0.0;
        self.z2_l = 0.0;
        self.z1_r = 0.0;
        self.z2_r = 0.0;
    }
}
