//! Insights report serialization.
//! Re-exports InsightsReport for external consumers.

pub use crate::evaluator::InsightsReport;

use crate::evaluator::InsightsReport as IR;

impl IR {
    /// Serialize to a pretty-printed JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
