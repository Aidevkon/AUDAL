//! Adapter Runtime — LineOS / Creator OS
//! Authority: LLM Adapter Amendment v1.1 §A4
//!
//! This crate is the SOLE permitted LLM invocation point in Creator OS.
//! All LLM calls — from any App or Pipeline — must route through here.
//!
//! FORBIDDEN: Any crate outside this module calling Ollama, OpenAI, Anthropic,
//! or any other LLM API directly.

pub mod llm_client;
pub mod providers;
