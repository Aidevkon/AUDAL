//! Per-install Ed25519 identity (§Σ/Ψ6).
//!
//! Authority: northstar-v2 §Σ/Ψ6
//!
//! Provides a persistent per-installation signing keypair stored at `~/.creator_os/identity/signing.key`
//! or overridden via the `M0_IDENTITY_PATH` environment variable.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Identity {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
    /// Deterministic key ID: "m0-" + first 16 hex chars of SHA-256(verifying_key_bytes)
    pub key_id: String,
}

impl Identity {
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.verifying_key.as_bytes())
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), ed25519_dalek::SignatureError> {
        self.verifying_key.verify(message, signature)
    }
}

/// Resolves the identity directory.
/// 1. Checks `M0_IDENTITY_PATH` env var.
/// 2. Defaults to `~/.creator_os/identity`.
pub fn resolve_identity_dir() -> PathBuf {
    if let Ok(env_path) = std::env::var("M0_IDENTITY_PATH") {
        if !env_path.trim().is_empty() {
            return PathBuf::from(env_path.trim());
        }
    }
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    home.join(".creator_os").join("identity")
}

/// Loads an existing `Identity` from `dir/signing.key` or atomically generates and saves a new 32-byte seed.
///
/// On Unix, permissions are restricted to 0o600.
pub fn load_or_generate(dir: &Path) -> Result<Identity, std::io::Error> {
    let key_path = dir.join("signing.key");
    let tmp_path = dir.join("signing.key.tmp");

    let seed_bytes = if key_path.exists() {
        let bytes = std::fs::read(&key_path)?;
        if bytes.len() != 32 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("signing.key at {:?} must be exactly 32 bytes, found {}", key_path, bytes.len()),
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        arr
    } else {
        std::fs::create_dir_all(dir)?;
        let mut seed = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut seed);

        std::fs::write(&tmp_path, &seed)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600));
        }

        std::fs::rename(&tmp_path, &key_path)?;
        seed
    };

    let signing_key = SigningKey::from_bytes(&seed_bytes);
    let verifying_key = signing_key.verifying_key();
    let hash = Sha256::digest(verifying_key.as_bytes());
    let key_id = format!("m0-{}", hex::encode(&hash[..8]));

    Ok(Identity {
        signing_key,
        verifying_key,
        key_id,
    })
}

/// Helper that resolves the default directory and loads or generates the `Identity`.
pub fn load_or_generate_default() -> Result<Identity, std::io::Error> {
    let dir = resolve_identity_dir();
    load_or_generate(&dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_identity_load_or_generate_roundtrip() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("sub");

        let id1 = load_or_generate(&dir).expect("first load_or_generate failed");
        assert!(id1.key_id.starts_with("m0-"));
        assert_eq!(id1.key_id.len(), 3 + 16);

        // Load again — must yield identical key
        let id2 = load_or_generate(&dir).expect("second load_or_generate failed");
        assert_eq!(id1.key_id, id2.key_id);
        assert_eq!(id1.verifying_key.as_bytes(), id2.verifying_key.as_bytes());
    }

    #[test]
    fn test_identity_env_override() {
        let tmp = tempdir().unwrap();
        std::env::set_var("M0_IDENTITY_PATH", tmp.path());
        let resolved = resolve_identity_dir();
        assert_eq!(resolved, tmp.path());
        std::env::remove_var("M0_IDENTITY_PATH");
    }

    #[test]
    fn test_identity_sign_verify_ok() {
        let tmp = tempdir().unwrap();
        let identity = load_or_generate(tmp.path()).unwrap();
        let msg = b"canonical_certificate_payload_12345";
        let sig = identity.sign(msg);
        assert!(identity.verify(msg, &sig).is_ok());
    }

    #[test]
    fn test_identity_distinct_tempdirs() {
        let tmp1 = tempdir().unwrap();
        let tmp2 = tempdir().unwrap();
        let id1 = load_or_generate(tmp1.path()).unwrap();
        let id2 = load_or_generate(tmp2.path()).unwrap();
        assert_ne!(id1.key_id, id2.key_id);
        assert_ne!(id1.verifying_key.as_bytes(), id2.verifying_key.as_bytes());
    }

    #[test]
    fn test_identity_tampered_message_fails() {
        let tmp = tempdir().unwrap();
        let identity = load_or_generate(tmp.path()).unwrap();
        let original_msg = b"original_payload";
        let tampered_msg = b"tampered_payload";
        let sig = identity.sign(original_msg);
        assert!(identity.verify(tampered_msg, &sig).is_err());
    }
}
