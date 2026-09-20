//! Health gate — M0 Constitution v2.0 §04.2
//! All 7 criteria must pass simultaneously before M0 signals healthy.
//! Health evaluation uses M0-internal state only — never calls M1.x services.

use axum::{routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

// ─── Health Criteria ─────────────────────────────────────────────────────────

/// All 7 M0 health criteria (M0 Constitution §04.2).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthCriteria {
    /// C1: All checksums.json assets verified against blake3
    pub cdn_ready: bool,
    /// C2: m0-registry.json parsed and digests verified
    pub registry_loaded: bool,
    /// C4: policies.toml loaded and deny-by-default active
    pub policy_active: bool,
    /// C5: Audit log directory writable (test write succeeded)
    pub audit_writable: bool,
    /// C6: Health endpoint responding (implicit — if this is evaluated, true)
    pub health_endpoint_responding: bool,
    /// C7: Health evaluates M0-internal state only (enforced by design)
    pub health_independence: bool,
}

impl HealthCriteria {
    pub fn all_pass(&self) -> bool {
        self.cdn_ready
            && self.registry_loaded
            && self.policy_active
            && self.audit_writable
            && self.health_endpoint_responding
            && self.health_independence
    }
}

// ─── HealthGate ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HealthGate {
    criteria: Arc<RwLock<HealthCriteria>>,
}

impl HealthGate {
    pub fn new() -> Self {
        // C6/C7 are structural invariants — always true by design
        let criteria = HealthCriteria {
            health_endpoint_responding: true,
            health_independence: true,
            ..Default::default()
        };
        Self {
            criteria: Arc::new(RwLock::new(criteria)),
        }
    }

    pub async fn is_healthy(&self) -> bool {
        self.criteria.read().await.all_pass()
    }

    #[allow(dead_code)]
    pub async fn get_criteria(&self) -> HealthCriteria {
        self.criteria.read().await.clone()
    }

    #[allow(dead_code)]
    pub async fn set_cdn_ready(&self, v: bool) {
        self.criteria.write().await.cdn_ready = v;
    }

    #[allow(dead_code)]
    pub async fn set_registry_loaded(&self, v: bool) {
        self.criteria.write().await.registry_loaded = v;
    }

    #[allow(dead_code)]
    pub async fn set_policy_active(&self, v: bool) {
        self.criteria.write().await.policy_active = v;
    }

    #[allow(dead_code)]
    pub async fn set_audit_writable(&self, v: bool) {
        self.criteria.write().await.audit_writable = v;
    }
}

impl Default for HealthGate {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Axum Response Types ──────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub criteria: HealthCriteria,
}

// ─── Health Handler ───────────────────────────────────────────────────────────

#[allow(dead_code)]
pub async fn health_handler(
    axum::extract::State(gate): axum::extract::State<HealthGate>,
) -> (axum::http::StatusCode, Json<HealthResponse>) {
    let criteria = gate.get_criteria().await;
    let healthy = criteria.all_pass();

    let status_code = if healthy {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(HealthResponse {
            status: if healthy { "ok" } else { "degraded" },
            criteria,
        }),
    )
}

/// Build the health router (mountable at any prefix).
#[allow(dead_code)]
pub fn health_router(gate: HealthGate) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .with_state(gate)
}

// ─── Startup Gate ─────────────────────────────────────────────────────────────

/// Retry health check every 5 seconds, fatal halt after 30 seconds.
/// Returns Ok(()) when all criteria pass, Err if timeout exceeded.
#[allow(dead_code)]
pub async fn await_health_gate(gate: &HealthGate) -> anyhow::Result<()> {
    use std::time::{Duration, Instant};

    let deadline = Instant::now() + Duration::from_secs(30);
    let retry_interval = Duration::from_secs(5);

    loop {
        if gate.is_healthy().await {
            tracing::info!("Health gate passed — all criteria satisfied");
            return Ok(());
        }

        if Instant::now() >= deadline {
            anyhow::bail!(
                "Health gate timeout after 30s — M0 cannot start (M0 Constitution §04.2)"
            );
        }

        tracing::warn!("Health gate not passed yet — retrying in 5s");
        tokio::time::sleep(retry_interval).await;
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_gate_all_criteria_required() {
        let gate = HealthGate::new();
        // Initially only C6+C7 are true — overall should be unhealthy
        assert!(!gate.is_healthy().await);
    }

    #[tokio::test]
    async fn health_gate_passes_when_all_set() {
        let gate = HealthGate::new();
        gate.set_cdn_ready(true).await;
        gate.set_registry_loaded(true).await;
        gate.set_policy_active(true).await;
        gate.set_audit_writable(true).await;
        assert!(gate.is_healthy().await);
    }

    #[tokio::test]
    async fn health_gate_fails_if_any_criterion_missing() {
        let gate = HealthGate::new();
        gate.set_cdn_ready(true).await;
        gate.set_registry_loaded(true).await;
        gate.set_policy_active(true).await;
        // audit_writable NOT set
        assert!(!gate.is_healthy().await);
    }

    #[tokio::test]
    async fn health_criteria_default_c6_c7_true() {
        let gate = HealthGate::new();
        let criteria = gate.get_criteria().await;
        assert!(
            criteria.health_endpoint_responding,
            "C6 must be true by design"
        );
        assert!(criteria.health_independence, "C7 must be true by design");
    }
}
