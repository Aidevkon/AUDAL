use crate::compressor::crossover::CrossoverLR4;

pub struct PhaseAligner {
    crossover_l: CrossoverLR4,
    crossover_r: CrossoverLR4,
}

impl PhaseAligner {
    pub fn new(crossover_hz: f32, sample_rate: u32) -> Self {
        Self {
            crossover_l: CrossoverLR4::new(crossover_hz, sample_rate),
            crossover_r: CrossoverLR4::new(crossover_hz, sample_rate),
        }
    }

    #[inline]
    pub fn process_stereo(&mut self, left: &mut f32, right: &mut f32) {
        let (low_l, high_l) = self.crossover_l.process(*left);
        let (low_r, high_r) = self.crossover_r.process(*right);
        // LR4 sum rule: low + high (NO sign inversion)
        *left = low_l + high_l;
        *right = low_r + high_r;
    }

    pub fn reset(&mut self) {
        self.crossover_l.reset();
        self.crossover_r.reset();
    }
}
