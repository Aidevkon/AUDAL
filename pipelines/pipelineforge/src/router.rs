use crate::conditions::{ConditionSet, EngineerCondition};
use crate::flavor::Flavor;

pub struct Router;

impl Router {
    pub fn select(conditions: &ConditionSet) -> Vec<Flavor> {
        let mut flavors = Vec::new();

        // Stage 1: Technical fixes (always first)
        if conditions.has(&EngineerCondition::DcOffsetDetected) {
            flavors.push(Flavor::DcRemoval);
        }
        if conditions.has(&EngineerCondition::MainsHumDetected) {
            flavors.push(Flavor::HumRemoval);
        }

        // Stage 2: Spectral shaping (EQ before compression)
        if conditions.has(&EngineerCondition::MuddyMix) {
            flavors.push(Flavor::LowMidClarity);
        }
        if conditions.has(&EngineerCondition::ThinMix)
            || conditions.has(&EngineerCondition::HarshTopEnd) {
            flavors.push(Flavor::PresenceAndAir);
        }

        // Stage 3: Dynamics (after EQ)
        if conditions.has(&EngineerCondition::PumpDrift)
            || conditions.has(&EngineerCondition::TransientLoss) {
            flavors.push(Flavor::AntiPumpStabilization);
        }

        // Stage 4: Stereo (after dynamics)
        if conditions.has(&EngineerCondition::MonoCompatibilityRisk)
            || conditions.has(&EngineerCondition::StereoCorrelationWeak) {
            flavors.push(Flavor::MonoSafeMaster);
        }

        // Stage 5: Loudness (always last)
        flavors.push(Flavor::LufsNormalization);

        flavors
    }
}
