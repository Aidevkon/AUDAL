//! Policy enforcement — M0 Constitution v2.0 §03.6
//! Deny-by-default outbound. Every blocked request → audit log.
//! Undeclared destination → 403 + audit entry. No retry.

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── Policy Data Structures ───────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct OutboundPolicy {
    #[serde(rename = "allow", default)]
    pub allowed: Vec<AllowedDestination>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AllowedDestination {
    pub destination: String,
    pub module: String,
    pub requires_user_consent: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct InboundPolicy {}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PoliciesConfig {
    #[serde(default)]
    pub outbound: OutboundPolicy,
    #[serde(default)]
    pub inbound: InboundPolicy,
}

// ─── PolicyEngine ─────────────────────────────────────────────────────────────

pub struct PolicyEngine {
    config: PoliciesConfig,
}

/// Result of a policy check.
#[derive(Debug, PartialEq)]
pub enum PolicyDecision {
    /// Request is allowed to proceed.
    Allow,
    /// Request is blocked — caller must return 403 and write audit entry.
    Block(String),
}

impl PolicyEngine {
    /// Load policies.toml from disk. Parse failure = fatal halt.
    pub fn load(policies_path: &str) -> Result<Self> {
        let path = Path::new(policies_path);
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read policies: {policies_path}"))?;

        let config: PoliciesConfig = toml::from_str(&raw)
            .with_context(|| format!("Invalid TOML in policies: {policies_path}"))?;

        tracing::info!(
            "Policy engine loaded: {} allowed destinations (deny-by-default)",
            config.outbound.allowed.len()
        );

        Ok(Self { config })
    }

    /// Check whether an outbound request to `destination` is allowed.
    /// Deny-by-default: unless explicitly listed in [outbound.allow], block it.
    pub fn check_outbound(&self, destination: &str) -> PolicyDecision {
        for allowed in &self.config.outbound.allowed {
            if destination.starts_with(&allowed.destination) {
                tracing::debug!(
                    "Outbound allowed: {} (module={})",
                    destination,
                    allowed.module
                );
                return PolicyDecision::Allow;
            }
        }

        let reason = format!(
            "Destination '{}' not declared in outbound policy — deny-by-default (M0 §03.6)",
            destination
        );
        PolicyDecision::Block(reason)
    }

    /// Check whether an inbound path is allowed.
    /// Currently all inbound routes via Caddy — anything reaching M0 directly is a violation.
    pub fn check_inbound(&self, path: &str) -> PolicyDecision {
        // All inbound traffic must arrive via Caddy proxy on port 7400.
        // Direct-to-M0 requests on health port (7401) are fine (health check).
        // Any other direct-to-M0 access is a policy violation.
        let _ = path; // inbound policy expanded in Phase 2
        PolicyDecision::Allow
    }

    /// Returns all currently allowed destinations.
    pub fn allowed_destinations(&self) -> &[AllowedDestination] {
        &self.config.outbound.allowed
    }

    pub fn config(&self) -> &PoliciesConfig {
        &self.config
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_policy(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    const EMPTY_POLICY: &str = r#"
[outbound]
[inbound]
"#;

    const ALLOW_POLICY: &str = r#"
[outbound]
[[outbound.allow]]
destination = "https://api.creatorcloud.io"
module = "m1.6-sync"
requires_user_consent = true

[inbound]
"#;

    #[test]
    fn policy_blocks_undeclared_destination() {
        let f = write_policy(EMPTY_POLICY);
        let engine = PolicyEngine::load(f.path().to_str().unwrap()).unwrap();

        let result = engine.check_outbound("https://example.com/api");
        match result {
            PolicyDecision::Block(reason) => {
                assert!(
                    reason.contains("deny-by-default"),
                    "Reason must mention deny-by-default: {reason}"
                );
            }
            PolicyDecision::Allow => panic!("Undeclared destination must be blocked"),
        }
    }

    #[test]
    fn policy_allows_declared_destination() {
        let f = write_policy(ALLOW_POLICY);
        let engine = PolicyEngine::load(f.path().to_str().unwrap()).unwrap();

        let result = engine.check_outbound("https://api.creatorcloud.io/sync");
        assert_eq!(result, PolicyDecision::Allow);
    }

    #[test]
    fn policy_deny_by_default_with_no_allow_rules() {
        let f = write_policy(EMPTY_POLICY);
        let engine = PolicyEngine::load(f.path().to_str().unwrap()).unwrap();

        assert_eq!(engine.allowed_destinations().len(), 0);
        let result = engine.check_outbound("https://any.destination.io");
        assert!(matches!(result, PolicyDecision::Block(_)));
    }

    #[test]
    fn policy_loaded_from_actual_config() {
        // Verify the actual policies.toml in the repo loads clean
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../config/policies.toml");
        if std::path::Path::new(path).exists() {
            let engine = PolicyEngine::load(path).unwrap();
            // Actual config has all outbound commented → 0 allowed
            assert_eq!(engine.allowed_destinations().len(), 0);
            // Everything blocked
            assert!(matches!(
                engine.check_outbound("https://api.creatorcloud.io"),
                PolicyDecision::Block(_)
            ));
        }
    }
}
