// integration/src/proof_log.rs — ProofLog stub
// Authority: spec/locked/S-009_integration_firewall.md v1.0
// Full implementation in S-010 (Execution Proof).
// S-009 only logs; S-010 consumes the log.

use crate::config::{DspConfig, DspDynamicsConfig, DspEqConfig, DspSatConfig, DspStereoConfig};
use crate::error::FirewallError;
use aether::intent::types::Intent;
use aether::semantic::ZoneAdjustments;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClampEvent {
    pub field: String,
    pub original: f32,
    pub clamped: f32,
}

/// Minimal ProofLog — collects decisions during a render.
/// Full implementation (hashing, certificate) in S-010.
#[derive(Debug, Default)]
pub struct ProofLog {
    pub clamp_events: Vec<ClampEvent>,
    pub final_config: Option<DspConfig>,
    pub intent: Option<Intent>,
    pub zone_adj: Option<ZoneAdjustments>,
}

impl ProofLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_clamp(&mut self, e: FirewallError) {
        if let FirewallError::FirewallClamp {
            field,
            original,
            clamped,
        } = e
        {
            self.clamp_events.push(ClampEvent {
                field,
                original,
                clamped,
            });
        }
    }

    pub fn record_dsp_config(
        &mut self,
        eq: &DspEqConfig,
        dynamics: &DspDynamicsConfig,
        sat: &DspSatConfig,
        stereo: &DspStereoConfig,
    ) {
        // Store for S-010 certificate generation
        // Full DspConfig reconstructed from parts
        let _ = (eq, dynamics, sat, stereo); // used in S-010
    }

    pub fn record_intent(&mut self, intent: &Intent) {
        self.intent = Some(intent.clone());
    }

    pub fn record_zone_adj(&mut self, zones: &ZoneAdjustments) {
        self.zone_adj = Some(zones.clone());
    }

    pub fn clamp_count(&self) -> usize {
        self.clamp_events.len()
    }
}
