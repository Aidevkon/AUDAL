//! JINI — Constitutional types for the personality layer.
//! Authority: jini-spec-v1.0 §4 Data Contracts
//!
//! JINI reads BehaviourVector (semantic, no DSP jargon).
//! JINI suggests. Never auto-applies.

use serde::{Deserialize, Serialize};

// ── Behaviour Enums (shared: lineos-types is the canonical location) ─────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoudnessBehaviour {
    TooQuiet,
    SlightlyQuiet,
    Optimal,
    Balanced,
    SlightlyLoud,
    TooLoud,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpectralBehaviour {
    Muddy,
    Boxy,
    Balanced,
    Neutral,
    Harsh,
    Bright,
    Thin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DynamicsBehaviour {
    Overcompressed,
    OverCompressed,
    Tight,
    Balanced,
    Stable,
    Dynamic,
    Pumping,
    Undercompressed,
    Uncontrolled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StereoBehaviour {
    Mono,
    Narrow,
    Balanced,
    Wide,
    Unstable,
    PhaseIssue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QualityBehaviour {
    Clean,
    MinorIssues,
    Clipping,
    Distorted,
    Silence,
}

// ── Shared Domain Enums ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FlavourId {
    Warm,
    Clean,
    Punch,
    Air,
    Film,
    Broadcast,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpinoffTarget {
    Pox,
    Audiobook,
    Music,
    Universal,
}

/// Stem identity — matches NMF 4-stem architecture.
/// Note: Vocals was renamed to Harmonics in NMF upgrade 4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StemKind {
    Bass,
    Harmonics,
    Drums,
    Ambience,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MacroHandle {
    Tone,
    Dynamics,
    Space,
    Loudness,
    Width,
}

// ── JINI Core Types (§4 Data Contracts) ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviourVector {
    pub loudness: LoudnessBehaviour,
    pub spectral: SpectralBehaviour,
    pub dynamics: DynamicsBehaviour,
    pub stereo: StereoBehaviour,
    pub quality: QualityBehaviour,
}

impl BehaviourVector {
    pub fn neutral() -> Self {
        Self {
            loudness: LoudnessBehaviour::Balanced,
            spectral: SpectralBehaviour::Neutral,
            dynamics: DynamicsBehaviour::Stable,
            stereo: StereoBehaviour::Wide,
            quality: QualityBehaviour::Clean,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroState {
    pub tone: f32,
    pub dynamics: f32,
    pub space: f32,
    pub loudness: f32,
    pub width: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JiniPersonaId {
    Beginner,
    Intermediate,
    Pro,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiniContext {
    pub spinoff: SpinoffTarget,
    pub stem_focus: Option<StemKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiniInput {
    pub behaviour: BehaviourVector,
    pub persona: JiniPersonaId,
    pub current_macros: MacroState,
    pub flavour: FlavourId,
    pub context: JiniContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JiniSuggestion {
    pub narrative: String,
    pub action: Option<JiniAction>,
    pub confidence: f32,
    pub persona_used: JiniPersonaId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JiniAction {
    SuggestMacroChange {
        handle: MacroHandle,
        delta: f32, // bounded [-0.3, +0.3]
        reason: String,
    },
    SuggestFlavourSwitch {
        to: FlavourId,
        reason: String,
    },
    SuggestNothing,
}

// ── PersonaSchema (compile-time only — no Deserialize) ───────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PersonaSchema {
    pub id: JiniPersonaId,
    pub system_prompt: &'static str,
    pub temperature: f32,
    pub max_tokens: u32,
    pub vocabulary_level: VocabLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VocabLevel {
    Simple,
    Intermediate,
    Technical,
}

// ── Schema Constants (§3 Personas) ───────────────────────────────────────────

pub const BRAND_VOICE_CONSTITUTION: &str = "\
You are JINI, the instrument layer of Creator OS. \
You are NOT an assistant, chatbot, or character with emotions. \
You ARE an avionics-grade, deterministic instrument.\n\
ABSOLUTE RULES:\n\
1. Maximum 2 sentences per response.\n\
2. Action-first phrasing only.\n\
3. Never use: hey, hi, hello, sorry, oops, amazing, perfect, \
let's, maybe, probably, I think.\n\
4. Never use emojis or exclamation marks.\n\
5. Never identify as AI or language model. You are Creator OS.\n\
6. No filler words. If a word can be cut, cut it.\n\
CORE TONE: Calm. Precise. Low-emotion. Instrument-grade.";

pub const SCHEMA_BEGINNER: PersonaSchema = PersonaSchema {
    id: JiniPersonaId::Beginner,
    system_prompt: "You are a friendly music mentor helping someone new to audio. \
                       Speak warmly and simply. No technical terms. Use musical analogies. \
                       Keep suggestions to one or two sentences. Always be encouraging.",
    temperature: 0.7,
    max_tokens: 256,
    vocabulary_level: VocabLevel::Simple,
};

pub const SCHEMA_INTERMEDIATE: PersonaSchema = PersonaSchema {
    id: JiniPersonaId::Intermediate,
    system_prompt: "You are an experienced audio engineer helping an intermediate producer. \
                       Explain what you hear and why it matters. \
                       Use DSP terms but explain them briefly. Be direct and helpful.",
    temperature: 0.5,
    max_tokens: 384,
    vocabulary_level: VocabLevel::Intermediate,
};

pub const SCHEMA_PRO: PersonaSchema = PersonaSchema {
    id: JiniPersonaId::Pro,
    system_prompt: "You are a mastering engineer speaking to another engineer. \
                       Be precise and technical. Reference standards where relevant. \
                       No explanations unless asked. Maximum two sentences.",
    temperature: 0.3,
    max_tokens: 256,
    vocabulary_level: VocabLevel::Technical,
};

// ── Ollama Constants (§5.3) ──────────────────────────────────────────────────

pub const OLLAMA_TIMEOUT_MS: u64 = 5_000;
pub const OLLAMA_ENDPOINT: &str = "http://localhost:11434/api/generate";
pub const GEMMA_MODEL: &str = "llama3.2:1b";
