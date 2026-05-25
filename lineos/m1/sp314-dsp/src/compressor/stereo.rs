use crate::compressor::core::{CompressorBand, CompressorBandConfig};

#[derive(Clone)]
pub struct CompressorV3Config {
    pub mid_config:  CompressorBandConfig,
    pub side_config: CompressorBandConfig,
}

pub struct CompressorV3 {
    mid_band:  CompressorBand,
    side_band: CompressorBand,
}

impl CompressorV3 {
    pub fn new(config: CompressorV3Config, sample_rate: u32) -> Self {
        Self {
            mid_band: CompressorBand::new(config.mid_config, sample_rate),
            side_band: CompressorBand::new(config.side_config, sample_rate),
        }
    }

    pub fn process_stereo(&mut self, left: &mut f32, right: &mut f32) {
        // M/S Encode — dereference required (*left, *right are &mut f32)
        let mut m = (*left  + *right) * 0.5_f32;
        let mut s = (*left  - *right) * 0.5_f32;

        // Process — fully independent, no linking
        m = self.mid_band.process(m);
        s = self.side_band.process(s);

        // M/S Decode
        *left  = m + s;
        *right = m - s;

        // Inline finalize (D5 — ±2.0 internal headroom)
        *left  = libm::fmaxf(-2.0_f32, libm::fminf(2.0_f32, *left));
        *right = libm::fmaxf(-2.0_f32, libm::fminf(2.0_f32, *right));
    }

    pub fn reset(&mut self) {
        self.mid_band.reset();
        self.side_band.reset();
    }
}
