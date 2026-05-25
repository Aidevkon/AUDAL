pub mod biquad;
pub mod cleaner;
pub mod gate;
pub use cleaner::RestorationChain;
pub use gate::NoiseGate;

/// Per-preset restoration settings.
/// Controls which stages are active.
#[derive(Clone, Debug, PartialEq)]
pub struct RestorationConfig {
    /// 80Hz high-pass (anti-plosive and rumble)
    pub lowcut_enabled:  bool,
    /// 50/100/150Hz notch cascade (mains hum removal)
    pub hum_enabled:     bool,
    /// HF sidechain de-esser (sibilance reduction)
    pub deess_enabled:   bool,
    /// Noise gate (silence background noise)
    pub gate_enabled:    bool,
}

impl RestorationConfig {
    /// For voice/podcast — all stages active
    pub fn voice() -> Self {
        Self { lowcut_enabled: true, hum_enabled: true, deess_enabled: true, gate_enabled: true }
    }

    /// For music mastering — no gate, no hum (assume clean source)
    pub fn music() -> Self {
        Self { lowcut_enabled: true, hum_enabled: false, deess_enabled: true, gate_enabled: false }
    }

    /// Fully bypassed — pass-through
    pub fn bypass() -> Self {
        Self { lowcut_enabled: false, hum_enabled: false, deess_enabled: false, gate_enabled: false }
    }
}

