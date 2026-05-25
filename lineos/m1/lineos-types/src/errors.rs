// src/errors.rs
// Shared error types for lineos-types

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineosError {
    pub message: String,
    pub code: String,
}

impl std::fmt::Display for LineosError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for LineosError {}
