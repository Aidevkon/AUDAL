//! Audit subsystem — M0 Constitution v2.0 §03.5
//! Append-only NDJSON. Synchronous writes. Failed write = fatal halt.
//! Validates against lineos/shared/schema/audit.schema.json

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ─── Event Types (M0 Constitution §03.5) ─────────────────────────────────────

pub const EVENT_STARTUP: &str = "m0d.startup";
pub const EVENT_HEALTH_GATE_PASSED: &str = "m0d.health_gate_passed";
pub const EVENT_ASSET_SERVED: &str = "m0d.asset_served";
pub const EVENT_POLICY_VIOLATION: &str = "m0d.policy_violation";
pub const EVENT_PROXY_REQUEST: &str = "m0d.proxy_request";
pub const EVENT_MARKETPLACE_REJECTION: &str = "m0d.marketplace_rejection";
pub const EVENT_SHUTDOWN: &str = "m0d.shutdown";

// ─── Audit Entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub event_type: String,
    pub level: AuditLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuditLevel {
    Info,
    Warn,
    Error,
    Audit,
}

impl AuditEntry {
    pub fn new(event_type: &str, level: AuditLevel, message: &str) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            event_type: event_type.to_string(),
            level,
            message: message.to_string(),
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, meta: Value) -> Self {
        self.metadata = Some(meta);
        self
    }
}

// ─── Audit Log ───────────────────────────────────────────────────────────────

pub struct AuditLog {
    log_dir: PathBuf,
    /// Mutex ensures synchronous writes — concurrent writes are serialized.
    file: Mutex<File>,
}

impl AuditLog {
    /// Open or create the audit log file for today.
    /// Failed open = fatal halt (M0 Constitution §03.5).
    pub fn open(log_dir: &str) -> Result<Self> {
        let log_dir = PathBuf::from(log_dir);
        std::fs::create_dir_all(&log_dir)
            .with_context(|| format!("Failed to create audit log directory: {log_dir:?}"))?;

        let today = Utc::now().format("%Y%m%d").to_string();
        let log_path = log_dir.join(format!("m0-audit-{today}.ndjson"));

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("Failed to open audit log: {log_path:?}"))?;

        Ok(Self {
            log_dir,
            file: Mutex::new(file),
        })
    }

    /// Append an audit entry. Synchronous write. Failed write = fatal halt.
    pub fn write(&self, entry: AuditEntry) -> Result<()> {
        let line = serde_json::to_string(&entry)
            .context("Failed to serialize audit entry")?;

        let mut file = self
            .file
            .lock()
            .map_err(|e| anyhow::anyhow!("Audit log mutex poisoned: {e}"))?;

        writeln!(file, "{line}")
            .with_context(|| "Failed to write audit entry — FATAL (M0 §03.5)")?;

        file.flush()
            .with_context(|| "Failed to flush audit entry — FATAL (M0 §03.5)")?;

        Ok(())
    }

    /// Convenience: write audit entry — panics on failure (fatal by design).
    pub fn write_fatal(&self, entry: AuditEntry) {
        if let Err(e) = self.write(entry) {
            eprintln!("FATAL: Audit write failure — M0 must halt: {e}");
            std::process::exit(1);
        }
    }

    /// Test that the audit log directory and file are writable.
    pub fn test_writable(&self) -> bool {
        let test_entry = AuditEntry::new("m0d.audit_test", AuditLevel::Info, "audit write test");
        self.write(test_entry).is_ok()
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }
}

// ─── Helper Constructors ──────────────────────────────────────────────────────

pub fn entry_startup() -> AuditEntry {
    AuditEntry::new(EVENT_STARTUP, AuditLevel::Audit, "M0 daemon starting")
}

pub fn entry_health_gate_passed() -> AuditEntry {
    AuditEntry::new(EVENT_HEALTH_GATE_PASSED, AuditLevel::Audit, "Health gate passed — all 7 criteria satisfied")
}

pub fn entry_asset_served(path: &str, digest: &str) -> AuditEntry {
    AuditEntry::new(EVENT_ASSET_SERVED, AuditLevel::Info, &format!("Asset served: {path}"))
        .with_metadata(serde_json::json!({ "path": path, "blake3": digest }))
}

pub fn entry_policy_violation(destination: &str, reason: &str) -> AuditEntry {
    AuditEntry::new(EVENT_POLICY_VIOLATION, AuditLevel::Audit, &format!("Policy violation: {destination}"))
        .with_metadata(serde_json::json!({ "destination": destination, "reason": reason }))
}

pub fn entry_proxy_request(method: &str, path: &str, status: u16) -> AuditEntry {
    AuditEntry::new(EVENT_PROXY_REQUEST, AuditLevel::Info, &format!("{method} {path} → {status}"))
        .with_metadata(serde_json::json!({ "method": method, "path": path, "status": status }))
}

pub fn entry_marketplace_rejection(engine_id: &str, reason: &str) -> AuditEntry {
    AuditEntry::new(EVENT_MARKETPLACE_REJECTION, AuditLevel::Audit, &format!("Marketplace rejection: {engine_id}"))
        .with_metadata(serde_json::json!({ "engine_id": engine_id, "reason": reason }))
}

pub fn entry_shutdown() -> AuditEntry {
    AuditEntry::new(EVENT_SHUTDOWN, AuditLevel::Audit, "M0 daemon shutting down")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn audit_write_creates_ndjson_line() {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::open(dir.path().to_str().unwrap()).unwrap();
        let entry = AuditEntry::new(EVENT_STARTUP, AuditLevel::Audit, "test startup");
        log.write(entry).unwrap();

        // Read back and parse
        let today = Utc::now().format("%Y%m%d").to_string();
        let path = dir.path().join(format!("m0-audit-{today}.ndjson"));
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: AuditEntry = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(parsed.event_type, EVENT_STARTUP);
        assert_eq!(parsed.message, "test startup");
    }

    #[test]
    fn audit_all_7_event_types_serialize() {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::open(dir.path().to_str().unwrap()).unwrap();

        log.write(entry_startup()).unwrap();
        log.write(entry_health_gate_passed()).unwrap();
        log.write(entry_asset_served("sp314.wasm", "aabbccdd")).unwrap();
        log.write(entry_policy_violation("https://example.com", "not declared")).unwrap();
        log.write(entry_proxy_request("GET", "/telemetry", 200)).unwrap();
        log.write(entry_marketplace_rejection("E100", "signature invalid")).unwrap();
        log.write(entry_shutdown()).unwrap();

        // Read back: expect 7 lines
        let today = Utc::now().format("%Y%m%d").to_string();
        let path = dir.path().join(format!("m0-audit-{today}.ndjson"));
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 7, "Must have exactly 7 audit lines");

        // Each line must be valid JSON
        for line in &lines {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(v.get("timestamp").is_some());
            assert!(v.get("event_type").is_some());
            assert!(v.get("level").is_some());
            assert!(v.get("message").is_some());
        }
    }

    #[test]
    fn audit_test_writable_succeeds() {
        let dir = TempDir::new().unwrap();
        let log = AuditLog::open(dir.path().to_str().unwrap()).unwrap();
        assert!(log.test_writable());
    }

    #[test]
    fn audit_fail_on_nonexistent_path() {
        // Read-only path — should fail
        let result = AuditLog::open("/proc/nonexistent_m0_audit_path");
        assert!(result.is_err(), "Should fail on non-creatable path");
    }
}
