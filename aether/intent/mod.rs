// aether/intent/mod.rs — S-004 Intent Parser
// Authority: spec/locked/S-004_intent_parser.md v1.0

pub mod parser;
pub mod types;

pub use parser::IntentParser;
pub use types::{Intent, IntentGoal, IntentStrength};
