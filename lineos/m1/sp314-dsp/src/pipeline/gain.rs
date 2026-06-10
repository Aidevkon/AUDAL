pub struct HeadroomManager {
    pad_linear: f32,
    makeup_linear: f32,
}

impl HeadroomManager {
    pub fn new(pad_db: f32, makeup_db: f32) -> Self {
        Self {
            pad_linear: libm::powf(10.0_f32, pad_db / 20.0_f32),
            makeup_linear: libm::powf(10.0_f32, makeup_db / 20.0_f32),
        }
    }

    #[inline]
    pub fn apply_input_pad(&self, left: &mut [f32], right: &mut [f32]) {
        for i in 0..left.len() {
            left[i] *= self.pad_linear;
            right[i] *= self.pad_linear;
        }
    }

    #[inline]
    pub fn apply_output_makeup(&self, left: &mut [f32], right: &mut [f32]) {
        for i in 0..left.len() {
            let l = left[i] * self.makeup_linear;
            let r = right[i] * self.makeup_linear;
            left[i] = libm::fmaxf(-1.0_f32, libm::fminf(1.0_f32, l));
            right[i] = libm::fmaxf(-1.0_f32, libm::fminf(1.0_f32, r));
        }
    }

    #[inline]
    pub fn apply_output_makeup_no_clip(&self, left: &mut [f32], right: &mut [f32]) {
        for i in 0..left.len() {
            left[i] *= self.makeup_linear;
            right[i] *= self.makeup_linear;
        }
    }
}
