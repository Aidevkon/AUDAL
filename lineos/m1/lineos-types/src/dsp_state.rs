#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DspState {
    pub ducking_depth: f32,
    pub sidechain_hold: usize,
    pub ms_width: f32,
    pub lfe_gain: f32,
}

impl Default for DspState {
    fn default() -> Self {
        Self {
            ducking_depth: 1.0,
            sidechain_hold: 3,
            ms_width: 1.0,
            lfe_gain: 0.0,
        }
    }
}
