use crate::masking_eq::{MaskingAwareEQ, MaskingEQConfig};
use crate::compressor::stereo::{CompressorV3, CompressorV3Config};
use crate::pipeline::phase::PhaseAligner;
use crate::pipeline::gain::HeadroomManager;
use crate::pipeline::telemetry::{Telemetry, analyze_offline_pre_pass};
use crate::limiter::{BrickwallLimiter, LimiterConfig, LOOKAHEAD_SAMPLES};
use crate::harmonic::{HarmonicEngine, HarmonicConfig};
use crate::restoration::{RestorationChain, RestorationConfig};

/// Configuration for the entire mastering engine.
/// Includes nested configurations for all DSP stages.
#[derive(Clone)]
pub struct EngineConfig {
    pub eq_config:        MaskingEQConfig,
    pub comp_config:      CompressorV3Config,
    pub parallel_mix:     f32,
    pub target_makeup_db: f32,
    pub limiter_config:   LimiterConfig,
    pub restoration_config: RestorationConfig,
    pub harmonic_config:  Option<HarmonicConfig>,
}

/// The core deterministic mastering engine.
/// Manages the full signal chain: Restoration -> EQ -> Harmonics -> Compressor -> Limiter.
pub struct Sp314MasteringEngine {
    pub eq:      MaskingAwareEQ,
    pub comp:    CompressorV3,
    pub aligner: PhaseAligner,
    pub harmonic: HarmonicEngine,
    pub limiter: BrickwallLimiter,
    pub restoration: RestorationChain,
    pub config:  EngineConfig,
}

pub fn calculate_adaptive_budget(
    telemetry:      &Telemetry,
    user_makeup_db: f32,
) -> (f32, f32) {
    let pad_db = if telemetry.peak_db > -3.0 {
        -12.0
    } else if telemetry.peak_db > -9.0 {
        -6.0
    } else {
        0.0
    };

    let makeup_db = -pad_db + user_makeup_db;
    (pad_db, makeup_db)
}

impl Sp314MasteringEngine {
    pub fn new(config: EngineConfig, sample_rate: u32) -> Result<Self, &'static str> {
        let crossover_hz = config.comp_config.mid_config.crossover_hz;
        Ok(Self {
            eq:      MaskingAwareEQ::new(config.eq_config.clone(), sample_rate)
                         .map_err(|_| "EQ config error")?,
            comp:    CompressorV3::new(config.comp_config.clone(), sample_rate),
            aligner: PhaseAligner::new(crossover_hz, sample_rate),
            harmonic: HarmonicEngine::new(config.harmonic_config.unwrap_or_else(|| {
                crate::harmonic::HarmonicConfig { mix: 0.0, ..Default::default() }
            })),
            limiter: BrickwallLimiter::new(config.limiter_config, sample_rate),
            restoration: RestorationChain::new(sample_rate as f32, config.restoration_config.clone()),
            config,
        })
    }

    /// Process the entire audio file offline.
    /// Performs a pre-pass to analyze the signal and calculate adaptive gain staging,
    /// then processes the audio and returns telemetry data.
    pub fn process_offline(
        &mut self,
        left:  &mut std::vec::Vec<f32>,
        right: &mut std::vec::Vec<f32>,
    ) -> Telemetry {
        debug_assert_eq!(left.len(), right.len());

        let telemetry = analyze_offline_pre_pass(left, right);
        let (pad_db, makeup_db) =
            calculate_adaptive_budget(&telemetry, self.config.target_makeup_db);
        let headroom = HeadroomManager::new(pad_db, makeup_db);



        headroom.apply_input_pad(left, right);

        self.restoration.process(left, right);

        self.eq.process_block(left, right);

        self.process_stereo_block_internal(left, right);

        headroom.apply_output_makeup_no_clip(left, right);

        self.limiter.process_block(left, right);

        // Latency compensation — flush the delay line
        let mut flush_l = [0.0_f32; LOOKAHEAD_SAMPLES];
        let mut flush_r = [0.0_f32; LOOKAHEAD_SAMPLES];
        self.limiter.process_block(&mut flush_l, &mut flush_r);

        left.extend_from_slice(&flush_l);
        right.extend_from_slice(&flush_r);

        left.drain(..LOOKAHEAD_SAMPLES);
        right.drain(..LOOKAHEAD_SAMPLES);

        telemetry
    }

    #[inline]
    fn process_stereo_block_internal(&mut self, left: &mut [f32], right: &mut [f32]) {
        let mix = self.config.parallel_mix;
        for i in 0..left.len() {
            let mut wet_l = left[i];
            let mut wet_r = right[i];
            let mut dry_l = left[i];
            let mut dry_r = right[i];

            self.aligner.process_stereo(&mut dry_l, &mut dry_r);

            let mid = (wet_l + wet_r) * 0.5;
            let side = (wet_l - wet_r) * 0.5;
            let (mid_out, side_out) = self.harmonic.process_frame(mid, side);
            wet_l = mid_out + side_out;
            wet_r = mid_out - side_out;

            self.comp.process_stereo(&mut wet_l, &mut wet_r);

            left[i]  = dry_l * (1.0_f32 - mix) + wet_l * mix;
            right[i] = dry_r * (1.0_f32 - mix) + wet_r * mix;
        }
    }

    /// Process a fixed block of stereo frames in real-time.
    /// Block size must be consistent across calls.
    /// Called from the engine thread — not the audio callback.
    /// No allocation inside this method.
    pub fn process_block(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        debug_assert_eq!(left.len(), right.len());
        
        // In real-time, we don't have offline pre-pass telemetry. 
        // We assume pad_db = 0.0 and just apply the target_makeup_db from the config.
        let headroom = HeadroomManager::new(0.0, self.config.target_makeup_db);
        
        headroom.apply_input_pad(left, right);
        
        self.restoration.process(left, right);

        self.eq.process_block(left, right);
        
        self.process_stereo_block_internal(left, right);
        
        headroom.apply_output_makeup_no_clip(left, right);
        
        self.limiter.process_block(left, right);
    }

    /// Reset the internal state of all DSP modules (delays, envelope followers, filters).
    /// Required before processing a new track or when seeking.
    pub fn reset(&mut self) {
        self.eq.reset();
        self.harmonic.reset();
        self.comp.reset();
        self.aligner.reset();
        self.limiter.reset();
        self.restoration.reset();
    }
}
