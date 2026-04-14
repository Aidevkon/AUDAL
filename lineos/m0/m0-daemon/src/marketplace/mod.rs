//! Marketplace gatekeeper — M0 Constitution v2.0 §03.7
//! 4-step verification: manifest → signature → permissions → digest.
//! Pure Rust: ed25519-dalek + blake3. No ring. No C FFI.

pub mod gatekeeper;
pub mod revocation;

// Re-export primary types at module level for convenience
pub use gatekeeper::{EngineManifest, Gatekeeper, GatekeeperError};
pub use revocation::is_revoked;
