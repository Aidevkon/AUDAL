//! Schema Agent — validates JiniSuggestion before it reaches the UI.
//! Authority: JINI Spec v1.0 §6, ARCHITECTURE v3.4 §Schema Agent
//! Every suggestion passes through here. Invalid → silent drop → fallback.
//! INV-JINI-3: no suggestion shown without validation.
//! Schema Agent is the sole writer of ProjectState (future).

use lineos_types::{JiniAction, JiniSuggestion};

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    NarrativeEmpty,
    NarrativeTooLong(usize),   // max 500
    ConfidenceOutOfRange(f32), // must be [0.0, 1.0]
    DeltaOutOfRange(f32),      // must be [-0.3, +0.3] INV-JINI-7
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn ok() -> Self {
        Self {
            valid: true,
            errors: vec![],
        }
    }
    pub fn fail(e: ValidationError) -> Self {
        Self {
            valid: false,
            errors: vec![e],
        }
    }
}

pub fn validate(s: &JiniSuggestion) -> ValidationResult {
    // INV-JINI-11: narrative non-empty and ≤ 500 chars
    if s.narrative.is_empty() {
        return ValidationResult::fail(ValidationError::NarrativeEmpty);
    }
    if s.narrative.chars().count() > 500 {
        return ValidationResult::fail(ValidationError::NarrativeTooLong(
            s.narrative.chars().count(),
        ));
    }

    // Confidence in [0.0, 1.0]
    if s.confidence < 0.0 || s.confidence > 1.0 {
        return ValidationResult::fail(ValidationError::ConfidenceOutOfRange(s.confidence));
    }

    // Validate action if present
    if let Some(ref action) = s.action {
        match action {
            JiniAction::SuggestMacroChange { delta, .. } => {
                // INV-JINI-7: delta bounded [-0.3, +0.3]
                if *delta < -0.3 || *delta > 0.3 {
                    return ValidationResult::fail(ValidationError::DeltaOutOfRange(*delta));
                }
            }
            JiniAction::SuggestFlavourSwitch { .. } => {}
            JiniAction::SuggestNothing => {}
        }
    }

    ValidationResult::ok()
}

/// Validate and return suggestion if valid, fallback if not.
pub fn validated_or_fallback(
    suggestion: JiniSuggestion,
    fallback: JiniSuggestion,
) -> JiniSuggestion {
    if validate(&suggestion).valid {
        suggestion
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::*;

    fn valid_suggestion() -> JiniSuggestion {
        JiniSuggestion {
            narrative: "Test narrative".to_string(),
            action: Some(JiniAction::SuggestNothing),
            confidence: 0.85,
            persona_used: JiniPersonaId::Pro,
        }
    }

    #[test]
    fn valid_suggestion_passes() {
        assert!(validate(&valid_suggestion()).valid);
    }

    #[test]
    fn empty_narrative_fails() {
        let mut s = valid_suggestion();
        s.narrative = String::new();
        assert!(!validate(&s).valid);
    }

    #[test]
    fn narrative_over_500_fails() {
        let mut s = valid_suggestion();
        s.narrative = "x".repeat(501);
        assert!(!validate(&s).valid);
    }

    #[test]
    fn confidence_out_of_range_fails() {
        let mut s = valid_suggestion();
        s.confidence = 1.5;
        assert!(!validate(&s).valid);
    }

    #[test]
    fn delta_out_of_range_fails() {
        let mut s = valid_suggestion();
        s.action = Some(JiniAction::SuggestMacroChange {
            handle: MacroHandle::Tone,
            delta: 0.5, // > 0.3 — invalid
            reason: "test".to_string(),
        });
        assert!(!validate(&s).valid);
    }

    #[test]
    fn delta_at_boundary_passes() {
        let mut s = valid_suggestion();
        s.action = Some(JiniAction::SuggestMacroChange {
            handle: MacroHandle::Tone,
            delta: 0.3, // exactly at boundary — valid
            reason: "test".to_string(),
        });
        assert!(validate(&s).valid);
    }

    #[test]
    fn negative_delta_at_boundary_passes() {
        let mut s = valid_suggestion();
        s.action = Some(JiniAction::SuggestMacroChange {
            handle: MacroHandle::Dynamics,
            delta: -0.3,
            reason: "test".to_string(),
        });
        assert!(validate(&s).valid);
    }

    #[test]
    fn invalid_suggestion_returns_fallback() {
        let mut bad = valid_suggestion();
        bad.narrative = String::new();
        let fallback = valid_suggestion();
        let result = validated_or_fallback(bad, fallback.clone());
        assert_eq!(result.narrative, fallback.narrative);
    }
}
