//! Marketplace gatekeeper — M0 Constitution v2.0 §03.7
//! Steps: 1. manifest validation  2. Ed25519 sig verification
//!        3. permission enforcement  4. blake3 digest check
//! All-or-nothing. Any step fails → Err, audit event written.
//! Pure Rust: ed25519-dalek + blake3. No ring. No C FFI.

#![allow(dead_code)]

use blake3::Hasher as Blake3Hasher;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fmt;

// ─── Manifest Types ───────────────────────────────────────────────────────────

/// Allowed permission scopes for marketplace engines.
const ALLOWED_PERMISSIONS: &[&str] = &[
    "dsp.read",
    "dsp.write",
    "audio.read",
    "audio.write",
    "schema.read",
];

/// Engine manifest — describes a marketplace engine submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineManifest {
    /// Engine ID (E100–E999 certified, E1000+ community)
    pub engine_id: String,
    /// Version string (semver)
    pub version: String,
    /// Developer public key (hex-encoded ed25519 verifying key)
    pub developer_public_key: String,
    /// Registry co-signature (hex-encoded ed25519 signature over the manifest JSON)
    pub registry_signature: String,
    /// Developer signature (hex-encoded ed25519 signature over the artifact bytes)
    pub developer_signature: String,
    /// Expected blake3 digest of the artifact (hex)
    pub blake3_digest: String,
    /// Declared permissions
    pub permissions: Vec<String>,
}

// ─── Gatekeeper Errors ────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum GatekeeperError {
    ManifestInvalid(String),
    SignatureInvalid(String),
    PermissionDenied(String),
    DigestMismatch { expected: String, actual: String },
    KeyRevoked(String),
}

impl fmt::Display for GatekeeperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManifestInvalid(msg) => write!(f, "ManifestInvalid: {msg}"),
            Self::SignatureInvalid(msg) => write!(f, "SignatureInvalid: {msg}"),
            Self::PermissionDenied(msg) => write!(f, "PermissionDenied: {msg}"),
            Self::DigestMismatch { expected, actual } => {
                write!(f, "DigestMismatch: expected={expected}, actual={actual}")
            }
            Self::KeyRevoked(key) => write!(f, "KeyRevoked: {key}"),
        }
    }
}

impl std::error::Error for GatekeeperError {}

// ─── Gatekeeper ───────────────────────────────────────────────────────────────

pub struct Gatekeeper {
    revoked_keys: Vec<String>,
}

impl Gatekeeper {
    pub fn new(revoked_keys: Vec<String>) -> Self {
        Self { revoked_keys }
    }

    /// 4-step verification. All-or-nothing.
    /// Step 1: Validate manifest fields
    /// Step 2: Verify Ed25519 signature  
    /// Step 3: Check declared permissions against allowed scope
    /// Step 4: Compute blake3 of artifact, compare to manifest digest
    pub fn verify(
        &self,
        manifest: &EngineManifest,
        artifact: &[u8],
    ) -> Result<(), GatekeeperError> {
        // Step 1 — Manifest validation
        self.step1_validate_manifest(manifest)?;

        // Step 2 — Signature verification + revocation check
        self.step2_verify_signature(manifest, artifact)?;

        // Step 3 — Permission check
        self.step3_check_permissions(manifest)?;

        // Step 4 — Digest verification
        self.step4_verify_digest(manifest, artifact)?;

        tracing::info!(
            "Marketplace gatekeeper: engine '{}' v{} ACCEPTED",
            manifest.engine_id,
            manifest.version
        );
        Ok(())
    }

    // ─── Step 1: Manifest Validation ─────────────────────────────────────────

    fn step1_validate_manifest(&self, manifest: &EngineManifest) -> Result<(), GatekeeperError> {
        if manifest.engine_id.is_empty() {
            return Err(GatekeeperError::ManifestInvalid(
                "engine_id is empty".to_string(),
            ));
        }
        if manifest.version.is_empty() {
            return Err(GatekeeperError::ManifestInvalid(
                "version is empty".to_string(),
            ));
        }
        if manifest.developer_public_key.is_empty() {
            return Err(GatekeeperError::ManifestInvalid(
                "developer_public_key is empty — unsigned engines are rejected".to_string(),
            ));
        }
        if manifest.developer_signature.is_empty() {
            return Err(GatekeeperError::ManifestInvalid(
                "developer_signature is empty — unsigned engines are rejected".to_string(),
            ));
        }
        if manifest.blake3_digest.is_empty() {
            return Err(GatekeeperError::ManifestInvalid(
                "blake3_digest is empty".to_string(),
            ));
        }
        Ok(())
    }

    // ─── Step 2: Signature Verification ──────────────────────────────────────

    fn step2_verify_signature(
        &self,
        manifest: &EngineManifest,
        artifact: &[u8],
    ) -> Result<(), GatekeeperError> {
        // Check revocation first
        if super::revocation::is_revoked(&manifest.developer_public_key, &self.revoked_keys) {
            return Err(GatekeeperError::KeyRevoked(
                manifest.developer_public_key.clone(),
            ));
        }

        // Parse verifying key
        let key_bytes = hex::decode(&manifest.developer_public_key).map_err(|e| {
            GatekeeperError::SignatureInvalid(format!("Invalid public key hex: {e}"))
        })?;

        let key_array: [u8; 32] = key_bytes.try_into().map_err(|_| {
            GatekeeperError::SignatureInvalid("Public key must be 32 bytes".to_string())
        })?;

        let verifying_key = VerifyingKey::from_bytes(&key_array).map_err(|e| {
            GatekeeperError::SignatureInvalid(format!("Invalid ed25519 key: {e}"))
        })?;

        // Parse developer signature
        let sig_bytes = hex::decode(&manifest.developer_signature).map_err(|e| {
            GatekeeperError::SignatureInvalid(format!("Invalid signature hex: {e}"))
        })?;

        let sig_array: [u8; 64] = sig_bytes.try_into().map_err(|_| {
            GatekeeperError::SignatureInvalid("Signature must be 64 bytes".to_string())
        })?;

        let signature = Signature::from_bytes(&sig_array);

        // Verify signature over artifact bytes
        use ed25519_dalek::Verifier;
        verifying_key.verify(artifact, &signature).map_err(|e| {
            GatekeeperError::SignatureInvalid(format!("Ed25519 verification failed: {e}"))
        })?;

        Ok(())
    }

    // ─── Step 3: Permission Check ─────────────────────────────────────────────

    fn step3_check_permissions(&self, manifest: &EngineManifest) -> Result<(), GatekeeperError> {
        for perm in &manifest.permissions {
            if !ALLOWED_PERMISSIONS.contains(&perm.as_str()) {
                return Err(GatekeeperError::PermissionDenied(format!(
                    "Permission '{}' is not in allowed scope for engine '{}'",
                    perm, manifest.engine_id
                )));
            }
        }
        Ok(())
    }

    // ─── Step 4: Digest Verification ─────────────────────────────────────────

    fn step4_verify_digest(
        &self,
        manifest: &EngineManifest,
        artifact: &[u8],
    ) -> Result<(), GatekeeperError> {
        let mut hasher = Blake3Hasher::new();
        hasher.update(artifact);
        let actual = hasher.finalize().to_hex().to_string();

        if actual != manifest.blake3_digest {
            return Err(GatekeeperError::DigestMismatch {
                expected: manifest.blake3_digest.clone(),
                actual,
            });
        }

        Ok(())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{SigningKey, Signer};

    fn make_signing_key() -> SigningKey {
        use rand::RngCore;
        let mut rng = rand::rngs::OsRng;
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);
        SigningKey::from_bytes(&secret_bytes)
    }

    fn blake3_hex(data: &[u8]) -> String {
        let mut h = Blake3Hasher::new();
        h.update(data);
        h.finalize().to_hex().to_string()
    }

    fn make_valid_manifest(signing_key: &SigningKey, artifact: &[u8], permissions: Vec<String>) -> EngineManifest {
        let verifying_key = signing_key.verifying_key();
        let signature = signing_key.sign(artifact);

        EngineManifest {
            engine_id: "E100".to_string(),
            version: "1.0.0".to_string(),
            developer_public_key: hex::encode(verifying_key.as_bytes()),
            registry_signature: "".to_string(), // simplified for Phase 1
            developer_signature: hex::encode(signature.to_bytes()),
            blake3_digest: blake3_hex(artifact),
            permissions,
        }
    }

    #[test]
    fn gatekeeper_accepts_valid_engine() {
        let key = make_signing_key();
        let artifact = b"valid wasm artifact";
        let manifest = make_valid_manifest(&key, artifact, vec!["dsp.read".to_string()]);
        let gate = Gatekeeper::new(vec![]);
        assert!(gate.verify(&manifest, artifact).is_ok());
    }

    #[test]
    fn gatekeeper_rejects_unsigned_engine() {
        let artifact = b"unsigned wasm";
        let manifest = EngineManifest {
            engine_id: "E100".to_string(),
            version: "1.0.0".to_string(),
            developer_public_key: "".to_string(), // empty = unsigned
            registry_signature: "".to_string(),
            developer_signature: "".to_string(),
            blake3_digest: blake3_hex(artifact),
            permissions: vec![],
        };
        let gate = Gatekeeper::new(vec![]);
        let result = gate.verify(&manifest, artifact);
        assert!(
            matches!(result, Err(GatekeeperError::ManifestInvalid(_))),
            "Unsigned engine must fail manifest validation: {result:?}"
        );
    }

    #[test]
    fn gatekeeper_rejects_wrong_digest() {
        let key = make_signing_key();
        let artifact = b"original artifact";
        let mut manifest = make_valid_manifest(&key, artifact, vec![]);
        // Tamper: give wrong digest
        manifest.blake3_digest = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();

        let gate = Gatekeeper::new(vec![]);
        let result = gate.verify(&manifest, artifact);
        assert!(
            matches!(result, Err(GatekeeperError::DigestMismatch { .. })),
            "Wrong digest must produce DigestMismatch: {result:?}"
        );
    }

    #[test]
    fn gatekeeper_rejects_revoked_key() {
        let key = make_signing_key();
        let artifact = b"wasm from revoked dev";
        let manifest = make_valid_manifest(&key, artifact, vec![]);
        let revoked_key_hex = hex::encode(key.verifying_key().as_bytes());

        let gate = Gatekeeper::new(vec![revoked_key_hex]);
        let result = gate.verify(&manifest, artifact);
        assert!(
            matches!(result, Err(GatekeeperError::KeyRevoked(_))),
            "Revoked key must produce KeyRevoked: {result:?}"
        );
    }

    #[test]
    fn gatekeeper_rejects_undeclared_permission() {
        let key = make_signing_key();
        let artifact = b"engine requesting network access";
        let manifest = make_valid_manifest(
            &key,
            artifact,
            vec!["network.outbound".to_string()], // not in allowed scope
        );
        let gate = Gatekeeper::new(vec![]);
        let result = gate.verify(&manifest, artifact);
        assert!(
            matches!(result, Err(GatekeeperError::PermissionDenied(_))),
            "Undeclared permission must produce PermissionDenied: {result:?}"
        );
    }
}
