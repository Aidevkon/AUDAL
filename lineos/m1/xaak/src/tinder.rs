//! Mastering Tinder — swipe-to-sound preference learning.
//! FL-T1: generate N DspState variations around current state.
//! Authority: future-roadmap.md Mastering Tinder
//! INV-AB-1: same state + same n → same variations. Always.

use crate::flavours;
use crate::repo::DspState;

/// Generate N variations around a base DspState.
/// Interpolates between base and each flavour preset.
/// INV-AB-1: deterministic — no randomness.
pub fn generate_variations(base: &DspState, n: usize) -> Vec<DspState> {
    if n == 0 {
        return vec![];
    }

    let presets: Vec<DspState> = flavours::ALL.iter().map(|(_, s)| *s).collect();

    let mut variations = Vec::with_capacity(n);

    // Always include the base state as first variation
    variations.push(*base);

    // Interpolate between base and each flavour
    for preset in presets.iter() {
        if variations.len() >= n {
            break;
        }
        let t = 0.5_f32; // midpoint between base and preset
        variations.push(lerp_state(base, preset, t));

        // Also add the full preset if we have room
        if variations.len() < n {
            variations.push(*preset);
        }
    }

    // Trim or pad to exactly n
    variations.truncate(n);
    while variations.len() < n {
        // Fill remaining with slight variations of base
        let t = variations.len() as f32 / n as f32;
        let target = presets[variations.len() % presets.len()];
        variations.push(lerp_state(base, &target, t * 0.3));
    }

    variations
}

/// Weighted centroid of liked DspStates.
/// The "sound" that emerges from user preferences.
pub fn weighted_centroid(liked: &[DspState]) -> Option<DspState> {
    if liked.is_empty() {
        return None;
    }
    let n = liked.len() as f32;
    Some(DspState {
        ducking_depth: liked.iter().map(|s| s.ducking_depth).sum::<f32>() / n,
        ms_width: liked.iter().map(|s| s.ms_width).sum::<f32>() / n,
        lfe_gain: liked.iter().map(|s| s.lfe_gain).sum::<f32>() / n,
        sidechain_hold: (liked.iter().map(|s| s.sidechain_hold).sum::<usize>() / liked.len()),
    })
}

fn lerp_state(a: &DspState, b: &DspState, t: f32) -> DspState {
    DspState {
        ducking_depth: a.ducking_depth + (b.ducking_depth - a.ducking_depth) * t,
        ms_width: a.ms_width + (b.ms_width - a.ms_width) * t,
        lfe_gain: a.lfe_gain + (b.lfe_gain - a.lfe_gain) * t,
        sidechain_hold: a.sidechain_hold,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_correct_count() {
        let base = DspState::default();
        for n in [1, 4, 8, 12] {
            let v = generate_variations(&base, n);
            assert_eq!(v.len(), n, "Expected {} variations, got {}", n, v.len());
        }
    }

    #[test]
    fn first_variation_is_base() {
        let base = DspState::default();
        let v = generate_variations(&base, 8);
        assert!((v[0].ducking_depth - base.ducking_depth).abs() < 0.001);
    }

    #[test]
    fn weighted_centroid_of_one() {
        let s = DspState {
            ducking_depth: 1.5,
            ms_width: 1.2,
            sidechain_hold: 4,
            lfe_gain: 1.0,
        };
        let c = weighted_centroid(&[s]).unwrap();
        assert!((c.ducking_depth - 1.5).abs() < 0.001);
    }

    #[test]
    fn weighted_centroid_midpoint() {
        let a = DspState {
            ducking_depth: 1.0,
            ms_width: 1.0,
            sidechain_hold: 3,
            lfe_gain: 0.0,
        };
        let b = DspState {
            ducking_depth: 2.0,
            ms_width: 2.0,
            sidechain_hold: 3,
            lfe_gain: 2.0,
        };
        let c = weighted_centroid(&[a, b]).unwrap();
        assert!((c.ducking_depth - 1.5).abs() < 0.001);
        assert!((c.ms_width - 1.5).abs() < 0.001);
    }

    #[test]
    fn deterministic_same_output() {
        let base = DspState::default();
        let v1 = generate_variations(&base, 8);
        let v2 = generate_variations(&base, 8);
        for (a, b) in v1.iter().zip(v2.iter()) {
            assert!((a.ducking_depth - b.ducking_depth).abs() < 0.001);
        }
    }
}
