use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EngineerCondition {
    // Spectral problems
    MuddyMix,          // low-mid buildup 200-400Hz
    ThinMix,           // lacking low-mid body
    HarshTopEnd,       // excessive 4-8kHz energy
    LowFrequencyMuddy, // sub-bass buildup below 80Hz

    // Dynamic problems
    PumpDrift,       // compressor pumping artifacts
    TransientLoss,   // attack transients being crushed
    DynamicRangeLow, // over-compressed, no dynamics

    // Stereo/translation problems
    MonoCompatibilityRisk, // phase issues on mono playback
    StereoCorrelationWeak, // L/R phase cancellation risk
    TranslationRisk,       // won't translate to small speakers

    // Loudness problems
    LufsTooLoud,      // exceeds target LUFS
    LufsTooQuiet,     // below target LUFS
    TruePeakExceeded, // clipping risk

    // Technical problems
    DcOffsetDetected, // DC offset present
    MainsHumDetected, // 50/60Hz mains hum
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionSet {
    pub conditions: Vec<EngineerCondition>,
    pub sample_rate: u32,
    pub target_lufs: f32,
}

impl ConditionSet {
    pub fn has(&self, condition: &EngineerCondition) -> bool {
        self.conditions.contains(condition)
    }
}
