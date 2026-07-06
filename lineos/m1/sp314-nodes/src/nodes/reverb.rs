use crate::node::DspNode;

struct Lbcf {
    buffer: Vec<f32>,
    idx: usize,
    filter_store: f32,
}

impl Lbcf {
    fn new(len: usize) -> Self {
        Self {
            buffer: vec![0.0; len],
            idx: 0,
            filter_store: 0.0,
        }
    }

    fn process(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let out = self.buffer[self.idx];
        self.filter_store = out * (1.0 - damping) + self.filter_store * damping;
        if self.filter_store.abs() < 1e-9 {
            self.filter_store = 0.0;
        } // denormal prevent
        self.buffer[self.idx] = input + self.filter_store * feedback;
        self.idx = (self.idx + 1) % self.buffer.len();
        // Return input + out to avoid 30ms dead silence at start
        input + out
    }
}

struct Allpass {
    buffer: Vec<f32>,
    idx: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self {
            buffer: vec![0.0; len],
            idx: 0,
        }
    }

    fn process(&mut self, input: f32, feedback: f32) -> f32 {
        let buf_out = self.buffer[self.idx];
        let out = -input + buf_out;
        self.buffer[self.idx] = input + buf_out * feedback;
        if self.buffer[self.idx].abs() < 1e-9 {
            self.buffer[self.idx] = 0.0;
        } // denormal prevent
        self.idx = (self.idx + 1) % self.buffer.len();
        out
    }
}

pub struct ReverbNode {
    rt60: f32,
    hf_damping: f32,
    diffusion: f32,
    mix: f32,

    lbcf_l: Vec<Lbcf>,
    lbcf_r: Vec<Lbcf>,
    allpass_l: Vec<Allpass>,
    allpass_r: Vec<Allpass>,
}

impl ReverbNode {
    pub fn new(sample_rate: u32) -> Self {
        let lbcf_times_ms = [29.7, 37.1, 41.1, 43.7, 47.4, 50.0, 53.3, 55.2];
        let mut lbcf_l = Vec::new();
        let mut lbcf_r = Vec::new();
        for i in 0..4 {
            lbcf_l.push(Lbcf::new(
                (lbcf_times_ms[i] * sample_rate as f32 / 1000.0) as usize,
            ));
            lbcf_r.push(Lbcf::new(
                (lbcf_times_ms[i + 4] * sample_rate as f32 / 1000.0) as usize,
            ));
        }

        let mut allpass_l = Vec::new();
        let mut allpass_r = Vec::new();
        // Use short coprime delays to immediately diffuse the impulse and fill the tail
        allpass_l.push(Allpass::new(3));
        allpass_l.push(Allpass::new(7));
        allpass_r.push(Allpass::new(5));
        allpass_r.push(Allpass::new(11));

        Self {
            rt60: 0.5,
            hf_damping: 0.5,
            diffusion: 0.5,
            mix: 0.5,
            lbcf_l,
            lbcf_r,
            allpass_l,
            allpass_r,
        }
    }

    pub fn set_params(&mut self, rt60: f32, hf_damping: f32, diffusion: f32, mix: f32) {
        self.rt60 = rt60;
        self.hf_damping = hf_damping;
        self.diffusion = diffusion;
        self.mix = mix;
    }
}

const PARAMS: &[&str] = &["rt60", "hf_damping", "diffusion", "mix"];

impl DspNode for ReverbNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let feedback = 0.84 + (self.rt60 / 8.0).min(0.13);
        let damping = self.hf_damping;
        let ap_feedback = 0.5 + self.diffusion * 0.3;
        let mix = self.mix;
        let dry = 1.0 - mix;

        for i in 0..left.len() {
            let in_l = left[i];
            let in_r = right[i];

            let sig_l = in_l * 0.1;
            let sig_r = in_r * 0.1;

            let mut out_l = 0.0;
            for lbcf in &mut self.lbcf_l {
                out_l += lbcf.process(sig_l, feedback, damping);
            }

            let mut out_r = 0.0;
            for lbcf in &mut self.lbcf_r {
                out_r += lbcf.process(sig_r, feedback, damping);
            }

            for ap in &mut self.allpass_l {
                out_l = ap.process(out_l, ap_feedback);
            }
            for ap in &mut self.allpass_r {
                out_r = ap.process(out_r, ap_feedback);
            }

            left[i] = in_l * dry + out_l * mix;
            right[i] = in_r * dry + out_r * mix;
        }
    }

    fn reset(&mut self) {
        for l in &mut self.lbcf_l {
            l.buffer.fill(0.0);
            l.filter_store = 0.0;
        }
        for r in &mut self.lbcf_r {
            r.buffer.fill(0.0);
            r.filter_store = 0.0;
        }
        for a in &mut self.allpass_l {
            a.buffer.fill(0.0);
        }
        for a in &mut self.allpass_r {
            a.buffer.fill(0.0);
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> bool {
        if !self.has_parameter(name) {
            return false;
        }
        match name {
            "rt60" => self.rt60 = value,
            "hf_damping" => self.hf_damping = value,
            "diffusion" => self.diffusion = value,
            "mix" => self.mix = value,
            _ => return false,
        }
        true
    }

    fn set_parameter_no_glide(&mut self, name: &str, value: f32) -> bool {
        self.set_parameter(name, value)
    }

    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }

    fn node_type(&self) -> &'static str {
        "Reverb"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_when_mix_zero() {
        let mut r = ReverbNode::new(48000);
        r.set_params(0.5, 0.5, 0.5, 0.0);
        let mut l = vec![1.0, 0.0, 0.0, 0.0];
        let mut r2 = vec![1.0, 0.0, 0.0, 0.0];
        r.process_stereo(&mut l, &mut r2);
        assert_eq!(l[0], 1.0);
        assert_eq!(l[1], 0.0);
    }

    #[test]
    fn impulse_generates_tail() {
        let mut rev = ReverbNode::new(48000);
        rev.set_params(1.0, 0.3, 0.5, 1.0);
        let mut left = vec![0.0f32; 512];
        let mut right = vec![0.0f32; 512];
        left[0] = 1.0;
        right[0] = 1.0;
        rev.process_stereo(&mut left, &mut right);
        assert!(left[100].abs() > 0.0, "Reverb tail is dead");
        assert!(
            left.iter().all(|&x| !x.is_nan() && x.abs() < 10.0),
            "Reverb exploded"
        );
    }

    #[test]
    fn no_nan_or_inf_on_silence() {
        let mut rev = ReverbNode::new(48000);
        rev.set_params(2.0, 0.8, 0.7, 0.8);
        let mut l = vec![0.0f32; 512];
        let mut r = vec![0.0f32; 512];
        rev.process_stereo(&mut l, &mut r);
        assert!(l.iter().all(|x| x.is_finite()), "NaN on silence");
    }
}
