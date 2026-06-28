//! m0d — LineOS Local Mirror Daemon
//! M0 Constitution v2.0
//! Authority: LineOS Constitution v2.0 · Creator OS Constitution v2.6
//! Startup order: registry → hash verify → policy → audit → caddy → health gate
//!
//! Phase 6: Added mastering API router on port 7400 (Caddy proxy target).
//! New endpoints: POST /master, GET /blob/:id, POST /export
//! Existing:      GET /health (port 7401)

pub mod agents;
pub mod app_state;
pub mod audit;
pub mod blob_store;
mod cdn;
pub mod db;
pub mod domain;
pub mod dsp;
pub mod handlers;
mod health;
pub mod jini;
mod marketplace;
mod policy;
mod realtime_bridge;
mod registry;

use anyhow::Result;
use app_state::AppState;
use health::HealthGate;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, RequestId, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

// Environment variable defaults
#[allow(dead_code)]
const REGISTRY_PATH: &str = "lineos/m0/registry/m0-registry.json";
#[allow(dead_code)]
const CHECKSUMS_PATH: &str = "lineos/m0/registry/checksums.json";
#[allow(dead_code)]
const POLICIES_PATH: &str = "lineos/m0/config/policies.toml";
#[allow(dead_code)]
const AUDIT_LOG_DIR: &str = "lineos/m0/logs/audit";
#[allow(dead_code)]
const ASSETS_ROOT: &str = "lineos/m0/assets/wasm";
#[allow(dead_code)]
const HEALTH_ADDR: &str = "127.0.0.1:7401";
/// Mastering API — proxied through Caddy at 127.0.0.1:7400
#[allow(dead_code)]
const MASTERING_ADDR: &str = "127.0.0.1:7402";

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("m0d starting — LineOS M0 v0.1.0");
    tracing::info!("Authority: M0 Constitution v2.0");

    // ── Read env overrides ────────────────────────────────────────────────────
    let registry_path =
        std::env::var("M0_REGISTRY_PATH").unwrap_or_else(|_| REGISTRY_PATH.to_string());
    let checksums_path =
        std::env::var("M0_CHECKSUMS_PATH").unwrap_or_else(|_| CHECKSUMS_PATH.to_string());
    let policies_path =
        std::env::var("M0_POLICIES_PATH").unwrap_or_else(|_| POLICIES_PATH.to_string());
    let audit_log_dir =
        std::env::var("M0_AUDIT_LOG_DIR").unwrap_or_else(|_| AUDIT_LOG_DIR.to_string());
    let assets_root = std::env::var("M0_ASSETS_ROOT").unwrap_or_else(|_| ASSETS_ROOT.to_string());

    let gate = HealthGate::new();

    // ── Step 1: Audit log ─────────────────────────────────────────────────────
    let audit = audit::AuditLog::open(&audit_log_dir)?;
    audit.write(audit::entry_startup())?;
    gate.set_audit_writable(true).await;
    tracing::info!("Audit log ready: {}", audit_log_dir);

    // ── Step 2: Registry ──────────────────────────────────────────────────────
    let _reg = registry::Registry::load(&registry_path, &checksums_path).await?;
    gate.set_registry_loaded(true).await;
    tracing::info!("Registry loaded OK");

    // ── Step 3: CDN + hash verification ──────────────────────────────────────
    let cdn = cdn::Cdn::load(&assets_root, &checksums_path).await?;
    let failures = cdn.verify_all().await;
    if !failures.is_empty() {
        for (name, err) in &failures {
            tracing::error!("CDN verify failed: {} — {}", name, err);
        }
        anyhow::bail!("CDN hash verification failed — M0 cannot start (M0 §03.1)");
    }
    gate.set_cdn_ready(true).await;
    tracing::info!("CDN verified OK");

    // ── Step 4: Policy ────────────────────────────────────────────────────────
    let _policy = policy::PolicyEngine::load(&policies_path)?;
    gate.set_policy_active(true).await;
    tracing::info!("Policy engine loaded OK (deny-by-default active)");

    // ── Step 5: Caddy ─────────────────────────────────────────────────────────
    gate.set_caddy_running(true).await;
    tracing::info!("Caddy integration: Phase 1 stub (process management in Phase 2)");

    // ── Step 6: Health gate ───────────────────────────────────────────────────
    if !gate.is_healthy().await {
        anyhow::bail!("Health gate failed on startup — M0 cannot serve (M0 Constitution §04.2)");
    }
    audit.write(audit::entry_health_gate_passed())?;

    if let Err(e) = std::fs::create_dir_all("/run/lineos") {
        tracing::warn!("Could not create /run/lineos: {} (normal in dev)", e);
    } else {
        let _ = std::fs::write("/run/lineos/m0-healthy", "");
    }

    tracing::info!("✅ All health criteria passed — M0 is healthy");

    // ── Step 7: Build AppState for mastering API ──────────────────────────────
    let audit_arc = Arc::new(audit);
    let app_state = AppState::new(audit_arc.clone()).await;

    // ── Step 8: Start mastering API router (Phase 6, port 7402) ──────────────
    // Phase 6: mastering router binds directly to 7402.
    // Caddy (7400) proxies → 7402. This matches M0 Constitution §03.
    let mastering_router = mastering_router(app_state);
    let mastering_addr: SocketAddr = MASTERING_ADDR.parse()?;

    // ── Step 9: Start health endpoint (port 7401) ──────────────────────────────
    let health_app = health::health_router(gate.clone());
    let health_addr: SocketAddr = HEALTH_ADDR.parse()?;

    tracing::info!("Health endpoint:    http://{HEALTH_ADDR}");
    tracing::info!("Mastering endpoint: http://{MASTERING_ADDR}");

    // Pre-warm the LLM so first real request skips cold start
    tokio::spawn(async {
        tracing::info!("[m0d] pre-warming llama3.2:1b...");
        let _ = reqwest::Client::new()
            .post("http://localhost:11434/api/generate")
            .json(&serde_json::json!({
                "model": "llama3.2:1b",
                "prompt": "ready",
                "stream": false,
                "keep_alive": "30m"
            }))
            .send()
            .await;
        tracing::info!("[m0d] pre-warm done");
    });

    // Run both routers concurrently
    let health_listener = tokio::net::TcpListener::bind(health_addr).await?;
    let mastering_listener = tokio::net::TcpListener::bind(mastering_addr).await?;

    tokio::select! {
        res = axum::serve(health_listener, health_app) => {
            tracing::error!("Health router exited: {:?}", res);
        }
        res = axum::serve(mastering_listener, mastering_router) => {
            tracing::error!("Mastering router exited: {:?}", res);
        }
    }

    audit_arc.write(audit::entry_shutdown())?;
    Ok(())
}

/// Max simultaneous mastering jobs. Defaults to (physical cores - 1),
/// leaving headroom for UI/OS. Override via M0_MAX_CONCURRENT_JOBS.
fn max_concurrent_jobs() -> usize {
    std::env::var("M0_MAX_CONCURRENT_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get().saturating_sub(1).max(1))
                .unwrap_or(1)
        })
}

fn make_request_span(req: &axum::http::Request<axum::body::Body>) -> tracing::Span {
    let path = req.uri().path().to_string();
    let request_id = req.extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    if path.starts_with("/health") || path.starts_with("/progress") {
        tracing::debug_span!("http_request", method = %req.method(), %path, %request_id)
    } else {
        tracing::info_span!("http_request", method = %req.method(), %path, %request_id)
    }
}

/// Build the mastering API Axum router.
/// Phase 12A adds: POST /playback/control, GET /playback/state
/// Authority: Phase 6 task-decomposition P6-003 · Phase 12A P12A-007
fn mastering_router(state: AppState) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/master", post(handlers::master::trigger_mastering))
        .route(
            "/master/batch",
            post(handlers::master::trigger_batch_mastering),
        )
        .route("/preview", post(handlers::preview::create_preview))
        .route(
            "/preview/:id/:stem",
            get(handlers::preview::get_preview_stem),
        )
        .route("/blob/:id", get(handlers::blob::get_blob))
        .route(
            "/blob/:id/certificate.pdf",
            get(handlers::pdf_gen::get_track_certificate_pdf),
        )
        .route(
            "/album/:batch_id/certificate.pdf",
            get(handlers::pdf_gen::get_album_certificate_pdf),
        )
        .route(
            "/album/:batch_id/events/stream",
            get(handlers::album::stream_album_events),
        )
        .route("/export", post(handlers::export::export_audio))
        .route("/progress/:job_id", get(handlers::progress::get_progress))
        .route(
            "/progress/:job_id/stream",
            get(handlers::progress::stream_progress),
        )
        // Phase 12A/12B: PCM playback via xaak (A-003 §8)
        .route("/playback/state", get(handlers::playback::get_state))
        .route("/playback/control", post(handlers::playback::post_control))
        .route(
            "/cert/:blob_id/png",
            post(handlers::png_gen::export_cert_png),
        )
        .route(
            "/projects",
            post(handlers::projects::create_project).get(handlers::projects::list_projects),
        )
        .route("/projects/:id", get(handlers::projects::get_project))
        .route("/projects/:id/tracks", get(handlers::projects::list_tracks))
        .route("/mix/state", get(handlers::mix::get_state))
        .route("/mix/commit", post(handlers::mix::post_commit))
        .route("/mix/checkout", post(handlers::mix::post_checkout))
        .route("/mix/branch", post(handlers::mix::post_branch))
        .route("/mix/revert", post(handlers::mix::post_revert))
        .route("/mix/flavours", get(handlers::mix::get_flavours))
        .route("/mix/flavour", post(handlers::mix::post_flavour))
        .route("/tinder/variations", get(handlers::tinder::get_variations))
        .route("/tinder/like", post(handlers::tinder::post_like))
        .route("/tinder/result", post(handlers::tinder::post_result))
        .route(
            "/dev/snapshot",
            post(handlers::dev_snapshot::post_snapshot).get(handlers::dev_snapshot::get_snapshot),
        )
        .route("/dev/wait", post(handlers::dev_wait::post_wait))
        .layer(tower::limit::ConcurrencyLimitLayer::new(max_concurrent_jobs()))
        // Outer-wrapping order below is deliberate (Router::layer composition:
        // last .layer() call = outermost = sees request first):
        // 1) TraceLayer wraps ConcurrencyLimitLayer -> measures TOTAL client
        //    latency including queue wait, not just handler execution time.
        // 2) PropagateRequestIdLayer wraps TraceLayer -> copies the request's
        //    x-request-id onto the outgoing response, after Trace has read it.
        // 3) SetRequestIdLayer is outermost -> generates the UUID first,
        //    before anything else sees the request.
        .layer(TraceLayer::new_for_http().make_span_with(make_request_span))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

/// Test-only entry point for in-memory router testing (tower::oneshot).
/// Exposes the same router the production daemon serves, without
/// widening visibility of the internal mastering_router builder.
#[doc(hidden)]
pub fn build_router_for_test(state: AppState) -> axum::Router {
    mastering_router(state)
}
