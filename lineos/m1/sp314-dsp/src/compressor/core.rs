use super::crossover::CrossoverLR4;
use super::envelope::EnvelopeFollower;
use super::gain::compute_gain_reduction;

#[derive(Clone)]
pub struct CompressorBandConfig {
    pub threshold_db:  f32,
    pub ratio:         f32,
    pub knee_db:       f32,
    pub attack_ms:     f32,
    pub release_ms:    f32,
    pub makeup_db:     f32,
    pub crossover_hz:  f32,
}

pub struct CompressorBand {
    crossover:    CrossoverLR4,
    env_low:      EnvelopeFollower,
    env_high:     EnvelopeFollower,
    config:       CompressorBandConfig,
    makeup_linear: f32,
    sample_rate:  u32,
}

impl CompressorBand {
    pub fn new(config: CompressorBandConfig, sample_rate: u32) -> Self {
        let crossover = CrossoverLR4::new(config.crossover_hz, sample_rate);
        let env_low = EnvelopeFollower::new(config.attack_ms, config.release_ms, sample_rate);
        let env_high = EnvelopeFollower::new(config.attack_ms, config.release_ms, sample_rate);
        let makeup_linear = libm::powf(10.0_f32, config.makeup_db / 20.0_f32);
        
        Self {
            crossover,
            env_low,
            env_high,
            config,
            makeup_linear,
            sample_rate,
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let (low, high) = self.crossover.process(x);

        let env_low_db  = self.env_low.process(low);
        let env_high_db = self.env_high.process(high);

        let gr_low_db  = compute_gain_reduction(env_low_db,
            self.config.threshold_db, self.config.ratio, self.config.knee_db);
        let gr_high_db = compute_gain_reduction(env_high_db,
            self.config.threshold_db, self.config.ratio, self.config.knee_db);

        let gr_low_linear  = libm::powf(10.0_f32, gr_low_db  / 20.0_f32);
        let gr_high_linear = libm::powf(10.0_f32, gr_high_db / 20.0_f32);

        let out_low  = low  * gr_low_linear;
        let out_high = high * gr_high_linear;

        // LR4 Crossover Sum Rule:
        // low + high = input (flat response, all frequencies)
        // DO NOT negate high band — LR4 is in-phase (360° = 0° net)
        // LR2 required inversion; LR4 does NOT
        let out = (out_low + out_high) * self.makeup_linear;

        libm::fmaxf(-2.0_f32, libm::fminf(2.0_f32, out))
    }

    pub fn reset(&mut self) {
        self.crossover.reset();
        self.env_low.reset();
        self.env_high.reset();
    }
}
