//! ReferenceResolver — deterministic, non-ML
//! spectral character targeting.
//!
//! Loads the podcast-v1 Reference Profile and
//! computes per-band EQ gain offsets that move
//! an input spectral shape toward the LTASS
//! target. Stateless — all functions are pure.
//!
//! Authority:
//!   lineos/docs/reference-driven-sonic-vision-
//!   podcast-v1_1.md §3.2–§3.4
//! Sources:
//!   Byrne et al. 1994 JASA DOI:10.1121/1.410152
//!   WO2007120453 A1 (band-energy SBR method)

use serde::Deserialize;

// ── Profile JSON (compile-time embed) ─────────
const PODCAST_V1_JSON: &str = include_str!(
    "../../../lineos/shared/schema/reference-profiles/\
     podcast-v1.json"
);

// ── Proprietary craft constants ────────────────
// "In dark, not hidden": the structure is public;
// these values are the proprietary craft layer.
// G_MAX_DB: placeholder — refined during tuning.
const G_MAX_DB: f32 = 6.0;

// SBR band indices (must match podcast-v1.json
// and the 8-band PreAnalysisData layout).
const SBR_LO: usize = 3; // Mid-Low  500-1kHz
const SBR_HI: usize = 4; // Mid-High 1-2kHz

// ── Serde structs (JSON shape) ─────────────────
// Fields prefixed with '_' are metadata/comments
// in the JSON — we skip them with
// `#[serde(default)]` + deny_unknown_fields OFF.

#[derive(Deserialize)]
struct BandJson {
    index: usize,
    target_db_relative: f32,
}

#[derive(Deserialize)]
struct SpectralTargetJson {
    bands: Vec<BandJson>,
}

#[derive(Deserialize)]
struct HardConstraintsJson {
    target_lufs: f32,
    true_peak_ceiling_dbtp: f32,
    gate_absolute_lufs: f32,
    gate_relative_lu: f32,
    lra_target_lu: f32,
}

#[derive(Deserialize)]
struct SbrJson {
    lower_band_index: usize,
    upper_band_index: usize,
}

#[derive(Deserialize)]
struct ProfileJson {
    id: String,
    hard_constraints: HardConstraintsJson,
    spectral_target: SpectralTargetJson,
    sbr: SbrJson,
}

// ── Runtime profile (parsed once) ─────────────
/// Parsed, ready-to-use Reference Profile.
/// Loaded from the compile-time embedded JSON.
pub struct ReferenceProfile {
    pub id: String,
    /// Normalised spectral shape target (8 bands,
    /// mean=0, relative dB). NOT absolute dBFS —
    /// see spec Correction 1.
    pub spectral_target: [f32; 8],
    /// Hard loudness/peak constraints.
    pub target_lufs: f32,
    pub true_peak_ceiling_dbtp: f32,
    pub gate_absolute_lufs: f32,
    pub gate_relative_lu: f32,
    pub lra_target_lu: f32,
    /// SBR band indices.
    pub sbr_lo: usize,
    pub sbr_hi: usize,
}

impl ReferenceProfile {
    /// Load and parse the embedded JSON.
    /// Panics at startup if the JSON is malformed
    /// (compile-time embed guarantees it is not).
    pub fn load_podcast_v1() -> Self {
        let parsed: ProfileJson = serde_json::from_str(PODCAST_V1_JSON).expect(
            "podcast-v1.json is \
                         malformed — check embed",
        );

        let mut target = [0.0_f32; 8];
        for band in &parsed.spectral_target.bands {
            assert!(
                band.index < 8,
                "podcast-v1.json: band index {} \
                 out of range",
                band.index
            );
            target[band.index] = band.target_db_relative;
        }

        Self {
            id: parsed.id,
            spectral_target: target,
            target_lufs: parsed.hard_constraints.target_lufs,
            true_peak_ceiling_dbtp: parsed.hard_constraints.true_peak_ceiling_dbtp,
            gate_absolute_lufs: parsed.hard_constraints.gate_absolute_lufs,
            gate_relative_lu: parsed.hard_constraints.gate_relative_lu,
            lra_target_lu: parsed.hard_constraints.lra_target_lu,
            sbr_lo: parsed.sbr.lower_band_index,
            sbr_hi: parsed.sbr.upper_band_index,
        }
    }
}

// ── ReferenceResolver ─────────────────────────
/// Stateless resolver — all methods are pure
/// functions. No heap allocation in hot path.
/// Matches SemanticZoneResolver's stateless
/// pattern (see aether/semantic/resolver.rs).
pub struct ReferenceResolver;

impl ReferenceResolver {
    /// Compute per-band gain offsets:
    ///   G_k = clamp(target[k] - signal[k],
    ///               -G_MAX_DB, +G_MAX_DB)
    ///
    /// Spec §3.4 step 3. Positive → boost,
    /// negative → cut.
    pub fn compute_gains(signal: &[f32; 8], target: &[f32; 8]) -> [f32; 8] {
        let mut gains = [0.0_f32; 8];
        for k in 0..8 {
            let raw = target[k] - signal[k];
            gains[k] = libm::fminf(G_MAX_DB, libm::fmaxf(-G_MAX_DB, raw));
        }
        gains
    }

    /// Compute Spectral Balance Ratio delta:
    ///   R_dB(X) = 10·log10(E_upper / E_lower)
    ///   ΔR_dB   = R(signal) - R(target)
    ///
    /// Where E = 10^(band_db/10) (linear energy).
    /// Spec §3.2, WO2007120453 A1.
    ///
    /// ΔR_dB > 0 → brighter than reference.
    /// ΔR_dB < 0 → darker (more mud than clarity).
    /// ΔR_dB ≈ 0 → tilt matches reference.
    pub fn compute_sbr_delta(signal: &[f32; 8], target: &[f32; 8]) -> f32 {
        let r_signal = Self::sbr_db(signal[SBR_LO], signal[SBR_HI]);
        let r_target = Self::sbr_db(target[SBR_LO], target[SBR_HI]);
        r_signal - r_target
    }

    /// R_dB = 10·log10(10^(hi/10) / 10^(lo/10))
    ///      = hi - lo  (dB subtraction = ratio)
    /// Simplified form: avoids two pow10 calls.
    /// Mathematically identical to the oracle.
    #[inline(always)]
    fn sbr_db(lo_db: f32, hi_db: f32) -> f32 {
        hi_db - lo_db
    }

    /// Full resolve: given the input spectral
    /// profile (8-band, relative dBFS shape),
    /// return per-band EQ gain offsets.
    ///
    /// This is the entry point for §10.6
    /// (integration into DspConfig build path).
    pub fn resolve(signal_profile: &[f32; 8], profile: &ReferenceProfile) -> [f32; 8] {
        Self::compute_gains(signal_profile, &profile.spectral_target)
    }
}

// ── Tests ──────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> ReferenceProfile {
        ReferenceProfile::load_podcast_v1()
    }

    #[test]
    fn profile_loads_8_bands() {
        let p = load();
        assert_eq!(p.id, "podcast-v1");
        assert_eq!(p.sbr_lo, SBR_LO);
        assert_eq!(p.sbr_hi, SBR_HI);
        // All 8 bands populated (none left at 0.0
        // except where the target genuinely is 0 —
        // none in podcast-v1).
        let zeros = p.spectral_target.iter().filter(|&&v| v == 0.0).count();
        assert_eq!(zeros, 0, "unexpected zero in spectral_target");
    }

    #[test]
    fn hard_constraints_match_spec() {
        let p = load();
        // Apple Podcasts art.893 / ITU-R BS.1770-5
        assert!((p.target_lufs - (-16.0)).abs() < 1e-5);
        assert!((p.true_peak_ceiling_dbtp - (-1.0)).abs() < 1e-5);
        assert!((p.gate_absolute_lufs - (-70.0)).abs() < 1e-5);
    }

    #[test]
    fn balanced_voice_zero_gains() {
        let p = load();
        let gains = ReferenceResolver::compute_gains(&p.spectral_target, &p.spectral_target);
        for (k, &g) in gains.iter().enumerate() {
            assert!(g.abs() < 1e-5, "band {k}: expected 0 gain, got {g}");
        }
    }

    #[test]
    fn balanced_voice_zero_sbr_delta() {
        let p = load();
        let delta = ReferenceResolver::compute_sbr_delta(&p.spectral_target, &p.spectral_target);
        assert!(
            delta.abs() < 1e-5,
            "ΔR_dB should be 0 for balanced input, \
             got {delta}"
        );
    }

    #[test]
    fn muddy_voice_sbr_negative() {
        let p = load();
        let mut signal = p.spectral_target;
        signal[SBR_LO] += 4.0; // too much mud
        signal[SBR_HI] -= 3.0; // too little clarity
        let delta = ReferenceResolver::compute_sbr_delta(&signal, &p.spectral_target);
        assert!(
            delta < 0.0,
            "muddy voice should have ΔR_dB < 0, \
             got {delta}"
        );
    }

    #[test]
    fn harsh_voice_sbr_positive() {
        let p = load();
        let mut signal = p.spectral_target;
        signal[SBR_LO] -= 3.0;
        signal[SBR_HI] += 4.0;
        let delta = ReferenceResolver::compute_sbr_delta(&signal, &p.spectral_target);
        assert!(
            delta > 0.0,
            "harsh voice should have ΔR_dB > 0, \
             got {delta}"
        );
    }

    #[test]
    fn gains_clamped_to_g_max() {
        let p = load();
        let mut signal = p.spectral_target;
        // Extreme deviation — should clamp
        signal[3] += 20.0;
        signal[4] -= 20.0;
        let gains = ReferenceResolver::compute_gains(&signal, &p.spectral_target);
        for &g in &gains {
            assert!(
                g >= -G_MAX_DB && g <= G_MAX_DB,
                "gain {g} exceeds G_MAX_DB={G_MAX_DB}"
            );
        }
    }

    #[test]
    fn sbr_converges_after_gains() {
        // INV-REF-2: applying gains moves SBR
        // toward reference (|ΔR_after| ≤ |ΔR_before|)
        let p = load();
        let mut signal = p.spectral_target;
        signal[SBR_LO] += 4.0;
        signal[SBR_HI] -= 3.0;

        let delta_before = ReferenceResolver::compute_sbr_delta(&signal, &p.spectral_target);

        let gains = ReferenceResolver::compute_gains(&signal, &p.spectral_target);
        let mut output = signal;
        for k in 0..8 {
            output[k] += gains[k];
        }

        let delta_after = ReferenceResolver::compute_sbr_delta(&output, &p.spectral_target);

        assert!(
            delta_after.abs() <= delta_before.abs() + 1e-5,
            "INV-REF-2 violated: SBR did not \
             converge. before={delta_before:.4}, \
             after={delta_after:.4}"
        );
    }
}
