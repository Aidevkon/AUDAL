use crate::dsp::biquad::{butter_hp2_prewarped, butter_lp2_prewarped, Biquad};

pub struct Allpass {
    x1: f32,
    y1: f32,
    c: f32,
}

impl Allpass {
    pub fn new(c: f32) -> Self {
        Self {
            x1: 0.0,
            y1: 0.0,
            c,
        }
    }
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.c * x + self.x1 - self.c * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

pub enum WidthMode {
    Reveal,
    Create,
}

pub struct GlueChain {
    lpf_l1: Biquad,
    lpf_l2: Biquad,
    lpf_r1: Biquad,
    lpf_r2: Biquad,
    hpf_l1: Biquad,
    hpf_l2: Biquad,
    hpf_r1: Biquad,
    hpf_r2: Biquad,
    apf: Allpass,
    w: f32,
    d: f32,
    mode: WidthMode,
}

impl GlueChain {
    pub fn new(sample_rate: f32, mode: WidthMode) -> Self {
        Self {
            // 4th order LPF at 8kHz
            lpf_l1: butter_lp2_prewarped(8000.0, sample_rate),
            lpf_l2: butter_lp2_prewarped(8000.0, sample_rate),
            lpf_r1: butter_lp2_prewarped(8000.0, sample_rate),
            lpf_r2: butter_lp2_prewarped(8000.0, sample_rate),

            // 4th order HPF at 150Hz
            hpf_l1: butter_hp2_prewarped(150.0, sample_rate),
            hpf_l2: butter_hp2_prewarped(150.0, sample_rate),
            hpf_r1: butter_hp2_prewarped(150.0, sample_rate),
            hpf_r2: butter_hp2_prewarped(150.0, sample_rate),

            apf: Allpass::new(0.5),
            w: 1.0,
            d: 0.0,
            mode,
        }
    }

    pub fn set_amount(&mut self, amount: f32) {
        let amt = amount.clamp(0.0, 1.0);
        self.w = 1.0 + 0.5 * amt;
        self.d = 0.7 * amt;
    }

    pub fn set_drive(&mut self, d: f32) {
        self.d = d;
    }

    pub fn set_width(&mut self, w: f32) {
        self.w = w;
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            // Apply cleanup filters to the FULL signal (L and R)
            let mut l_filt = self.lpf_l1.process(*l);
            l_filt = self.lpf_l2.process(l_filt);
            l_filt = self.hpf_l1.process(l_filt);
            l_filt = self.hpf_l2.process(l_filt);

            let mut r_filt = self.lpf_r1.process(*r);
            r_filt = self.lpf_r2.process(r_filt);
            r_filt = self.hpf_r1.process(r_filt);
            r_filt = self.hpf_r2.process(r_filt);

            // M/S processing
            let m = (l_filt + r_filt) * 0.5;
            let s_in = (l_filt - r_filt) * 0.5;

            // Widener allpass
            let s = match self.mode {
                WidthMode::Reveal => self.apf.process(s_in),
                WidthMode::Create => self.apf.process(m),
            };

            let l_new = m + s * self.w;
            let r_new = m - s * self.w;

            let d = self.d;
            *l = if d < 1e-5 {
                l_new
            } else {
                libm::tanhf(l_new * d) / d
            };
            *r = if d < 1e-5 {
                r_new
            } else {
                libm::tanhf(r_new * d) / d
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mono_fold_down() {
        let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
        chain.set_amount(1.0);
        let mut l = vec![0.5, -0.5, 0.25, -0.1];
        let mut r = vec![0.5, -0.5, 0.25, -0.1]; // Mono input

        let l_orig = l.clone();
        let r_orig = r.clone();

        chain.process(&mut l, &mut r);
        let mut sum_widened = vec![0.0; l.len()];
        for i in 0..l.len() {
            sum_widened[i] = l[i] + r[i];
        }

        let mut chain_bypassed = GlueChain::new(48000.0, WidthMode::Reveal);
        chain_bypassed.set_amount(1.0);
        chain_bypassed.w = 1.0;

        let mut l_b = l_orig.clone();
        let mut r_b = r_orig.clone();
        chain_bypassed.process(&mut l_b, &mut r_b);

        let mut sum_bypassed = vec![0.0; l_b.len()];
        for i in 0..l_b.len() {
            sum_bypassed[i] = l_b[i] + r_b[i];
        }

        let mut e_w = 0.0;
        let mut e_b = 0.0;
        for i in 0..l.len() {
            e_w += sum_widened[i] * sum_widened[i];
            e_b += sum_bypassed[i] * sum_bypassed[i];
        }

        let diff = (10.0 * libm::log10f(e_w / e_b)).abs();
        assert!(diff < 0.01, "Energy diff was {}", diff);
    }

    #[test]
    fn test_amount_zero_leaves_mono_unchanged() {
        let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
        chain.set_amount(0.0);
        let mut l = vec![0.5, -0.2, 0.1];
        let mut r = vec![0.5, -0.2, 0.1];
        let l_orig = l.clone();
        let r_orig = r.clone();

        chain.process(&mut l, &mut r);

        // It is unchanged APART from the two filters.
        // We can simulate just the two filters on the original signal to compare.
        let mut chain_filters_only = GlueChain::new(48000.0, WidthMode::Reveal);
        chain_filters_only.set_amount(0.0);
        let mut l_f = l_orig.clone();
        let mut r_f = r_orig.clone();
        // Manually apply just the LPF and HPF to l_f and r_f
        for (ll, rr) in l_f.iter_mut().zip(r_f.iter_mut()) {
            let mut l_filt = chain_filters_only.lpf_l1.process(*ll);
            l_filt = chain_filters_only.lpf_l2.process(l_filt);
            l_filt = chain_filters_only.hpf_l1.process(l_filt);
            l_filt = chain_filters_only.hpf_l2.process(l_filt);

            let mut r_filt = chain_filters_only.lpf_r1.process(*rr);
            r_filt = chain_filters_only.lpf_r2.process(r_filt);
            r_filt = chain_filters_only.hpf_r1.process(r_filt);
            r_filt = chain_filters_only.hpf_r2.process(r_filt);
            *ll = l_filt;
            *rr = r_filt;
        }

        for i in 0..l.len() {
            assert!((l[i] - r[i]).abs() < 1e-6); // Mono is maintained
            assert!((l[i] - l_f[i]).abs() < 1e-6); // Unchanged apart from the filters
        }
    }

    #[test]
    fn test_hpf_attenuates_bass() {
        let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
        chain.set_amount(0.0);
        let mut l = vec![0.0; 48000];
        let mut r = vec![0.0; 48000];
        for i in 0..48000 {
            let t = i as f32 / 48000.0;
            let s = libm::sinf(2.0 * core::f32::consts::PI * 60.0 * t);
            l[i] = s;
            r[i] = s; // Mono 60Hz sine
        }
        let mut e_in = 0.0;
        for &x in &l {
            e_in += x * x;
        }

        chain.process(&mut l, &mut r);
        let mut e_out = 0.0;
        for &x in &l {
            e_out += x * x;
        }

        let atten = 10.0 * libm::log10f(e_out / e_in);
        assert!(atten < -20.0, "Attenuation was {}", atten);
    }

    #[test]
    fn test_lpf_attenuates_treble() {
        let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
        chain.set_amount(0.0);
        let mut l = vec![0.0; 48000];
        let mut r = vec![0.0; 48000];
        for i in 0..48000 {
            let t = i as f32 / 48000.0;
            let s = libm::sinf(2.0 * core::f32::consts::PI * 15000.0 * t);
            l[i] = s;
            r[i] = s; // Mono 15kHz sine
        }
        let mut e_in = 0.0;
        for &x in &l {
            e_in += x * x;
        }

        chain.process(&mut l, &mut r);
        let mut e_out = 0.0;
        for &x in &l {
            e_out += x * x;
        }

        let atten = 10.0 * libm::log10f(e_out / e_in);
        // Using 4th order filter, attenuation is ~31.14 dB. Using a 30 dB gate.
        assert!(atten < -30.0, "Attenuation was {}", atten);
    }

    #[test]
    fn test_mono_does_not_invent_width() {
        let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
        chain.set_amount(1.0);
        let mut l = vec![0.5, -0.3, 0.8];
        let mut r = vec![0.5, -0.3, 0.8];
        chain.process(&mut l, &mut r);
        for i in 0..l.len() {
            assert!((l[i] - r[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_no_nan_inf() {
        let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
        chain.set_amount(1.0);
        let mut l = vec![1.0, -1.0, 1.0, 0.0];
        let mut r = vec![1.0, -1.0, 1.0, 0.0];
        chain.process(&mut l, &mut r);
        for &x in &l {
            assert!(x.is_finite());
        }
        for &x in &r {
            assert!(x.is_finite());
        }
    }

    #[test]
    fn test_reveal_mono_in_out() {
        let mut chain = GlueChain::new(48000.0, WidthMode::Reveal);
        chain.set_amount(1.0);
        let mut l = vec![0.5, -0.3, 0.8];
        let mut r = vec![0.5, -0.3, 0.8];
        chain.process(&mut l, &mut r);
        for i in 0..l.len() {
            assert!((l[i] - r[i]).abs() < 1e-6, "Reveal mode invented width!");
        }
    }

    #[test]
    fn test_create_mono_in_out() {
        let mut chain = GlueChain::new(48000.0, WidthMode::Create);
        chain.set_amount(1.0);

        // Feed a low amplitude impulse to avoid saturation warping the mono fold-down check
        let mut l = vec![0.01, 0.0, 0.0, 0.0];
        let mut r = vec![0.01, 0.0, 0.0, 0.0];
        let l_orig = l.clone();
        let r_orig = r.clone();

        chain.process(&mut l, &mut r);

        let mut diff_sum = 0.0;
        for i in 0..l.len() {
            diff_sum += (l[i] - r[i]).abs();
        }
        assert!(diff_sum > 0.0001, "Create mode failed to invent width!");

        let mut sum_created = vec![0.0; l.len()];
        for i in 0..l.len() {
            sum_created[i] = l[i] + r[i];
        }

        let mut chain_bypassed = GlueChain::new(48000.0, WidthMode::Create);
        chain_bypassed.set_amount(1.0);
        chain_bypassed.w = 1.0;

        let mut l_b = l_orig.clone();
        let mut r_b = r_orig.clone();
        chain_bypassed.process(&mut l_b, &mut r_b);

        let mut sum_bypassed = vec![0.0; l_b.len()];
        for i in 0..l_b.len() {
            sum_bypassed[i] = l_b[i] + r_b[i];
        }

        let mut e_c = 0.0;
        let mut e_b = 0.0;
        for i in 0..l.len() {
            e_c += sum_created[i] * sum_created[i];
            e_b += sum_bypassed[i] * sum_bypassed[i];
        }

        let diff = (10.0 * libm::log10f(e_c / e_b)).abs();
        assert!(diff < 0.01, "Energy diff was {} in Create mode", diff);
    }
}
