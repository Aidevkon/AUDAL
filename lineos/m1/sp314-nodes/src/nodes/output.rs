use crate::node::DspNode;

pub struct OutputNode;

const PARAMS: &[&str] = &[];

impl DspNode for OutputNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, _left: &mut [f32], _right: &mut [f32]) {}
    fn set_parameter(&mut self, _name: &str, _value: f32) -> bool {
        false
    }
    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }
    fn reset(&mut self) {}
    fn node_type(&self) -> &'static str {
        "Output"
    }
}
