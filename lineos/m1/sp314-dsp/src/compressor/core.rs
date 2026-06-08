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

use super::crossover::CrossoverLR4x3;

#[derive(Clone)]
pub struct MultibandCompressor3Config {
    pub f_low:       f32,
    pub f_high:      f32,
    pub low_config:  CompressorBandConfig,
    pub mid_config:  CompressorBandConfig,
    pub high_config: CompressorBandConfig,
}

pub struct MultibandCompressor3 {
    crossover:    CrossoverLR4x3,
    env_low:      EnvelopeFollower,
    env_mid:      EnvelopeFollower,
    env_high:     EnvelopeFollower,
    cfg_low:      CompressorBandConfig,
    cfg_mid:      CompressorBandConfig,
    cfg_high:     CompressorBandConfig,
    makeup_low:   f32,
    makeup_mid:   f32,
    makeup_high:  f32,
}

impl MultibandCompressor3 {
    pub fn new(config: MultibandCompressor3Config, sample_rate: u32) -> Self {
        let makeup = |db: f32| libm::powf(10.0_f32, db / 20.0_f32);
        Self {
            crossover:   CrossoverLR4x3::new(config.f_low, config.f_high, sample_rate),
            env_low:     EnvelopeFollower::new(
                             config.low_config.attack_ms,
                             config.low_config.release_ms,
                             sample_rate),
            env_mid:     EnvelopeFollower::new(
                             config.mid_config.attack_ms,
                             config.mid_config.release_ms,
                             sample_rate),
            env_high:    EnvelopeFollower::new(
                             config.high_config.attack_ms,
                             config.high_config.release_ms,
                             sample_rate),
            makeup_low:  makeup(config.low_config.makeup_db),
            makeup_mid:  makeup(config.mid_config.makeup_db),
            makeup_high: makeup(config.high_config.makeup_db),
            cfg_low:     config.low_config,
            cfg_mid:     config.mid_config,
            cfg_high:    config.high_config,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let (low, mid, high) = self.crossover.process(x);

        let env_low_db  = self.env_low.process(low);
        let env_mid_db  = self.env_mid.process(mid);
        let env_high_db = self.env_high.process(high);

        let gr_low  = libm::powf(10.0_f32,
            compute_gain_reduction(env_low_db,
                self.cfg_low.threshold_db,
                self.cfg_low.ratio,
                self.cfg_low.knee_db) / 20.0_f32);

        let gr_mid  = libm::powf(10.0_f32,
            compute_gain_reduction(env_mid_db,
                self.cfg_mid.threshold_db,
                self.cfg_mid.ratio,
                self.cfg_mid.knee_db) / 20.0_f32);

        let gr_high = libm::powf(10.0_f32,
            compute_gain_reduction(env_high_db,
                self.cfg_high.threshold_db,
                self.cfg_high.ratio,
                self.cfg_high.knee_db) / 20.0_f32);

        let out = low  * gr_low  * self.makeup_low
                + mid  * gr_mid  * self.makeup_mid
                + high * gr_high * self.makeup_high;

        // Internal headroom clamp — matches CompressorBand convention
        libm::fmaxf(-2.0_f32, libm::fminf(2.0_f32, out))
    }

    pub fn process_stereo(&mut self, left: &mut f32, right: &mut f32) {
        *left  = self.process(*left);
        *right = self.process(*right);
    }

    pub fn reset(&mut self) {
        self.crossover.reset();
        self.env_low.reset();
        self.env_mid.reset();
        self.env_high.reset();
    }
}
