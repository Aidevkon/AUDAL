// aether/intent/mod.rs — S-004 Intent Parser
// Authority: spec/locked/S-004_intent_parser.md v1.0

pub mod types;
pub mod parser;

pub use types::{Intent, IntentGoal, IntentStrength};
pub use parser::IntentParser;
