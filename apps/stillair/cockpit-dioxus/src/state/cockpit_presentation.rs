// state/cockpit_presentation.rs — CockpitPresentation
// Authority: ADR-C0 v1.0 (locked 2026-05-29)
//
// Centralises all tier→visibility decisions.
// No component performs inline `tier >= X` checks except through this struct.
// Tier enforcement never enters LineOS or Aether.

use crate::types::CockpitTier;

/// All tier-dependent UI visibility flags, computed once from CockpitTier.
/// Passed as a prop — components never inspect tier directly.
#[derive(Debug, Clone, PartialEq)]
pub struct CockpitPresentation {
    pub show_play_stop:        bool,
    pub show_timecode:         bool,
    pub show_abort:            bool,
    pub show_skip:             bool,
    pub show_scrub:            bool,
    pub show_ab_toggle:        bool,
    pub show_annunciators:     bool,
    pub show_oled_tile:        bool,
    pub show_ha_button:        bool,
    pub show_intent_bay:       bool,
    pub show_coach_hud:        bool,
    pub show_abcd_commit:      bool,
    pub show_delta_analysis:   bool,
    pub show_transport_intent: bool, // always false at launch (ADR-C0.6)
    pub tier_label:            &'static str,
}

impl CockpitPresentation {
    /// Build presentation flags from tier.
    /// This is the ONLY place tier→visibility mapping lives.
    pub fn from_tier(tier: CockpitTier) -> Self {
        let t2 = tier >= CockpitTier::Tier2_Medium;
        let t3 = tier >= CockpitTier::Tier3_Pro;

        Self {
            // Visible at ALL tiers (ADR-C0.1, C0.3, C0.4, C0.8)
            show_play_stop:        true,
            show_timecode:         true,
            show_abort:            true,
            show_skip:             true,
            show_scrub:            true,
            show_ab_toggle:        true,
            show_annunciators:     true,
            show_oled_tile:        true,

            // Tier 2+ (ADR-C0.5)
            show_ha_button:        t2,
            show_intent_bay:       t2,

            // Tier 3 only
            show_coach_hud:        t3,
            show_abcd_commit:      t3,
            show_delta_analysis:   t3,

            // Always false at launch — post-launch decision (ADR-C0.6)
            show_transport_intent: false,

            // Label
            tier_label: match tier {
                CockpitTier::Tier1_BlackBox => "BLACK BOX",
                CockpitTier::Tier2_Medium   => "MEDIUM",
                CockpitTier::Tier3_Pro      => "PRO",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier1_blackbox_shows_core_controls() {
        let p = CockpitPresentation::from_tier(CockpitTier::Tier1_BlackBox);
        assert!(p.show_play_stop);
        assert!(p.show_abort);
        assert!(p.show_skip);
        assert!(p.show_scrub);
        assert!(p.show_ab_toggle);
        assert!(p.show_annunciators);
        assert!(p.show_oled_tile);
    }

    #[test]
    fn tier1_blackbox_hides_advanced() {
        let p = CockpitPresentation::from_tier(CockpitTier::Tier1_BlackBox);
        assert!(!p.show_ha_button);
        assert!(!p.show_intent_bay);
        assert!(!p.show_coach_hud);
        assert!(!p.show_abcd_commit);
        assert!(!p.show_delta_analysis);
        assert!(!p.show_transport_intent);
    }

    #[test]
    fn tier2_medium_unlocks_ha_and_intent() {
        let p = CockpitPresentation::from_tier(CockpitTier::Tier2_Medium);
        assert!(p.show_ha_button);
        assert!(p.show_intent_bay);
        assert!(!p.show_coach_hud);
        assert!(!p.show_transport_intent);
    }

    #[test]
    fn tier3_pro_unlocks_all_except_transport_intent() {
        let p = CockpitPresentation::from_tier(CockpitTier::Tier3_Pro);
        assert!(p.show_ha_button);
        assert!(p.show_intent_bay);
        assert!(p.show_coach_hud);
        assert!(p.show_abcd_commit);
        assert!(p.show_delta_analysis);
        assert!(!p.show_transport_intent); // always false at launch
    }

    #[test]
    fn transport_intent_always_false() {
        for tier in [CockpitTier::Tier1_BlackBox,
                     CockpitTier::Tier2_Medium,
                     CockpitTier::Tier3_Pro] {
            assert!(!CockpitPresentation::from_tier(tier).show_transport_intent);
        }
    }

    #[test]
    fn tier_labels_correct() {
        assert_eq!(CockpitPresentation::from_tier(CockpitTier::Tier1_BlackBox).tier_label, "BLACK BOX");
        assert_eq!(CockpitPresentation::from_tier(CockpitTier::Tier2_Medium).tier_label, "MEDIUM");
        assert_eq!(CockpitPresentation::from_tier(CockpitTier::Tier3_Pro).tier_label, "PRO");
    }
}
