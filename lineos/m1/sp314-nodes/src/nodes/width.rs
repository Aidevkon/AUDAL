use crate::node::DspNode;

pub struct WidthNode {
    sample_rate: u32,
    decorrelation: f32,
    side_gain_db: f32,
    mono_comp_shelf_db: f32,

    mid_delay: Vec<f32>,
    mid_idx: usize,
    side_delay: Vec<f32>,
    side_idx: usize,

    shelf_low: f32,
}

impl WidthNode {
    pub fn new(sample_rate: u32) -> Self {
        let delay_m = (0.005 * sample_rate as f32) as usize;
        let delay_s = (0.007 * sample_rate as f32) as usize;
        Self {
            sample_rate,
            decorrelation: 0.0,
            side_gain_db: 0.0,
            mono_comp_shelf_db: 0.0,
            mid_delay: vec![0.0; delay_m.max(1)],
            mid_idx: 0,
            side_delay: vec![0.0; delay_s.max(1)],
            side_idx: 0,
            shelf_low: 0.0,
        }
    }

    pub fn set_params(&mut self, decorrelation: f32, side_gain_db: f32, mono_comp_shelf_db: f32) {
        self.decorrelation = decorrelation;
        self.side_gain_db = side_gain_db;
        self.mono_comp_shelf_db = mono_comp_shelf_db;
    }
}

const PARAMS: &[&str] = &["decorrelation", "side_gain_db", "mono_comp_shelf_db"];

impl DspNode for WidthNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let sq2 = std::f32::consts::SQRT_2;
        let isq2 = 1.0 / sq2;
        let sg = 10_f32.powf(self.side_gain_db / 20.0);
        let mcs = 10_f32.powf(self.mono_comp_shelf_db / 20.0);
        let dt = 1.0 / self.sample_rate as f32;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * 200.0);
        let alpha = dt / (rc + dt);
        let dec = self.decorrelation;

        for i in 0..left.len() {
            let l = left[i];
            let r = right[i];

            // 1. M/S Matrix
            let mid = (l + r) * isq2;
            let mut side = (l - r) * isq2;

            // 2. Side Gain
            side *= sg;

            // 3. Decorrelation
            let d_mid = self.mid_delay[self.mid_idx];
            let d_side = self.side_delay[self.side_idx];

            self.mid_delay[self.mid_idx] = mid;
            self.side_delay[self.side_idx] = side;
            self.mid_idx = (self.mid_idx + 1) % self.mid_delay.len();
            self.side_idx = (self.side_idx + 1) % self.side_delay.len();

            side += (d_side + d_mid) * dec;

            // 4. Mono Comp Shelf (1st order HP on Side below ~200Hz)
            self.shelf_low = side * alpha + self.shelf_low * (1.0 - alpha);
            if self.shelf_low.abs() < 1e-9 {
                self.shelf_low = 0.0;
            }
            let shelf_high = side - self.shelf_low;

            side = shelf_high + self.shelf_low * mcs;

            // 5. L/R Matrix
            left[i] = (mid + side) * isq2;
            right[i] = (mid - side) * isq2;
        }
    }

    fn reset(&mut self) {
        self.mid_delay.fill(0.0);
        self.side_delay.fill(0.0);
        self.shelf_low = 0.0;
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> bool {
        if !self.has_parameter(name) {
            return false;
        }
        match name {
            "decorrelation" => self.decorrelation = value,
            "side_gain_db" => self.side_gain_db = value,
            "mono_comp_shelf_db" => self.mono_comp_shelf_db = value,
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
        "Width"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass() {
        let mut n = WidthNode::new(48000);
        let mut l = vec![1.0, 0.5];
        let mut r = vec![0.5, 1.0];
        n.process_stereo(&mut l, &mut r);
        assert!((l[0] - 1.0).abs() < 1e-4);
        assert!((l[1] - 0.5).abs() < 1e-4);
        assert!((r[0] - 0.5).abs() < 1e-4);
        assert!((r[1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn side_gain() {
        let mut n = WidthNode::new(48000);
        n.set_params(0.0, 6.0, 0.0);
        let mut l = vec![1.0];
        let mut r = vec![-1.0];
        n.process_stereo(&mut l, &mut r);
        assert!(l[0] > 1.9 && l[0] < 2.1, "Side gain failed");
    }

    #[test]
    fn decorrelation_breaks_mono() {
        let mut n = WidthNode::new(48000);
        n.set_params(0.5, 0.0, 0.0);
        let mut l = vec![0.0f32; 512];
        let mut r = vec![0.0f32; 512];
        l[0] = 1.0;
        r[0] = 1.0;
        n.process_stereo(&mut l, &mut r);
        let mut has_diff = false;
        for i in 0..l.len() {
            if (l[i] - r[i]).abs() > 1e-4 {
                has_diff = true;
                break;
            }
        }
        assert!(has_diff, "Decorrelation did not break mono");
    }

    #[test]
    fn no_nan() {
        let mut n = WidthNode::new(48000);
        n.set_params(0.5, 2.0, -1.0);
        let mut l = vec![0.0f32; 512];
        let mut r = vec![0.0f32; 512];
        n.process_stereo(&mut l, &mut r);
        assert!(l.iter().all(|x| x.is_finite()));
    }
}
