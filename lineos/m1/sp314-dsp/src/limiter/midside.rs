// src/limiter/midside.rs

// RBJ Butterworth HP 100Hz @ 48kHz — verified vs Phase 3 fixture
const B0: f64 =  0.9907866979;
const B1: f64 = -1.9815733959;
const B2: f64 =  0.9907866979;
const A1: f64 = -1.9814885091;
const A2: f64 =  0.9816582826;

pub struct MidSideProcessor {
    // Direct Form I — f64 for low-frequency stability
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl MidSideProcessor {
    pub fn new() -> Self {
        Self { x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        let l = left  as f64;
        let r = right as f64;

        // 1. Encode M/S
        let mid  = (l + r) * 0.5;
        let side = (l - r) * 0.5;

        // 2. HP filter on Side channel (Direct Form I, f64)
        let side_filtered = B0 * side
                          + B1 * self.x1
                          + B2 * self.x2
                          - A1 * self.y1
                          - A2 * self.y2;

        // Update state
        self.x2 = self.x1;
        self.x1 = side;
        self.y2 = self.y1;
        self.y1 = side_filtered;

        // Denormal flush — prevents CPU penalty on near-zero states
        // CRITICAL no_std RULE: use libm::fabs, NOT f64::abs()
        if libm::fabs(self.y1) < 1e-30 { self.y1 = 0.0; }
        if libm::fabs(self.y2) < 1e-30 { self.y2 = 0.0; }

        // 3. Decode L/R
        let out_l = (mid + side_filtered) as f32;
        let out_r = (mid - side_filtered) as f32;
        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0; self.x2 = 0.0;
        self.y1 = 0.0; self.y2 = 0.0;
    }
}
