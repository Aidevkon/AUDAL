# LineOS — Phase 1 Task Decomposition

**Document:** `lineos/plan/phase-1/task-decomposition.md`
**Version:** 2.0
**Phase:** 1 — M0 Moat
**Status:** 🔒 LOCKED
**Authority:** Phase 1 Master Prompt · M0 Constitution v2.0

---

## Task Order

```
P1-001  Workspace + m0d crate scaffold
P1-002  Registry + checksums subsystem (blake3 + SHA-256)
P1-003  Health gate + health endpoint (7 criteria)
P1-004  Caddy integration + reverse proxy
P1-005  Asset CDN + hash verification
P1-006  Audit subsystem
P1-007  Policy enforcement
P1-008  Marketplace gatekeeper (manifest + signature + permissions + digest)
P1-009  m0-api.schema.json (API freeze boundary)
P1-010  systemd + Quadlet integration
P1-011  Justfile recipes
P1-012  Integration tests + DoD gate
```

Gate-before-proceed. Each task DoD must pass before the next begins.

---

## P1-001 — Workspace + m0d Crate Scaffold

**Goal:** Add `m0d` to the Cargo workspace, create crate skeleton with module stubs.

**Actions:**

Update `Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = [
    "lineos/m0/m0-daemon",
]
```

Create `lineos/m0/m0-daemon/Cargo.toml`:
```toml
[package]
name = "m0d"
version = "0.1.0"
edition = "2021"
# M0 Constitution v2.0
# inherits_from = ["creator-os-invariants", "lineos-constitution"]

[dependencies]
tokio        = { version = "1", features = ["full"] }
axum         = "0.7"
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
toml         = "0.8"
sha2         = "0.10"
blake3       = "1"
hex          = "0.4"
ed25519-dalek = "2"
tracing      = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow       = "1"
chrono       = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tokio-test = "0.4"
```

Create `lineos/m0/m0-daemon/src/main.rs` — module stubs:
```rust
//! m0d — LineOS Local Mirror Daemon
//! M0 Constitution v2.0
//! Startup: registry → hash verify → policy → audit → caddy → health gate

mod audit;
mod cdn;
mod health;
mod marketplace;
mod policy;
mod registry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::init();
    tracing::info!("m0d starting — LineOS M0 v0.1.0");
    Ok(())
}
```

Create stubs: `audit.rs`, `cdn.rs`, `health.rs`, `marketplace.rs`,
`policy.rs`, `registry.rs` — each with `// TODO: Phase 1`.

**DoD P1-001:**
```bash
cargo check -p m0d
# Expected: zero errors
echo "✅ P1-001 complete"
```

---

## P1-002 — Registry + Checksums (blake3 + SHA-256)

**Goal:** Load and verify `m0-registry.json` and `checksums.json`.
Hash mismatch = fatal error, never a warning.

Create `lineos/m0/registry/m0-registry.json`:
```json
{
  "version": "0.1.0",
  "wasm_modules": {
    "sp314-dsp.wasm":      { "blake3": "", "sha256": "", "size_bytes": 0, "status": "pending" },
    "speakforge.wasm":     { "blake3": "", "sha256": "", "size_bytes": 0, "status": "pending" },
    "video-engine.wasm":   { "blake3": "", "sha256": "", "size_bytes": 0, "status": "pending" },
    "av-forge.wasm":       { "blake3": "", "sha256": "", "size_bytes": 0, "status": "pending" }
  },
  "engines": {
    "sp314-native": { "version": "0.1.0", "blake3": "", "sha256": "", "path": "lineos/m0/assets/engines/sp314-native/sp314" }
  },
  "marketplace_engines": {},
  "revoked_keys": []
}
```

Create `lineos/m0/registry/checksums.json`:
```json
{ "version": "0.1.0", "assets": {} }
```

Implement `registry.rs`:
```rust
pub struct Registry { /* ... */ }
impl Registry {
    pub async fn load(registry_path: &str, checksums_path: &str) -> anyhow::Result<Self>
    // Hash mismatch → anyhow::bail! — never silently pass
    pub fn verify_asset(&self, path: &str, actual_hash: &[u8]) -> anyhow::Result<()>
    pub fn revoked_keys(&self) -> &[String]
}
```

**DoD P1-002:**
```bash
cargo test -p m0d -- registry
# Hash mismatch test must pass (fatal behavior verified)
echo "✅ P1-002 complete"
```

---

## P1-003 — Health Gate + Health Endpoint

**Goal:** Implement all 7 criteria from M0 Constitution §04.2.
Health endpoint must NOT depend on any M1.x service.

```rust
pub struct HealthGate {
    pub cdn_ready: bool,        // all checksums.json assets verified
    pub registry_loaded: bool,  // m0-registry.json parsed + digests verified
    pub caddy_running: bool,    // port accepting connections
    pub policy_active: bool,    // policies.toml loaded
    pub audit_writable: bool,   // test write succeeded
    // criterion 6: health endpoint responding (implicit — if we respond, true)
    // criterion 7: health independence (enforced by design — no M1.x calls)
}

impl HealthGate {
    pub fn is_healthy(&self) -> bool {
        self.cdn_ready && self.registry_loaded && self.caddy_running
            && self.policy_active && self.audit_writable
    }
}
```

Health endpoint:
```
GET 127.0.0.1:7401/health
→ 200 {"status":"ok","criteria":{...}}     all pass
→ 503 {"status":"degraded","criteria":{...}}  any fail
```

Startup: retry every 5s, timeout 30s → fatal halt.

**DoD P1-003:**
```bash
cargo run --release -p m0d &
sleep 3
curl -sf http://127.0.0.1:7401/health | python3 -m json.tool
# Expected: {"status":"ok"}
kill %1
cargo test -p m0d -- health
echo "✅ P1-003 complete"
```

---

## P1-004 — Caddy Integration + Reverse Proxy

**Goal:** M0 manages Caddy. All traffic routes via `127.0.0.1`.
Never `0.0.0.0`. Deny-by-default.

Create `lineos/m0/config/caddy.json`:
```json
{
  "admin": { "disabled": true },
  "apps": {
    "http": {
      "servers": {
        "m0_proxy": {
          "listen": ["127.0.0.1:7400"],
          "routes": [
            {
              "match": [{"path": ["/telemetry*"]}],
              "handle": [{"handler": "reverse_proxy", "upstreams": [{"dial": "127.0.0.1:7411"}],
                          "transport": {"protocol": "http"}, "timeouts": {"response": "10s"}}]
            },
            {
              "match": [{"path": ["/metadata*"]}],
              "handle": [{"handler": "reverse_proxy", "upstreams": [{"dial": "127.0.0.1:7412"}],
                          "transport": {"protocol": "http"}, "timeouts": {"response": "10s"}}]
            },
            {
              "match": [{"path": ["/insights*"]}],
              "handle": [{"handler": "reverse_proxy", "upstreams": [{"dial": "127.0.0.1:7413"}],
                          "transport": {"protocol": "http"}, "timeouts": {"response": "10s"}}]
            },
            {
              "match": [{"path": ["/coach*"]}],
              "handle": [{"handler": "reverse_proxy", "upstreams": [{"dial": "127.0.0.1:7414"}],
                          "transport": {"protocol": "http"}, "timeouts": {"response": "10s"}}]
            },
            {
              "match": [{"path": ["/sync-egress*"]}],
              "handle": [{"handler": "reverse_proxy", "upstreams": [{"dial": "127.0.0.1:7415"}],
                          "transport": {"protocol": "http"}, "timeouts": {"response": "10s"}}]
            }
          ]
        }
      }
    }
  }
}
```

m0d spawns Caddy as child process, monitors its health.
If Caddy dies → M0 reports unhealthy.

**DoD P1-004:**
```bash
just m0::validate-caddy-binding
# Expected: ✅ Caddy localhost-only

curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7400/telemetry
# Expected: 502 (proxy working, pod not started) or 200

curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7400/undeclared
# Expected: 404

echo "✅ P1-004 complete"
```

---

## P1-005 — Asset CDN + Hash Verification

**Goal:** M0 serves assets only after hash verification. Tamper = fatal.

Implement `cdn.rs`:
```rust
pub struct Cdn { /* registry reference */ }
impl Cdn {
    pub async fn serve(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        // 1. Look up blake3 + sha256 from checksums
        // 2. Read file from m0/assets/{path}
        // 3. Compute actual blake3 digest
        // 4. Compare — mismatch: anyhow::bail! (fatal, never serve)
        // 5. Return bytes
    }
}
```

Create placeholder WASM stubs for testing:
```bash
echo "placeholder" > lineos/m0/assets/wasm/sp314-dsp.wasm
# Compute and register hash:
just m0::hash-asset lineos/m0/assets/wasm/sp314-dsp.wasm
# → Add to checksums.json
```

Tamper test (required):
```rust
#[tokio::test]
async fn test_hash_mismatch_prevents_serve() {
    // Write asset with wrong hash in checksums.json
    // cdn.serve() must return Err — never Ok
}
```

**DoD P1-005:**
```bash
just m0::verify-assets
# Expected: ✅ All assets verified

cargo test -p m0d -- cdn
# tamper test must pass
echo "✅ P1-005 complete"
```

---

## P1-006 — Audit Subsystem

**Goal:** Append-only NDJSON. Synchronous write. Failed write = fatal halt.
Validates against `lineos/shared/schema/audit.schema.json`.

Create `lineos/shared/schema/audit.schema.json`:
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "additionalProperties": false,
  "type": "object",
  "required": ["timestamp", "event_type", "level", "message"],
  "properties": {
    "timestamp":  {"type": "string", "format": "date-time"},
    "event_type": {"type": "string"},
    "level":      {"type": "string", "enum": ["info", "warn", "error", "audit"]},
    "message":    {"type": "string"},
    "metadata":   {"type": "object"}
  }
}
```

Minimum audit events to write:
- `m0d.startup`
- `m0d.health_gate_passed`
- `m0d.asset_served` (path + digest — never content)
- `m0d.policy_violation` (blocked destination)
- `m0d.proxy_request` (method + path + status)
- `m0d.marketplace_rejection` (engine_id + reason)
- `m0d.shutdown`

**DoD P1-006:**
```bash
ls lineos/m0/logs/audit/m0-audit-$(date +%Y%m%d).ndjson
just m0::audit-last 3
cargo test -p m0d -- audit
echo "✅ P1-006 complete"
```

---

## P1-007 — Policy Enforcement

**Goal:** Deny-by-default outbound. Every blocked request → audit log.

Create `lineos/m0/config/policies.toml`:
```toml
# M0 Policy Configuration
# M0 Constitution v2.0 §03.6
# deny-by-default — only explicitly listed destinations are allowed

[outbound]
# M1.6 sync — disabled by default (requires explicit user opt-in)
# [[outbound.allow]]
# destination = "https://api.creatorcloud.io"
# module = "m1.6-sync"
# requires_user_consent = true

[inbound]
# All inbound via Caddy proxy only
# Direct pod access blocked at network level
```

**DoD P1-007:**
```bash
cargo test -p m0d -- policy

# Verify blocked request → 403 + audit entry
curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7400/sync-egress
# Expected: 403

just m0::audit-violations | head -5
# Expected: policy_violation entry
echo "✅ P1-007 complete"
```

---

## P1-008 — Marketplace Gatekeeper

**Goal:** Implement all 4 verification steps from M0 Constitution §03.7.
Pure Rust: `ed25519-dalek` + `blake3`.

Implement `marketplace/gatekeeper.rs`:
```rust
pub struct Gatekeeper { /* registry reference */ }

pub enum GatekeeperError {
    ManifestInvalid(String),
    SignatureInvalid(String),
    PermissionDenied(String),
    DigestMismatch { expected: String, actual: String },
    KeyRevoked(String),
}

impl Gatekeeper {
    pub fn verify(&self, manifest: &EngineManifest, artifact: &[u8])
        -> Result<(), GatekeeperError>
    // Steps in order:
    // 1. Validate manifest against manifest.schema.json
    // 2. Verify developer Ed25519 signature + registry co-signature
    // 3. Check declared permissions against allowed scope
    // 4. Compute blake3 of artifact, compare to manifest digest
    // All-or-nothing — any step fails → return Err, write audit event
}
```

Implement `marketplace/revocation.rs`:
```rust
pub fn is_revoked(key: &str, revoked_keys: &[String]) -> bool
// Check against registry revoked_keys list
```

Required tests:
- unsigned engine → rejected
- wrong digest → rejected
- revoked key → rejected
- valid engine → accepted

**DoD P1-008:**
```bash
cargo test -p m0d -- marketplace
# All 4 test cases must pass
echo "✅ P1-008 complete"
```

---

## P1-009 — m0-api.schema.json (API Freeze Boundary)

**Goal:** Create the sole public interface of M0.
All callers communicate with M0 only through this schema.

Create `lineos/m0/api/m0-api.schema.json`:
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "version": "1.0",
  "description": "M0 public API — sole interface for all callers",
  "additionalProperties": false,
  "definitions": {
    "HealthResponse": {
      "type": "object",
      "required": ["status"],
      "properties": {
        "status": { "type": "string", "enum": ["ok", "degraded"] },
        "criteria": { "type": "object" }
      }
    },
    "ProxyRequest": {
      "type": "object",
      "required": ["path", "method"],
      "properties": {
        "path": { "type": "string" },
        "method": { "type": "string" },
        "headers": { "type": "object" }
      }
    }
  }
}
```

**DoD P1-009:**
```bash
python3 -c "import json; json.load(open('lineos/m0/api/m0-api.schema.json')); print('✅ m0-api.schema.json valid')"
echo "✅ P1-009 complete"
```

---

## P1-010 — systemd + Quadlet Integration

**Goal:** M0 runs under systemd. Pod gated on M0 health via `/run/lineos/m0-healthy`.

Create `lineos/m0/systemd/m0d.service`:
```ini
[Unit]
Description=LineOS M0 Daemon
After=network.target
Wants=network.target

[Service]
Type=notify
ExecStart=/usr/local/bin/m0d
Restart=on-failure
RestartSec=5s
TimeoutStartSec=35s
WorkingDirectory=/opt/lineos
User=lineos
Group=lineos
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

Create `infra/quadlet/m0d.quadlet` and `infra/quadlet/m1.quadlet`.
M1 quadlet: `ConditionPathExists=/run/lineos/m0-healthy` — gates pod on M0 health.
Network mode: `slirp4netns` — never `host`.

m0d writes `touch /run/lineos/m0-healthy` when health gate passes.

**DoD P1-010:**
```bash
ls lineos/m0/systemd/m0d.service
ls infra/quadlet/m0d.quadlet
ls infra/quadlet/m1.quadlet
grep -i "network=host" infra/quadlet/m1.quadlet && echo "❌" || echo "✅ No host networking"
grep "ConditionPathExists" infra/quadlet/m1.quadlet
# Expected: ConditionPathExists=/run/lineos/m0-healthy
echo "✅ P1-010 complete"
```

---

## P1-011 — Justfile Recipes

**Goal:** Place Justfiles at canonical paths. Verify all recipes work.

```bash
cp _docs/Justfile-v2          Justfile
cp _docs/m0-Justfile-v2       lineos/m0/Justfile
```

**DoD P1-011:**
```bash
just --list | grep -E "build|ci|validate|m0::"
just validate-schemas
just m0::validate
just m0::dev &   # should start cargo watch
# Ctrl+C
echo "✅ P1-011 complete"
```

---

## P1-012 — Integration Tests + DoD Gate

**Goal:** Run all Phase 1 exit criteria. Every check must pass.

```bash
# Full gate
just ci && just gate-phase1 && just m0::ci
# Expected: all ✅

# Marketplace test
just m0::test-marketplace

# Audit log check
just m0::audit-last 5
```

If any ❌ → fix before tagging.

**DoD P1-012:**
```bash
just ci && just gate-phase1 && just m0::ci
echo "✅ P1-012 complete — ready to tag"
```

---

## Phase 1 Complete

All 12 tasks done → deliver completion report from master prompt.

```bash
git add -A
git commit -m "feat(m0): Phase 1 — M0 Moat complete ..."
git tag v0.1.0-m0
```

---

**Lead Architect:** Anestis
**System:** LineOS
**Phase:** 1 — M0 Moat
**Version:** 2.0
**Status:** 🔒 LOCKED
