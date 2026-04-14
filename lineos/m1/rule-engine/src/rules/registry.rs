//! Rule registry — static RULES array. Zero dynamic dispatch.
//! All rules are compile-time known. Adding a rule = add entry here.
//! Never change existing rule behavior without a major version bump.
//! Authority: Coach-Core README §6 · LineOS Constitution v2.0 §07

use crate::types::{AnalysisReport, Issue};
use crate::thresholds::Thresholds;
use super::{loudness, peak, dynamics, stereo};

/// A single rule entry in the static registry.
/// `check` is a plain function pointer — zero dynamic dispatch.
pub struct Rule {
    /// Stable ID matching the rule function name and Issue.id.
    pub id:    &'static str,
    /// Pure function: (report, thresholds) → Option<Issue>
    pub check: fn(&AnalysisReport, &Thresholds) -> Option<Issue>,
    /// Category tags for filtering/grouping (not used in evaluation).
    pub tags:  &'static [&'static str],
}

/// Static registry. Evaluated in declaration order.
/// No rule depends on the output of another (hard invariant).
/// Always evaluates ALL rules — filtering is the Cockpit's responsibility.
pub static RULES: &[Rule] = &[
    Rule { id: "lufs_too_high",           check: loudness::lufs_too_high,          tags: &["loudness"] },
    Rule { id: "lufs_too_low",            check: loudness::lufs_too_low,            tags: &["loudness"] },
    Rule { id: "lufs_apple_too_high",     check: loudness::lufs_apple_too_high,     tags: &["loudness"] },
    Rule { id: "true_peak_exceeded",      check: peak::true_peak_exceeded,          tags: &["peak", "critical"] },
    Rule { id: "dynamic_range_low",       check: dynamics::dynamic_range_low,       tags: &["dynamics"] },
    Rule { id: "lra_too_high",            check: dynamics::lra_too_high,            tags: &["dynamics"] },
    Rule { id: "stereo_correlation_weak", check: stereo::stereo_correlation_weak,   tags: &["stereo"] },
    Rule { id: "dc_offset_detected",      check: stereo::dc_offset_detected,        tags: &["quality"] },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rules_count() {
        assert_eq!(RULES.len(), 8, "Expected exactly 8 core rules");
    }

    #[test]
    fn test_rules_ids_unique() {
        let mut ids: Vec<&str> = RULES.iter().map(|r| r.id).collect();
        let original_len = ids.len();
        ids.dedup();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), original_len, "Rule IDs must be unique");
    }
}
