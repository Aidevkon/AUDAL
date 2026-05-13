#[derive(Clone, PartialEq)]
pub struct EQState {
    pub low_db: f32,       // −12.0..+12.0
    pub mid_db: f32,
    pub presence_db: f32,
    pub air_db: f32,
    pub curve_points: Vec<(f32, f32)>,  // normalized 0..1 for SVG path
}

#[derive(Clone, PartialEq)]
pub struct CompressorState {
    pub threshold_db: f32,  // −40..0
    pub ratio: f32,          // 1.0..20.0
    pub gain_reduction_db: f32,
    pub makeup_db: f32,
    pub curve_points: Vec<(f32, f32)>,
}

#[derive(Clone, PartialEq)]
pub struct LimiterState {
    pub ceiling_dbtp: f32,   // −3.0..0.0
    pub release_auto: bool,
    pub isp_factor: u8,      // 4 or 8
    pub curve_points: Vec<(f32, f32)>,
}

#[derive(Clone, PartialEq)]
pub struct SatTelemetry {
    pub thd_percent: f32,         // 0.0..12.0 — total harmonic distortion %
    pub knee_curve: [f32; 64],    // normalized 0..1 soft-clip transfer function
    pub headroom_db: f32,         // remaining headroom before clip
    pub harmonic_density: f32,    // 0.0..1.0 — harmonic energy ratio
}
