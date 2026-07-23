//! config.rs — Centralized configuration for m0-daemon

pub const REGISTRY_PATH: &str = "lineos/m0/registry/m0-registry.json";
pub const CHECKSUMS_PATH: &str = "lineos/m0/registry/checksums.json";
pub const POLICIES_PATH: &str = "lineos/m0/config/policies.toml";
pub const AUDIT_LOG_DIR: &str = "lineos/m0/logs/audit";
pub const ASSETS_ROOT: &str = "lineos/m0/assets/wasm";

pub const MAX_FILE_BYTES: u64 = 500 * 1024 * 1024; // 500 MB

/// MAX_STREAM_FILE_BYTES: 12GB headroom for streaming (an 8h 48k/32f stereo file is ~11GB)
pub const MAX_STREAM_FILE_BYTES: u64 = 12 * 1024 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct M0Config {
    pub registry_path: String,
    pub checksums_path: String,
    pub policies_path: String,
    pub audit_log_dir: String,
    pub assets_root: String,
    pub max_concurrent_jobs: usize,
    pub db_path: String,
    pub certs_path: String,
    pub state_path: String,
}

impl M0Config {
    pub fn from_env() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Self {
            registry_path: std::env::var("M0_REGISTRY_PATH")
                .unwrap_or_else(|_| REGISTRY_PATH.to_string()),
            checksums_path: std::env::var("M0_CHECKSUMS_PATH")
                .unwrap_or_else(|_| CHECKSUMS_PATH.to_string()),
            policies_path: std::env::var("M0_POLICIES_PATH")
                .unwrap_or_else(|_| POLICIES_PATH.to_string()),
            audit_log_dir: std::env::var("M0_AUDIT_LOG_DIR")
                .unwrap_or_else(|_| AUDIT_LOG_DIR.to_string()),
            assets_root: std::env::var("M0_ASSETS_ROOT")
                .unwrap_or_else(|_| ASSETS_ROOT.to_string()),
            max_concurrent_jobs: std::env::var("M0_MAX_CONCURRENT_JOBS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| {
                    std::thread::available_parallelism()
                        .map(|n| n.get().saturating_sub(1).max(1))
                        .unwrap_or(1)
                }),
            db_path: std::env::var("M0_DB_PATH")
                .unwrap_or_else(|_| format!("{home}/.creator_os/db")),
            certs_path: std::env::var("M0_CERTS_PATH")
                .unwrap_or_else(|_| format!("{home}/.creator_os/certificates")),
            state_path: std::env::var("M0_STATE_PATH")
                .unwrap_or_else(|_| format!("{home}/.creator_os/state")),
        }
    }
}
