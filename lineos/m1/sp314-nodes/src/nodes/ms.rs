use crate::node::DspNode;

pub struct MsMatrixNode;

const PARAMS: &[&str] = &[];

impl DspNode for MsMatrixNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let mid = (*l + *r) * 0.5;
            let side = (*l - *r) * 0.5;
            *l = mid;
            *r = side;
        }
    }

    fn set_parameter(&mut self, _name: &str, _value: f32) -> bool {
        false
    }
    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }
    fn reset(&mut self) {}
    fn node_type(&self) -> &'static str {
        "MS_Matrix"
    }
}

pub struct InverseMsMatrixNode;

impl DspNode for InverseMsMatrixNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, mid: &mut [f32], side: &mut [f32]) {
        for (m, s) in mid.iter_mut().zip(side.iter_mut()) {
            let l = *m + *s;
            let r = *m - *s;
            *m = l;
            *s = r;
        }
    }

    fn set_parameter(&mut self, _name: &str, _value: f32) -> bool {
        false
    }
    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }
    fn reset(&mut self) {}
    fn node_type(&self) -> &'static str {
        "Inverse_MS_Matrix"
    }
}
