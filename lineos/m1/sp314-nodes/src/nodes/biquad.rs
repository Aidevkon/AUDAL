use crate::node::DspNode;
use libm::{cosf, powf, sinf, sqrtf};
use sp314_dsp::glider::ParameterGlider;

const PI: f32 = core::f32::consts::PI;

pub struct BiquadFilterNode {
    filter_type: f32,
    freq_hz: f32,
    q: f32,
    gain_db: f32,
    sample_rate: f32,

    freq_glider: ParameterGlider,
    gain_glider: ParameterGlider,
    q_glider: ParameterGlider,
    glide_ms: f32,

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

impl BiquadFilterNode {
    pub fn new(sample_rate: f32) -> Self {
        let mut node = Self {
            filter_type: 0.0, // LowPass
            freq_hz: 1000.0,
            q: 0.707,
            gain_db: 0.0,
            sample_rate,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1_l: 0.0,
            z2_l: 0.0,
            z1_r: 0.0,
            z2_r: 0.0,
            freq_glider: ParameterGlider::new(1000.0, 300.0, sample_rate),
            gain_glider: ParameterGlider::new(0.0, 300.0, sample_rate),
            q_glider: ParameterGlider::new(0.707, 300.0, sample_rate),
            glide_ms: 300.0,
        };
        node.recompute();
        node
    }

    fn recompute(&mut self) {
        let w0 = 2.0 * PI * self.freq_hz / self.sample_rate;
        let alpha = sinf(w0) / (2.0 * self.q);
        let cos_w = cosf(w0);
        let a = powf(10.0, self.gain_db / 40.0);
        let sqrt_a = sqrtf(a);

        let (b0, b1, b2, a0, a1, a2) = if self.filter_type == 0.0 {
            // LowPass
            (
                (1.0 - cos_w) / 2.0,
                1.0 - cos_w,
                (1.0 - cos_w) / 2.0,
                1.0 + alpha,
                -2.0 * cos_w,
                1.0 - alpha,
            )
        } else if self.filter_type == 1.0 {
            // HighPass
            (
                (1.0 + cos_w) / 2.0,
                -(1.0 + cos_w),
                (1.0 + cos_w) / 2.0,
                1.0 + alpha,
                -2.0 * cos_w,
                1.0 - alpha,
            )
        } else if self.filter_type == 2.0 {
            // Notch
            (
                1.0,
                -2.0 * cos_w,
                1.0,
                1.0 + alpha,
                -2.0 * cos_w,
                1.0 - alpha,
            )
        } else if self.filter_type == 3.0 {
            // Peaking
            (
                1.0 + alpha * a,
                -2.0 * cos_w,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos_w,
                1.0 - alpha / a,
            )
        } else if self.filter_type == 4.0 {
            // LowShelf
            (
                a * ((a + 1.0) - (a - 1.0) * cos_w + 2.0 * sqrt_a * alpha),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w),
                a * ((a + 1.0) - (a - 1.0) * cos_w - 2.0 * sqrt_a * alpha),
                (a + 1.0) + (a - 1.0) * cos_w + 2.0 * sqrt_a * alpha,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos_w),
                (a + 1.0) + (a - 1.0) * cos_w - 2.0 * sqrt_a * alpha,
            )
        } else {
            // HighShelf (5.0)
            (
                a * ((a + 1.0) + (a - 1.0) * cos_w + 2.0 * sqrt_a * alpha),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w),
                a * ((a + 1.0) + (a - 1.0) * cos_w - 2.0 * sqrt_a * alpha),
                (a + 1.0) - (a - 1.0) * cos_w + 2.0 * sqrt_a * alpha,
                2.0 * ((a - 1.0) - (a + 1.0) * cos_w),
                (a + 1.0) - (a - 1.0) * cos_w - 2.0 * sqrt_a * alpha,
            )
        };

        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }
}

const PARAMS: &[&str] = &["filter_type", "freq_hz", "q", "gain_db", "glide_ms"];

impl DspNode for BiquadFilterNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let mut recompute_needed = false;

            if self.freq_glider.is_gliding() {
                self.freq_hz = self.freq_glider.next();
                recompute_needed = true;
            }
            if self.gain_glider.is_gliding() {
                self.gain_db = self.gain_glider.next();
                recompute_needed = true;
            }
            if self.q_glider.is_gliding() {
                self.q = self.q_glider.next();
                recompute_needed = true;
            }

            if recompute_needed {
                self.recompute();
            }

            let out_l = self.b0 * *l + self.z1_l;
            self.z1_l = self.b1 * *l - self.a1 * out_l + self.z2_l;
            self.z2_l = self.b2 * *l - self.a2 * out_l;
            *l = out_l;

            let out_r = self.b0 * *r + self.z1_r;
            self.z1_r = self.b1 * *r - self.a1 * out_r + self.z2_r;
            self.z2_r = self.b2 * *r - self.a2 * out_r;
            *r = out_r;
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> bool {
        if !self.has_parameter(name) {
            return false;
        }
        let mut changed = false;
        match name {
            "filter_type" => {
                if self.filter_type != value {
                    self.filter_type = value;
                    changed = true;
                }
            }
            "freq_hz" => self.freq_glider.set_target(value),
            "q" => self.q_glider.set_target(value),
            "gain_db" => self.gain_glider.set_target(value),
            "glide_ms" => {
                self.glide_ms = value;
                self.freq_glider.set_glide_ms(value);
                self.gain_glider.set_glide_ms(value);
                self.q_glider.set_glide_ms(value);
            }
            _ => return false,
        }

        if changed {
            self.recompute();
        }
        true
    }

    fn set_parameter_no_glide(&mut self, name: &str, value: f32) -> bool {
        if !self.has_parameter(name) {
            return false;
        }
        let mut changed = false;
        match name {
            "filter_type" => {
                if self.filter_type != value {
                    self.filter_type = value;
                    changed = true;
                }
            }
            "freq_hz" => {
                if self.freq_hz != value {
                    self.freq_glider.set_target_instant(value);
                    self.freq_hz = value;
                    changed = true;
                }
            }
            "q" => {
                if self.q != value {
                    self.q_glider.set_target_instant(value);
                    self.q = value;
                    changed = true;
                }
            }
            "gain_db" => {
                if self.gain_db != value {
                    self.gain_glider.set_target_instant(value);
                    self.gain_db = value;
                    changed = true;
                }
            }
            "glide_ms" => {
                self.glide_ms = value;
                self.freq_glider.set_glide_ms(value);
                self.gain_glider.set_glide_ms(value);
                self.q_glider.set_glide_ms(value);
            }
            _ => return false,
        }

        if changed {
            self.recompute();
        }
        true
    }

    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }

    fn reset(&mut self) {
        self.z1_l = 0.0;
        self.z2_l = 0.0;
        self.z1_r = 0.0;
        self.z2_r = 0.0;
        self.freq_glider.reset();
        self.gain_glider.reset();
        self.q_glider.reset();
        self.freq_hz = self.freq_glider.value();
        self.gain_db = self.gain_glider.value();
        self.q = self.q_glider.value();
        self.recompute();
    }

    fn node_type(&self) -> &'static str {
        "BiquadFilter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::DspNode;

    /// Επαληθεύει το ΔΟΓΜΑ που δικαιολογεί το «ΚΟΙΜΑΤΑΙ» του LTASS
    /// (sp314-orchestrator/streaming_pipeline.rs, 2026-09-16): ένα
    /// Peaking biquad στο gain_db=0.0 πρέπει να είναι BIT-EXACT
    /// passthrough, μηδενικής καθυστέρησης — όχι «πολύ κοντά».
    ///
    /// Απόδειξη, αλγεβρικά, γιατί ισχύει (Direct Form II Transposed,
    /// γρ. 156-168 παραπάνω): στο gain_db=0.0, a = 10^0 = 1.0 ακριβώς,
    /// άρα b0 = (1+α·1)/(1+α/1) = 1.0 ακριβώς, και b1/a0 == a1/a0 ·
    /// b2/a0 == a2/a0 (ίδιοι αριθμητές, ίδιος διαιρέτης — bit-ταυτόσημα).
    /// Με z1=z2=0 αρχικά: out = 1.0·x + 0 = x ακριβώς· z1_new =
    /// (b1-a1)·x = 0·x = 0 ακριβώς (αφαίρεση ίδιας τιμής από τον εαυτό
    /// της)· z2_new ίδια λογική. Με επαγωγή, z1=z2=0 ΓΙΑ ΠΑΝΤΑ — άρα
    /// out[n] = x[n] ακριβώς, κάθε n. Το test παρακάτω το μετράει, δεν
    /// το υποθέτει.
    #[test]
    fn peaking_gain_zero_is_bit_exact_passthrough() {
        let mut node = BiquadFilterNode::new(48_000.0);
        node.set_parameter_no_glide("filter_type", 3.0); // Peaking
        node.set_parameter_no_glide("freq_hz", 6000.0); // ltass_band_6, το ακραίο περιστατικό
        node.set_parameter_no_glide("q", 0.707);
        node.set_parameter_no_glide("gain_db", 0.0);

        // Ντετερμινιστικός λευκός θόρυβος, ίδιος LCG με τα υπόλοιπα
        // όργανα αυτής της δουλειάς (0x5EED1234, 1664525/1013904223).
        let mut seed: u32 = 0x5EED1234;
        let n = 48_000; // 1s @ 48kHz
        let mut left: Vec<f32> = (0..n)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
            })
            .collect();
        let mut right = left.clone();
        let input = left.clone();

        node.process_stereo(&mut left, &mut right);

        for i in 0..n {
            assert_eq!(
                left[i].to_bits(),
                input[i].to_bits(),
                "sample {i}: gain=0 Peaking must be bit-exact passthrough (left), got {} vs input {}",
                left[i],
                input[i]
            );
            assert_eq!(
                right[i].to_bits(),
                input[i].to_bits(),
                "sample {i}: gain=0 Peaking must be bit-exact passthrough (right)"
            );
        }
    }
}
