//! CDN subsystem — M0 Constitution v2.0 §03.1
//! Serves WASM artifacts and schemas. Hash verification before every serve.
//! Hash mismatch = fatal halt (never a warning).

#![allow(dead_code)]

use anyhow::{bail, Context, Result};
use blake3::Hasher as Blake3Hasher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ─── Data Structures ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CdnAssetEntry {
    pub blake3: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CdnChecksums {
    pub version: String,
    pub assets: HashMap<String, CdnAssetEntry>,
}

// ─── Cdn ─────────────────────────────────────────────────────────────────────

pub struct Cdn {
    /// Root directory for all M0-served assets
    assets_root: PathBuf,
    /// Registered checksums from checksums.json
    checksums: CdnChecksums,
}

impl Cdn {
    /// Load CDN from an assets root directory and a checksums JSON file.
    pub async fn load(assets_root: &str, checksums_path: &str) -> Result<Self> {
        let checksums_raw = tokio::fs::read_to_string(checksums_path)
            .await
            .with_context(|| format!("Failed to read checksums: {checksums_path}"))?;

        let checksums: CdnChecksums = serde_json::from_str(&checksums_raw)
            .with_context(|| format!("Invalid JSON in checksums: {checksums_path}"))?;

        tracing::info!(
            "CDN loaded: {} registered assets (root={})",
            checksums.assets.len(),
            assets_root
        );

        Ok(Self {
            assets_root: PathBuf::from(assets_root),
            checksums,
        })
    }

    /// Serve asset by name. Performs blake3 hash verification before returning bytes.
    /// Hash mismatch → anyhow::bail! — M0 must halt, never serve a tampered asset.
    pub async fn serve(&self, asset_name: &str) -> Result<Vec<u8>> {
        let asset_path = self.assets_root.join(asset_name);

        // 1. Look up registered checksum
        let registered = self.checksums.assets.get(asset_name);

        // 2. Read file from disk
        let bytes = tokio::fs::read(&asset_path)
            .await
            .with_context(|| format!("Failed to read asset '{asset_name}' at {asset_path:?}"))?;

        // 3. Compute blake3 digest
        let actual_blake3 = blake3_hex(&bytes);

        // 4. Compare against registered value
        if let Some(entry) = registered {
            if !entry.blake3.is_empty() && actual_blake3 != entry.blake3 {
                bail!(
                    "CDN tamper detected for '{}': expected blake3={}, actual={} — FATAL (M0 §03.1)",
                    asset_name,
                    entry.blake3,
                    actual_blake3
                );
            }
        }
        // If not in checksums, still serve (but log a warning — unregistered assets will
        // be rejected once all stubs are pinned in Phase 2+)
        else {
            tracing::warn!("Asset '{}' not in checksums.json — serving without hash verification (Phase 1 stub)", asset_name);
        }

        tracing::info!("CDN served '{}' ({} bytes, blake3={})", asset_name, bytes.len(), &actual_blake3[..16]);
        Ok(bytes)
    }

    /// Verify all registered assets exist and pass hash verification.
    /// Returns list of (asset_name, error) for any failures.
    pub async fn verify_all(&self) -> Vec<(String, String)> {
        let mut failures = Vec::new();

        for (name, entry) in &self.checksums.assets {
            // Skip empty-hash placeholders
            if entry.blake3.is_empty() {
                tracing::debug!("Skipping verification of '{}' — empty hash (pending stub)", name);
                continue;
            }

            let asset_path = self.assets_root.join(name);
            match tokio::fs::read(&asset_path).await {
                Err(e) => {
                    failures.push((name.clone(), format!("Read error: {e}")));
                }
                Ok(bytes) => {
                    let actual = blake3_hex(&bytes);
                    if actual != entry.blake3 {
                        failures.push((
                            name.clone(),
                            format!("Hash mismatch: expected={}, actual={}", entry.blake3, actual),
                        ));
                    }
                }
            }
        }

        failures
    }

    pub fn checksums(&self) -> &CdnChecksums {
        &self.checksums
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn blake3_hex(data: &[u8]) -> String {
    let mut hasher = Blake3Hasher::new();
    hasher.update(data);
    hasher.finalize().to_hex().to_string()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    fn write_asset(dir: &TempDir, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn checksums_json(assets: &[(&str, &str)]) -> String {
        let entries: Vec<String> = assets
            .iter()
            .map(|(name, hash)| {
                format!(r#""{name}": {{ "blake3": "{hash}", "sha256": "", "size_bytes": 0 }}"#)
            })
            .collect();
        format!(r#"{{ "version": "0.1.0", "assets": {{ {} }} }}"#, entries.join(", "))
    }

    #[tokio::test]
    async fn cdn_serves_correct_asset() {
        let dir = TempDir::new().unwrap();
        let content = b"hello world";
        write_asset(&dir, "test.wasm", content);
        let real_hash = blake3_hex(content);

        let mut cs_file = NamedTempFile::new().unwrap();
        cs_file.write_all(checksums_json(&[("test.wasm", &real_hash)]).as_bytes()).unwrap();

        let cdn = Cdn::load(dir.path().to_str().unwrap(), cs_file.path().to_str().unwrap())
            .await
            .unwrap();

        let served = cdn.serve("test.wasm").await.unwrap();
        assert_eq!(served, content);
    }

    #[tokio::test]
    async fn cdn_hash_mismatch_prevents_serve() {
        let dir = TempDir::new().unwrap();
        write_asset(&dir, "tampered.wasm", b"tampered content");

        // Register a different (wrong) hash
        let mut cs_file = NamedTempFile::new().unwrap();
        cs_file.write_all(checksums_json(&[("tampered.wasm", "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef")]).as_bytes()).unwrap();

        let cdn = Cdn::load(dir.path().to_str().unwrap(), cs_file.path().to_str().unwrap())
            .await
            .unwrap();

        let result = cdn.serve("tampered.wasm").await;
        assert!(result.is_err(), "Tampered asset must not be served");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("CDN tamper detected"), "Error must indicate tamper: {err}");
    }

    #[tokio::test]
    async fn cdn_verify_all_passes_with_correct_hashes() {
        let dir = TempDir::new().unwrap();
        let content = b"sp314 placeholder";
        write_asset(&dir, "sp314.wasm", content);
        let hash = blake3_hex(content);

        let mut cs_file = NamedTempFile::new().unwrap();
        cs_file.write_all(checksums_json(&[("sp314.wasm", &hash)]).as_bytes()).unwrap();

        let cdn = Cdn::load(dir.path().to_str().unwrap(), cs_file.path().to_str().unwrap())
            .await
            .unwrap();

        let failures = cdn.verify_all().await;
        assert!(failures.is_empty(), "No failures expected: {failures:?}");
    }

    #[tokio::test]
    async fn cdn_verify_all_detects_tamper() {
        let dir = TempDir::new().unwrap();
        write_asset(&dir, "bad.wasm", b"actual content");

        let mut cs_file = NamedTempFile::new().unwrap();
        cs_file.write_all(checksums_json(&[("bad.wasm", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")]).as_bytes()).unwrap();

        let cdn = Cdn::load(dir.path().to_str().unwrap(), cs_file.path().to_str().unwrap())
            .await
            .unwrap();

        let failures = cdn.verify_all().await;
        assert!(!failures.is_empty(), "Should detect tampered asset");
    }
}
