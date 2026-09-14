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
    pub lowcut_enabled: bool,
    /// 50/100/150Hz notch cascade (mains hum removal)
    pub hum_enabled: bool,
    /// HF sidechain de-esser (sibilance reduction)
    pub deess_enabled: bool,
    /// Noise gate (silence background noise)
    pub gate_enabled: bool,
}

impl RestorationConfig {
    /// For voice/podcast — lowcut, de-ess and gate active; hum OFF (see below)
    pub fn voice() -> Self {
        Self {
            lowcut_enabled: true,
            // ΚΛΕΙΣΤΟ, ΟΧΙ ΑΦΑΙΡΕΜΕΝΟ. Το στατικό cascade
            // κόβει 50/100/150 με Q=20 (πλάτος 2.5 Hz).
            // ΜΕΤΡΗΘΗΚΕ 2026-09-14 σε εννέα αρχεία αφήγησης:
            // η κορυφή βόμβου είναι στα 59.8-60.8 Hz σε έξι,
            // σε κανένα στα 50 — οκτώ πλάτη μακριά.
            // ΚΑΙ ΤΟ F-082 (24/08) μέτρησε τι αφαιρούσε όταν
            // δεν υπήρχε hum στα 50: ενέργεια 70-250 Hz με
            // κέντρο βάρους 126-136 Hz, δηλαδή ανδρική
            // θεμελιώδη.
            // ⇒ ΔΕΝ ΠΙΑΝΕΙ ΑΥΤΟ ΠΟΥ ΥΠΑΡΧΕΙ ΚΑΙ ΠΙΑΝΕΙ ΑΥΤΟ
            //   ΠΟΥ ΔΕΝ ΕΠΡΕΠΕ.
            // ΑΝΤΙΚΑΘΙΣΤΑΤΑΙ ΟΤΑΝ ΥΠΑΡΞΕΙ ΑΝΙΧΝΕΥΤΗΣ ΠΟΥ
            // ΔΙΑΛΕΓΕΙ ΣΥΧΝΟΤΗΤΑ (R5b #4). Ο DeHumNode
            // (sp314-nodes) δέχεται ΗΔΗ fundamental_hz ως
            // παράμετρο — κανείς δεν του τη δίνει.
            hum_enabled: false,
            deess_enabled: true,
            gate_enabled: true,
        }
    }

    /// For music mastering — no gate, no hum (assume clean source)
    pub fn music() -> Self {
        Self {
            lowcut_enabled: true,
            hum_enabled: false,
            deess_enabled: true,
            gate_enabled: false,
        }
    }

    /// Fully bypassed — pass-through
    pub fn bypass() -> Self {
        Self {
            lowcut_enabled: false,
            hum_enabled: false,
            deess_enabled: false,
            gate_enabled: false,
        }
    }
}
