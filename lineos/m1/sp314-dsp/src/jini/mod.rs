//! JINI rule-based fallback engine.
//! Authority: JINI Spec v1.0 J-P2
//! Pure function. Deterministic. No LLM, no randomness.
//! Used when Ollama is unavailable or times out.
//! Same input → same output. Always.

use lineos_types::{
    JiniSuggestion, JiniAction, JiniPersonaId,
    BehaviourVector,
    LoudnessBehaviour, SpectralBehaviour,
    DynamicsBehaviour, StereoBehaviour, QualityBehaviour,
    MacroHandle, FlavourId,
};

/// Deterministic rule-based suggestion.
/// Maps BehaviourVector → JiniSuggestion without LLM.
/// Used as fallback when Ollama is unavailable.
pub fn rule_based_suggestion(
    behaviour: &BehaviourVector,
    persona:   &JiniPersonaId,
) -> JiniSuggestion {
    let action = derive_action(behaviour);
    let narrative = render_narrative(&action, behaviour, persona);
    JiniSuggestion {
        narrative,
        action: Some(action),
        confidence:   0.85,
        persona_used: persona.clone(),
    }
}

fn derive_action(b: &BehaviourVector) -> JiniAction {
    // Priority order: Quality → Loudness → Spectral → Dynamics → Stereo
    match &b.quality {
        QualityBehaviour::Clipping =>
            return JiniAction::SuggestMacroChange {
                handle: MacroHandle::Loudness,
                delta:  -0.2,
                reason: "clipping detected".to_string(),
            },
        QualityBehaviour::Silence =>
            return JiniAction::SuggestNothing,
        QualityBehaviour::Clean => {}
        _ => {}
    }

    match &b.loudness {
        LoudnessBehaviour::TooLoud =>
            return JiniAction::SuggestMacroChange {
                handle: MacroHandle::Loudness,
                delta:  -0.2,
                reason: "output level too high".to_string(),
            },
        LoudnessBehaviour::TooQuiet =>
            return JiniAction::SuggestMacroChange {
                handle: MacroHandle::Loudness,
                delta:  0.2,
                reason: "output level too low".to_string(),
            },
        LoudnessBehaviour::Balanced => {}
        _ => {}
    }

    match &b.spectral {
        SpectralBehaviour::Muddy =>
            return JiniAction::SuggestFlavourSwitch {
                to:     FlavourId::Clean,
                reason: "low-mid buildup detected".to_string(),
            },
        SpectralBehaviour::Harsh =>
            return JiniAction::SuggestFlavourSwitch {
                to:     FlavourId::Warm,
                reason: "high-mid harshness detected".to_string(),
            },
        SpectralBehaviour::Thin =>
            return JiniAction::SuggestFlavourSwitch {
                to:     FlavourId::Warm,
                reason: "low-end energy lacking".to_string(),
            },
        SpectralBehaviour::Boxy =>
            return JiniAction::SuggestMacroChange {
                handle: MacroHandle::Tone,
                delta:  -0.2,
                reason: "boxiness in low-mids".to_string(),
            },
        SpectralBehaviour::Bright | SpectralBehaviour::Neutral => {}
        _ => {}
    }

    match &b.dynamics {
        DynamicsBehaviour::Overcompressed | DynamicsBehaviour::OverCompressed =>
            return JiniAction::SuggestMacroChange {
                handle: MacroHandle::Dynamics,
                delta:  -0.2,
                reason: "dynamics too compressed".to_string(),
            },
        DynamicsBehaviour::Pumping =>
            return JiniAction::SuggestMacroChange {
                handle: MacroHandle::Dynamics,
                delta:  -0.15,
                reason: "compression pumping detected".to_string(),
            },
        DynamicsBehaviour::Undercompressed | DynamicsBehaviour::Stable => {}
        _ => {}
    }

    match &b.stereo {
        StereoBehaviour::Mono | StereoBehaviour::Narrow =>
            return JiniAction::SuggestMacroChange {
                handle: MacroHandle::Width,
                delta:  0.15,
                reason: "stereo field too narrow".to_string(),
            },
        StereoBehaviour::Unstable =>
            return JiniAction::SuggestMacroChange {
                handle: MacroHandle::Width,
                delta:  -0.2,
                reason: "phase instability detected".to_string(),
            },
        StereoBehaviour::Wide => {}
        _ => {}
    }

    JiniAction::SuggestNothing
}

fn render_narrative(
    action:    &JiniAction,
    behaviour: &BehaviourVector,
    persona:   &JiniPersonaId,
) -> String {
    match persona {
        JiniPersonaId::Beginner =>
            render_beginner(action, behaviour),
        JiniPersonaId::Intermediate =>
            render_intermediate(action, behaviour),
        JiniPersonaId::Pro =>
            render_pro(action, behaviour),
    }
}

fn render_beginner(action: &JiniAction, b: &BehaviourVector) -> String {
    match action {
        JiniAction::SuggestFlavourSwitch { to, .. } => match to {
            FlavourId::Clean => "Your track sounds a bit heavy — Clean mode will open it up.".to_string(),
            FlavourId::Warm  => "Your track sounds a bit harsh — Warm mode will smooth it out.".to_string(),
            _ => "Try switching modes to get a better sound.".to_string(),
        },
        JiniAction::SuggestMacroChange { handle, delta, .. } => match handle {
            MacroHandle::Loudness if *delta < 0.0 =>
                "Your track is a bit too loud — let's bring it down slightly.".to_string(),
            MacroHandle::Loudness =>
                "Your track could be a bit louder — let's bring it up.".to_string(),
            MacroHandle::Dynamics if *delta < 0.0 =>
                "Your track sounds over-processed — let it breathe a little.".to_string(),
            MacroHandle::Width if *delta < 0.0 =>
                "The stereo feels unstable — let's narrow it a touch.".to_string(),
            MacroHandle::Width =>
                "Your track sounds quite narrow — let's open it up.".to_string(),
            _ => "A small adjustment should help things sit better.".to_string(),
        },
        JiniAction::SuggestNothing => match &b.quality {
            QualityBehaviour::Silence => "No signal detected — check your file.".to_string(),
            _ => "Everything sounds good!".to_string(),
        },
    }
}

fn render_intermediate(action: &JiniAction, _b: &BehaviourVector) -> String {
    match action {
        JiniAction::SuggestFlavourSwitch { to, reason, .. } =>
            format!("{} — switching to {:?} mode should help.", reason, to),
        JiniAction::SuggestMacroChange { handle, delta, reason } =>
            format!("{} — {} {:?} by {:.0}%.",
                reason,
                if *delta < 0.0 { "reduce" } else { "increase" },
                handle,
                delta.abs() * 100.0),
        JiniAction::SuggestNothing => "Signal analysis complete — no adjustments needed.".to_string(),
    }
}

fn render_pro(action: &JiniAction, _b: &BehaviourVector) -> String {
    match action {
        JiniAction::SuggestFlavourSwitch { to, reason } =>
            format!("{} → {:?}.", reason, to),
        JiniAction::SuggestMacroChange { handle, delta, reason } =>
            format!("{} {:?} {:+.2}.", reason, handle, delta),
        JiniAction::SuggestNothing =>
            "No action required.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::*;

    #[test]
    fn neutral_behaviour_suggests_nothing() {
        let b = BehaviourVector::neutral();
        let s = rule_based_suggestion(&b, &JiniPersonaId::Pro);
        assert_eq!(s.action, Some(JiniAction::SuggestNothing));
    }

    #[test]
    fn muddy_suggests_clean_flavour() {
        let mut b = BehaviourVector::neutral();
        b.spectral = SpectralBehaviour::Muddy;
        let s = rule_based_suggestion(&b, &JiniPersonaId::Pro);
        assert!(matches!(s.action,
            Some(JiniAction::SuggestFlavourSwitch { to: FlavourId::Clean, .. })));
    }

    #[test]
    fn harsh_suggests_warm_flavour() {
        let mut b = BehaviourVector::neutral();
        b.spectral = SpectralBehaviour::Harsh;
        let s = rule_based_suggestion(&b, &JiniPersonaId::Pro);
        assert!(matches!(s.action,
            Some(JiniAction::SuggestFlavourSwitch { to: FlavourId::Warm, .. })));
    }

    #[test]
    fn overcompressed_suggests_dynamics_reduction() {
        let mut b = BehaviourVector::neutral();
        b.dynamics = DynamicsBehaviour::Overcompressed;
        let s = rule_based_suggestion(&b, &JiniPersonaId::Intermediate);
        assert!(matches!(s.action,
            Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Dynamics, .. })));
    }

    #[test]
    fn persona_changes_narrative_not_action() {
        let mut b = BehaviourVector::neutral();
        b.spectral = SpectralBehaviour::Muddy;
        let s_beginner     = rule_based_suggestion(&b, &JiniPersonaId::Beginner);
        let s_intermediate = rule_based_suggestion(&b, &JiniPersonaId::Intermediate);
        let s_pro          = rule_based_suggestion(&b, &JiniPersonaId::Pro);
        // Same action
        assert_eq!(s_beginner.action,     s_intermediate.action);
        assert_eq!(s_intermediate.action, s_pro.action);
        // Different narrative
        assert_ne!(s_beginner.narrative,     s_pro.narrative);
        assert_ne!(s_intermediate.narrative, s_pro.narrative);
    }

    #[test]
    fn clipping_takes_priority_over_spectral() {
        let mut b = BehaviourVector::neutral();
        b.quality  = QualityBehaviour::Clipping;
        b.spectral = SpectralBehaviour::Muddy;
        let s = rule_based_suggestion(&b, &JiniPersonaId::Pro);
        // Quality check fires before spectral
        assert!(matches!(s.action,
            Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Loudness, .. })));
    }
}
