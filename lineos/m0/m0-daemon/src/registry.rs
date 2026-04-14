//! Registry subsystem — M0 Constitution v2.0 §03.4
//! Loads m0-registry.json and checksums.json.
//! Hash mismatch → fatal error (anyhow::bail!). Never silently pass.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Data Structures ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WasmModuleEntry {
    pub blake3: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub status: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EngineEntry {
    pub version: String,
    pub blake3: String,
    pub sha256: String,
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct M0RegistryJson {
    pub version: String,
    pub wasm_modules: HashMap<String, WasmModuleEntry>,
    pub engines: HashMap<String, EngineEntry>,
    pub marketplace_engines: HashMap<String, serde_json::Value>,
    pub revoked_keys: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChecksumsJson {
    pub version: String,
    pub assets: HashMap<String, AssetChecksum>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AssetChecksum {
    pub blake3: String,
    pub sha256: String,
    pub size_bytes: u64,
}

// ─── Registry ────────────────────────────────────────────────────────────────

pub struct Registry {
    registry: M0RegistryJson,
    checksums: ChecksumsJson,
}

impl Registry {
    /// Load and parse registry + checksums from disk.
    /// Any parse error is fatal — M0 will not start with a corrupt registry.
    pub async fn load(registry_path: &str, checksums_path: &str) -> Result<Self> {
        let registry_raw = tokio::fs::read_to_string(registry_path)
            .await
            .with_context(|| format!("Failed to read registry: {registry_path}"))?;

        let checksums_raw = tokio::fs::read_to_string(checksums_path)
            .await
            .with_context(|| format!("Failed to read checksums: {checksums_path}"))?;

        let registry: M0RegistryJson = serde_json::from_str(&registry_raw)
            .with_context(|| format!("Invalid JSON in registry: {registry_path}"))?;

        let checksums: ChecksumsJson = serde_json::from_str(&checksums_raw)
            .with_context(|| format!("Invalid JSON in checksums: {checksums_path}"))?;

        tracing::info!(
            "Registry loaded: {} wasm_modules, {} engines, {} revoked_keys",
            registry.wasm_modules.len(),
            registry.engines.len(),
            registry.revoked_keys.len()
        );

        Ok(Self {
            registry,
            checksums,
        })
    }

    /// Verify that an asset's actual blake3 hash matches the registered value.
    /// Mismatch → anyhow::bail! — NEVER silently pass.
    /// Empty registered hash (pending WASM stub) → skip verification.
    pub fn verify_asset(&self, asset_name: &str, actual_hash: &[u8]) -> Result<()> {
        // Look up in checksums first, then wasm_modules
        let expected = if let Some(entry) = self.checksums.assets.get(asset_name) {
            entry.blake3.clone()
        } else if let Some(entry) = self.registry.wasm_modules.get(asset_name) {
            entry.blake3.clone()
        } else {
            // Asset not registered at all — fatal
            bail!(
                "Asset '{}' not found in registry — unregistered assets are forbidden (M0 §03.4)",
                asset_name
            );
        };

        // Empty hash = pending stub — skip hash verification (Phase 1 stubs)
        if expected.is_empty() {
            tracing::debug!("Asset '{}' has empty registered hash (pending stub) — skipping verification", asset_name);
            return Ok(());
        }

        let actual_hex = hex::encode(actual_hash);
        if actual_hex != expected {
            bail!(
                "Hash mismatch for asset '{}': expected blake3={}, actual={} — FATAL (M0 §03.4)",
                asset_name,
                expected,
                actual_hex
            );
        }

        tracing::debug!("Asset '{}' verified OK (blake3={})", asset_name, &actual_hex[..16]);
        Ok(())
    }

    /// Returns the list of revoked developer keys.
    pub fn revoked_keys(&self) -> &[String] {
        &self.registry.revoked_keys
    }

    /// Returns a reference to the parsed registry data.
    pub fn registry_data(&self) -> &M0RegistryJson {
        &self.registry
    }

    /// Returns a reference to the checksums data.
    pub fn checksums_data(&self) -> &ChecksumsJson {
        &self.checksums
    }

    /// Returns true if the given key is in the revoked_keys list.
    pub fn is_key_revoked(&self, key: &str) -> bool {
        self.registry.revoked_keys.iter().any(|k| k == key)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_json(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    fn make_registry_json(revoked: &[&str]) -> String {
        let revoked_json: Vec<String> = revoked.iter().map(|k| format!("\"{}\"", k)).collect();
        format!(
            r#"{{
              "version": "0.1.0",
              "wasm_modules": {{
                "test.wasm": {{ "blake3": "aabbccdd", "sha256": "", "size_bytes": 0, "status": "pending" }},
                "empty_hash.wasm": {{ "blake3": "", "sha256": "", "size_bytes": 0, "status": "pending" }}
              }},
              "engines": {{}},
              "marketplace_engines": {{}},
              "revoked_keys": [{}]
            }}"#,
            revoked_json.join(",")
        )
    }

    fn make_checksums_json(assets: &[(&str, &str)]) -> String {
        let entries: Vec<String> = assets
            .iter()
            .map(|(name, hash)| {
                format!(
                    r#""{name}": {{ "blake3": "{hash}", "sha256": "", "size_bytes": 0 }}"#
                )
            })
            .collect();
        format!(r#"{{ "version": "0.1.0", "assets": {{ {} }} }}"#, entries.join(", "))
    }

    #[tokio::test]
    async fn registry_hash_mismatch_is_fatal() {
        // Registered hash is "aabbccdd", we feed it a different hash → must bail!
        let reg_file = write_temp_json(&make_registry_json(&[]));
        let cs_file = write_temp_json(&make_checksums_json(&[("test.wasm", "aabbccdd")]));

        let registry = Registry::load(
            reg_file.path().to_str().unwrap(),
            cs_file.path().to_str().unwrap(),
        )
        .await
        .expect("load should succeed");

        let wrong_hash = hex::decode("deadbeef").unwrap();
        let result = registry.verify_asset("test.wasm", &wrong_hash);

        assert!(result.is_err(), "Hash mismatch must return Err, never silently pass");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Hash mismatch"),
            "Error must contain 'Hash mismatch', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn registry_correct_hash_passes() {
        let expected_hash = "aabbccdd00112233445566778899aabb";
        let reg_file = write_temp_json(&make_registry_json(&[]));
        let cs_file = write_temp_json(&make_checksums_json(&[("good.wasm", expected_hash)]));

        let registry = Registry::load(
            reg_file.path().to_str().unwrap(),
            cs_file.path().to_str().unwrap(),
        )
        .await
        .expect("load should succeed");

        let actual = hex::decode(expected_hash).unwrap();
        let result = registry.verify_asset("good.wasm", &actual);
        assert!(result.is_ok(), "Correct hash must pass verification");
    }

    #[tokio::test]
    async fn registry_unregistered_asset_is_fatal() {
        let reg_file = write_temp_json(&make_registry_json(&[]));
        let cs_file = write_temp_json(&make_checksums_json(&[]));

        let registry = Registry::load(
            reg_file.path().to_str().unwrap(),
            cs_file.path().to_str().unwrap(),
        )
        .await
        .expect("load should succeed");

        let result = registry.verify_asset("unknown.wasm", &[0xde, 0xad]);
        assert!(result.is_err(), "Unregistered asset must be fatal");
    }

    #[tokio::test]
    async fn registry_pending_stub_skips_verification() {
        // empty_hash.wasm has blake3="" — pending status, must skip hash check
        let reg_file = write_temp_json(&make_registry_json(&[]));
        let cs_file = write_temp_json(&make_checksums_json(&[]));

        let registry = Registry::load(
            reg_file.path().to_str().unwrap(),
            cs_file.path().to_str().unwrap(),
        )
        .await
        .expect("load should succeed");

        // empty_hash.wasm is in wasm_modules with empty blake3 → should pass
        let result = registry.verify_asset("empty_hash.wasm", &[0x00, 0x01, 0x02]);
        assert!(result.is_ok(), "Pending stub with empty hash must skip verification");
    }

    #[tokio::test]
    async fn registry_revoked_keys_accessible() {
        let reg_file = write_temp_json(&make_registry_json(&["key_abc", "key_xyz"]));
        let cs_file = write_temp_json(&make_checksums_json(&[]));

        let registry = Registry::load(
            reg_file.path().to_str().unwrap(),
            cs_file.path().to_str().unwrap(),
        )
        .await
        .expect("load should succeed");

        assert_eq!(registry.revoked_keys().len(), 2);
        assert!(registry.is_key_revoked("key_abc"));
        assert!(registry.is_key_revoked("key_xyz"));
        assert!(!registry.is_key_revoked("key_valid"));
    }

    #[tokio::test]
    async fn registry_load_fails_on_missing_file() {
        let result = Registry::load("/nonexistent/registry.json", "/nonexistent/checksums.json").await;
        assert!(result.is_err(), "Missing file must return Err");
    }
}
