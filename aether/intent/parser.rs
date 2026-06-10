// aether/intent/parser.rs — IntentParser
// Authority: spec/locked/S-004_intent_parser.md v1.0
//
// Two resolution paths:
//   Rule path:  deterministic macro handle → Intent (no ML)
//   LLM path:   text string → Intent (adapter-runtime boundary)
//
// Resolution order (§6):
//   1. Explicit persona selection ("use warm_analog")
//   2. Directional handle keywords ("more warmth", "less punch")
//   3. Semantic synonyms ("brighter", "darker", "punchier")
//   4. NoOp if nothing matches

use super::types::{Intent, IntentGoal, IntentStrength};

pub struct IntentParser;

impl IntentParser {
    /// Rule-based path: parse a text string into an Intent.
    /// Deterministic — no ML, no randomness.
    /// Case-insensitive, keyword matching.
    pub fn parse_text(input: &str) -> Intent {
        let lower = input.to_lowercase();

        // Resolution order §6:

        // 1. Explicit persona selection
        for persona_id in [
            "warm_analog",
            "clean_punch",
            "hybrid_hifi",
            "cinematic_wide",
        ] {
            if lower.contains(persona_id) || lower.contains(&persona_id.replace('_', " ")) {
                return Intent {
                    goal: IntentGoal::SelectPersona(persona_id.to_string()),
                    strength: IntentStrength::new(1.0),
                    context: Some(input.to_string()),
                };
            }
        }

        // 2 + 3. Directional handle + semantic synonyms
        let goal = Self::match_goal(&lower);
        let strength = Self::extract_strength(&lower);

        Intent {
            goal,
            strength: IntentStrength::new(strength),
            context: None,
        }
    }

    /// Direct macro handle path: convert handle value change to Intent.
    /// Used by Tier 2/3 UI (S-011a) when engineer moves a handle.
    /// delta > 0 = increase, delta < 0 = decrease.
    pub fn from_handle(handle: &str, delta: f32) -> Intent {
        let goal = match (handle, delta >= 0.0) {
            ("warmth", true) => IntentGoal::IncreaseWarmth,
            ("warmth", false) => IntentGoal::DecreaseWarmth,
            ("punch", true) => IntentGoal::IncreasePunch,
            ("punch", false) => IntentGoal::DecreasePunch,
            ("forwardness", true) => IntentGoal::IncreaseForwardness,
            ("forwardness", false) => IntentGoal::DecreaseForwardness,
            ("smoothness", true) => IntentGoal::IncreaseSmootness,
            ("smoothness", false) => IntentGoal::DecreaseSmootness,
            _ => IntentGoal::NoOp,
        };
        Intent::new(goal, delta.abs().clamp(0.0, 1.0))
    }

    // ── private helpers ──────────────────────────────────────────

    fn match_goal(lower: &str) -> IntentGoal {
        // Warmth
        if Self::has_any(
            lower,
            &[
                "more warm",
                "warmer",
                "add warm",
                "more body",
                "fuller",
                "richer",
                "more low",
                "boost low",
            ],
        ) {
            return IntentGoal::IncreaseWarmth;
        }
        if Self::has_any(
            lower,
            &[
                "less warm",
                "less body",
                "thin",
                "reduce mud",
                "less mud",
                "cleaner low",
            ],
        ) {
            return IntentGoal::DecreaseWarmth;
        }

        // Punch
        if Self::has_any(
            lower,
            &[
                "more punch",
                "punchier",
                "more attack",
                "tighter",
                "more transient",
                "harder hit",
                "more impact",
            ],
        ) {
            return IntentGoal::IncreasePunch;
        }
        if Self::has_any(
            lower,
            &[
                "less punch",
                "softer",
                "less attack",
                "smoother attack",
                "less transient",
            ],
        ) {
            return IntentGoal::DecreasePunch;
        }

        // Forwardness
        if Self::has_any(
            lower,
            &[
                "more forward",
                "more presence",
                "brighter",
                "more air",
                "more open",
                "more clarity",
                "cut through",
            ],
        ) {
            return IntentGoal::IncreaseForwardness;
        }
        if Self::has_any(
            lower,
            &[
                "less forward",
                "pull back",
                "darker",
                "less presence",
                "more recessed",
                "less harsh",
                "less bright",
            ],
        ) {
            return IntentGoal::DecreaseForwardness;
        }

        // Smoothness
        if Self::has_any(
            lower,
            &[
                "smoother",
                "less harsh",
                "remove harsh",
                "less sibilanc",
                "de-ess",
                "softer high",
                "less edge",
                "more polished",
            ],
        ) {
            return IntentGoal::IncreaseSmootness;
        }
        if Self::has_any(
            lower,
            &[
                "more edge",
                "more grit",
                "more aggress",
                "less smooth",
                "rawer",
                "more bite",
            ],
        ) {
            return IntentGoal::DecreaseSmootness;
        }

        IntentGoal::NoOp
    }

    fn extract_strength(lower: &str) -> f32 {
        // Intensity modifiers
        if Self::has_any(
            lower,
            &["slightly", "a bit", "subtle", "a little", "just a touch"],
        ) {
            return 0.25;
        }
        if Self::has_any(
            lower,
            &[
                "much more",
                "much less",
                "a lot",
                "significantly",
                "dramatically",
                "extremely",
                "way more",
                "way less",
            ],
        ) {
            return 0.9;
        }
        // Default: medium strength
        0.5
    }

    fn has_any(text: &str, patterns: &[&str]) -> bool {
        patterns.iter().any(|p| {
            // Use word-boundary check: pattern must be preceded and
            // followed by non-alphanumeric character (or string boundary)
            let mut start = 0;
            while let Some(pos) = text[start..].find(p) {
                let abs_pos = start + pos;
                let before_ok =
                    abs_pos == 0 || !text.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
                let after_pos = abs_pos + p.len();
                let after_ok =
                    after_pos >= text.len() || !text.as_bytes()[after_pos].is_ascii_alphanumeric();
                if before_ok && after_ok {
                    return true;
                }
                start = abs_pos + 1;
                if start >= text.len() {
                    break;
                }
            }
            false
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::types::IntentGoal;

    #[test]
    fn parse_warmth_increase() {
        let i = IntentParser::parse_text("make it warmer");
        assert_eq!(i.goal, IntentGoal::IncreaseWarmth);
    }

    #[test]
    fn parse_punch_increase() {
        let i = IntentParser::parse_text("I want more punch");
        assert_eq!(i.goal, IntentGoal::IncreasePunch);
    }

    #[test]
    fn parse_persona_selection() {
        let i = IntentParser::parse_text("use warm analog");
        assert_eq!(i.goal, IntentGoal::SelectPersona("warm_analog".into()));
    }

    #[test]
    fn parse_noop_on_unknown() {
        let i = IntentParser::parse_text("something completely unrelated");
        assert_eq!(i.goal, IntentGoal::NoOp);
    }

    #[test]
    fn parse_strength_subtle() {
        let i = IntentParser::parse_text("slightly warmer please");
        assert_eq!(i.goal, IntentGoal::IncreaseWarmth);
        assert!((i.strength.value() - 0.25).abs() < 1e-5);
    }

    #[test]
    fn parse_strength_strong() {
        let i = IntentParser::parse_text("much more punch");
        assert_eq!(i.goal, IntentGoal::IncreasePunch);
        assert!((i.strength.value() - 0.9).abs() < 1e-5);
    }

    #[test]
    fn from_handle_warmth_positive() {
        let i = IntentParser::from_handle("warmth", 0.3);
        assert_eq!(i.goal, IntentGoal::IncreaseWarmth);
        assert!((i.strength.value() - 0.3).abs() < 1e-5);
    }

    #[test]
    fn from_handle_punch_negative() {
        let i = IntentParser::from_handle("punch", -0.7);
        assert_eq!(i.goal, IntentGoal::DecreasePunch);
        assert!((i.strength.value() - 0.7).abs() < 1e-5);
    }

    #[test]
    fn intent_serializable() {
        let i = Intent::new(IntentGoal::IncreaseWarmth, 0.5);
        let json = serde_json::to_string(&i).unwrap();
        let i2: Intent = serde_json::from_str(&json).unwrap();
        assert_eq!(i, i2);
    }

    #[test]
    fn intent_strength_clamped() {
        assert_eq!(IntentStrength::new(99.0).value(), 1.0);
        assert_eq!(IntentStrength::new(-1.0).value(), 0.0);
    }
}
