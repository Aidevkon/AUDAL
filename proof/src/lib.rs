// proof/src/lib.rs — Execution Proof crate
// Authority: spec/locked/S-010_execution_proof.md v1.0

pub mod certificate;
pub mod proof;

pub use certificate::{ExecutionCertificate, VerificationError};
pub use proof::ExecutionProof;
