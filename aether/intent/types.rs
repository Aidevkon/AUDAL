// aether/intent/types.rs — Intent types
// Authority: spec/locked/S-004_intent_parser.md v1.0

/// Semantic goal of the engineer's intent.
/// Deterministic mapping — no ML at this layer.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IntentGoal {
    /// Increase warmth / low-mid body
    IncreaseWarmth,
    /// Decrease warmth / reduce muddiness
    DecreaseWarmth,
    /// Increase punch / transient emphasis
    IncreasePunch,
    /// Decrease punch / soften transients
    DecreasePunch,
    /// Increase forwardness / presence
    IncreaseForwardness,
    /// Decrease forwardness / pull back
    DecreaseForwardness,
    /// Increase smoothness / reduce harshness
    IncreaseSmootness,
    /// Decrease smoothness / add edge
    DecreaseSmootness,
    /// Select a specific persona by ID
    SelectPersona(String),
    /// No-op — intent could not be resolved
    NoOp,
}

/// Strength of the intent [0.0, 1.0].
/// 0.0 = subtle, 1.0 = maximum.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IntentStrength(pub f32);

impl IntentStrength {
    pub fn new(v: f32) -> Self {
        Self(v.clamp(0.0, 1.0))
    }
    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Default for IntentStrength {
    fn default() -> Self {
        Self(0.5)
    }
}

/// Structured intent — output of IntentParser.
/// Validated against contracts/intent.schema.json by S-009.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Intent {
    pub goal: IntentGoal,
    pub strength: IntentStrength,
    /// Optional free-text context (from LLM path)
    pub context: Option<String>,
}

impl Intent {
    pub fn new(goal: IntentGoal, strength: f32) -> Self {
        Self {
            goal,
            strength: IntentStrength::new(strength),
            context: None,
        }
    }

    pub fn noop() -> Self {
        Self::new(IntentGoal::NoOp, 0.0)
    }
}
