# LineOS — M0 Constitution

**Document:** `lineos/m0/constitution/m0-constitution.md`
**Version:** 2.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution · Creator OS Constitution v2.5
**Inherits:** `creator-os/invariants/creator-os-invariants.md` v1.1
**Supersedes:** M0 Constitution v1.3

---

## Preamble

M0 is the trust boundary of LineOS.

It is the first process to start and the last to stop. Every interaction
between the outside world and the LineOS processing stack passes through M0.
No engine, no adapter, no application, no marketplace asset may bypass it.

This document is **normative** for the M0 subsystem. It is binding for CI,
deployment, and runtime startup sequencing.

If any rule in this document conflicts with the LineOS Constitution,
the LineOS Constitution wins. If it conflicts with `creator-os-invariants.md`,
the invariants win.

---

## §01 — What M0 Is

M0 is the local OS boundary of LineOS. It provides:

- **Trust boundary** — all traffic between Apps, Aether, and M1 services passes through M0
- **Local CDN** — serves version-pinned WASM engines and schemas
- **Reverse proxy** — routes all requests to M1 services via Caddy
- **Offline RPC gateway** — mediates Tauri IPC ↔ Podman pod services
- **Marketplace gatekeeper** — validates, verifies, and enforces permissions on all marketplace engines
- **Policy enforcement** — outbound traffic allowed only if explicitly declared
- **Audit log** — append-only NDJSON record of all significant operations
- **Health gate** — the system does not proceed until M0 reports healthy
- **API freeze boundary** — `lineos/m0/api/m0-api.schema.json` is the sole M0 public interface

**One sentence:** M0 is the moat that every request must cross — and earns the right to cross.

### §01.1 — What M0 Is Not

- ❌ Not a DSP engine — M0 never processes audio or video
- ❌ Not a measurement engine — M0 never computes LUFS, True Peak, or any metric
- ❌ Not a compute layer — M0 routes and enforces; it does not transform
- ❌ Not a plugin host — marketplace engines are sandboxed WASM, not plugins in M0
- ❌ Not a cloud client — M1.6 handles sync; M0 only enforces policy
- ❌ Not a container orchestrator — Podman handles orchestration; M0 gates startup
- ❌ Not an ML runtime — M0 has no knowledge of Aether or ML computation

M0 is a boundary, not a compute module.

---

## §02 — M0 Subsystems

| Subsystem | Responsibility |
|-----------|---------------|
| m0-daemon (`m0d`) | Process lifecycle, health gate, startup sequencing |
| Caddy reverse proxy | All inter-layer HTTP routing — localhost only |
| Local CDN | Serves WASM artifacts and schemas from `m0/assets/` |
| Registry | Version pinning and digest verification for all assets |
| Audit subsystem | Append-only NDJSON event log |
| Policy subsystem | Outbound traffic allowlist (`policies.toml`) |
| Health subsystem | Evaluates all health criteria before signalling ready |
| Marketplace gatekeeper | Manifest validation, signature verification, permission enforcement |
| WASM sandbox (Phase 2+) | Isolated execution environment for marketplace engines |

---

## §03 — Responsibilities

### §03.1 — Local CDN

M0 serves all WASM artifacts and schemas from `m0/assets/`. Every asset must
pass SHA-256 / blake3 hash verification before being served. Hash mismatch is
a fatal error — M0 halts, not warns.

Assets served:
- WASM engine artifacts: `speakforge.wasm`, `video-engine.wasm`, `av-forge.wasm`
- Native engine binaries: `sp314-native`, `av-forge-native`
- JSON schemas (local copies of `lineos/shared/schema/`)

**No asset is served without a passing hash check.**
**M0 does not download assets at runtime.** All assets are provisioned at install time.
Self-updating behavior is forbidden.

### §03.2 — Reverse Proxy

M0 routes all traffic via Caddy. All bindings are `127.0.0.1` — never `0.0.0.0`.

```
Apps / Cockpit  →  M0 (Caddy, localhost)  →  M1 services (pod-internal)
Aether          →  M0                     →  M1 services
Backend         →  M0 (m0-api boundary)   →  M1 services
M1.6 sync       →  M0 (policy gate)       →  Internet (opt-in only)
```

Route configuration is defined in `caddy.json`. All routes are explicitly declared.
Undeclared routes are blocked by default. No wildcard routes permitted.

### §03.3 — API Freeze Boundary

`lineos/m0/api/m0-api.schema.json` is the sole public interface of M0.

- All callers (Cockpit, Aether, backend) communicate with M0 only via this schema
- The schema is version-controlled — breaking changes require a new version
- No caller may rely on M0 internals — only on the published API schema
- The backend's `m0_bridge/` communicates with M0 exclusively through this schema

### §03.4 — Version Pinning and Registry

All WASM artifacts, native binaries, and marketplace engines are pinned by
digest in `m0/registry/m0-registry.json`.

Registry entries are immutable once locked. Updating a pinned entry requires:
1. New digest in `m0-registry.json`
2. Updated hash in `checksums.json`
3. CI asset hash validation pass

### §03.5 — Audit Log

M0 writes an append-only NDJSON audit log. Every significant operation is recorded.

- Access log: every proxied request
- Error log: all failures
- Audit log: security events, policy violations, marketplace decisions, sync attempts

**The audit subsystem writes synchronously.** A failed write is a fatal error — M0 halts.

Audit log rotation, size limits, and degraded mode: see `m0/docs/operations-runbook.md`.

### §03.6 — Policy Enforcement

Outbound traffic is allowed only if explicitly permitted in `policies.toml`.

Any request to an undeclared destination is:
1. Blocked immediately
2. Written to the audit log as a policy violation
3. Returned to the caller as 403

M0 never retries blocked requests. No automatic retry on policy violations.

### §03.7 — Marketplace Gatekeeper

M0 is the sole gatekeeper for all marketplace engines (E100+).

Before any marketplace engine loads, M0 performs all four steps in sequence:

```
1. Manifest validation    → engine-manifest.json validates against manifest.schema.json
2. Signature verification → developer Ed25519 sig + registry co-signature both valid
3. Permission enforcement → declared permissions checked against user-approved set
4. Digest verification    → blake3 digest of artifact matches registry entry
```

All four must pass. All-or-nothing — no partial loads.
Rejection is written to the audit log. The user is notified.

**Runtime hot-loading is forbidden.** All engines must be present, pinned, and
verified before system startup. Dynamic loading from untrusted sources is forbidden.

**Revoked keys:** M0 maintains a revoked key list. Any engine signed with a
revoked key is immediately unloadable. Revocation propagates on the next
registry sync.

### §03.8 — WASM Sandbox (Phase 2+)

The WASM sandbox (`lineos/m0/sandbox/`) provides isolated execution for
marketplace engines. Its configuration (`wasi.toml`, `limits.yaml`) defines
per-engine resource limits.

**Phase 2+ only.** The sandbox is defined in the repository tree but not
activated in Phase 1. Marketplace engine execution in Phase 1 uses M0 proxy
enforcement only. Full sandbox activation requires a constitution amendment.

---

## §04 — Invariants

These rules are absolute. No exception without a constitution amendment.

### §04.1 — Startup Invariants

- M0 must start before any other module
- If M0 fails to reach healthy status, LineOS does not start
- M0 must verify all asset hashes before serving anything
- The Podman pod must not start until M0 reports healthy
- The backend must not route requests until M0 is healthy

### §04.2 — Health Gate

M0 is **healthy** when all of the following are true simultaneously:

| # | Criterion | Description |
|---|-----------|-------------|
| 1 | Asset CDN ready | All assets in `checksums.json` verified and served without error |
| 2 | Registry loaded | `m0-registry.json` parsed, all digests verified |
| 3 | Caddy proxy running | Accepting connections on configured localhost port |
| 4 | Policy subsystem active | `policies.toml` loaded and enforced |
| 5 | Audit log writable | NDJSON file open, test write successful |
| 6 | Health endpoint responding | `GET /health` → `{"status":"ok"}` HTTP 200 |
| 7 | Health independence | Health evaluation uses M0-internal state only — no M1.x calls |

If any criterion fails: unhealthy, retry every 5 seconds, timeout at 30 seconds.
After timeout: fatal error, system halt, log written.

### §04.3 — Trust Invariants

- No module may bypass M0
- No outbound connection may occur without M0 policy approval
- No inbound connection may reach M1.x services directly
- **Caddy must bind only to `127.0.0.1`** — never `0.0.0.0` or any external interface
- **Podman pods must not use host networking** (`--network=host` is forbidden)
- M0 does not terminate TLS for external clients in Phase 1 — all traffic is local

### §04.4 — Determinism Invariants

- Asset delivery is deterministic — same registry state → same asset served
- Registry entries are immutable once locked
- Hash mismatches are fatal errors, never warnings

### §04.5 — Audit Invariants

- Audit logs are append-only
- No rotation without archival
- No deletion without explicit user action
- A failed audit write halts M0 immediately

---

## §05 — Forbidden Work

```
❌ M0 performing DSP, audio processing, or video processing
❌ M0 modifying data in transit
❌ M0 bypassing policies.toml for any reason
❌ M0 serving assets not listed in checksums.json
❌ M0 allowing direct App → M1 service traffic (must route via M0)
❌ M0 allowing direct Podman → Internet traffic without policy clearance
❌ M0 downloading assets or engines at runtime
❌ M0 binding Caddy to any interface other than 127.0.0.1
❌ Podman pods using --network=host
❌ M0 running containers or orchestrating Podman directly
❌ M0 storing raw audio, video, or user data
❌ M0 spawning external processes or executing scripts
❌ M0 loading a marketplace engine that has not passed all four verification steps
❌ M0 reporting healthy before all §04.2 criteria are met
❌ M0 permitting WebSocket or streaming connections without explicit caddy.json declaration
❌ M0 health evaluation depending on any M1.x service
❌ Runtime hot-loading of marketplace engines
❌ Accepting requests with serde_json::Value — typed schemas only at M0 API boundary
```

---

## §06 — Architecture

### §06.1 — Components

```
m0d (Rust daemon)
├── CDN subsystem            serves assets, verifies hashes
├── Registry subsystem       loads m0-registry.json, verifies digests
├── Caddy integration        manages Caddy process, monitors health
├── Policy subsystem         loads policies.toml, enforces on every request
├── Audit subsystem          append-only NDJSON, synchronous writes
├── Health subsystem         evaluates §04.2 criteria, exposes /health
└── Marketplace gatekeeper   manifest + signature + permissions + digest
```

### §06.2 — Traffic Flow

```
┌─────────────────────────────────────────────────────────┐
│  Apps (Cockpit) / Aether / Backend                      │
└──────────────────────────┬──────────────────────────────┘
                           │ localhost only
                           ▼
┌──────────────────────────────────────────────────────────┐
│  M0 (Caddy reverse proxy + m0d)                          │
│                                                          │
│  policies.toml enforcement on every request              │
│  audit log written for every significant operation       │
└──────┬───────────────┬──────────────────┬───────────────┘
       │               │                  │
       ▼               ▼                  ▼
  M1 services     WASM assets       Internet (opt-in,
  (pod-internal)  (CDN)             policy-gated via M1.6)
```

### §06.3 — Startup Sequence

```
systemd starts m0d
    │
    ├── verify all asset hashes (checksums.json)
    ├── load registry (m0-registry.json)
    ├── load policies (policies.toml)
    ├── start audit subsystem (test write)
    ├── start Caddy reverse proxy
    └── evaluate health gate (§04.2)
              │
         all passing?
              │
    ┌─────────┴──────────┐
   YES                   NO (retry every 5s, max 30s)
    │                         └── timeout → fatal halt + log
    │
  write /run/lineos/m0-healthy
    │
  systemd/Quadlet starts Podman pod
    │
  M1.x services start
    │
  Apps become operational
```

### §06.4 — Error Behavior

| Scenario | M0 behavior |
|----------|-------------|
| M1.x service unreachable | 504 after timeout, error logged — no retry |
| M1.x returns 5xx | Proxied to caller as-is, logged |
| Policy violation (outbound) | 403, audit log entry written |
| Asset hash mismatch | Fatal halt, log written |
| Audit write failure | Fatal halt immediately |
| Marketplace engine verification failure | 403, audit entry, user notified |
| Health gate timeout (30s) | Fatal halt, log written |
| Revoked key detected | Engine unloaded, audit entry, user notified |

**M0 performs no automatic retries.** All failed upstream requests fail
immediately after timeout. No exponential backoff. No circuit breaker at M0 level.

---

## §07 — Repository Structure (Canonical)

```
lineos/m0/
├── api/
│   └── m0-api.schema.json          ← API freeze boundary (sole public interface)
├── m0-daemon/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── cdn.rs
│       ├── registry.rs
│       ├── health.rs
│       ├── policy.rs
│       ├── audit.rs
│       └── marketplace/
│           ├── gatekeeper.rs
│           └── revocation.rs
├── sandbox/                        ← Phase 2+ (defined, not yet active)
│   ├── Cargo.toml
│   └── src/
│       ├── runtime/
│       │   ├── work/               ← ephemeral job dirs
│       │   └── state/              ← crash markers
│       └── policies/
│           ├── wasi.toml
│           └── limits.yaml
├── assets/
│   ├── wasm/                       ← version-pinned WASM artifacts
│   │   ├── speakforge.wasm         (E1)
│   │   ├── video-engine.wasm       (E12)
│   │   └── av-forge.wasm           (E13)
│   ├── engines/                    ← native binaries, pinned by digest
│   │   ├── sp314-native/
│   │   └── av-forge-native/
│   └── schemas/                    ← local copies of lineos/shared/schema/
├── registry/
│   ├── m0-registry.json            ← version manifest, all digests
│   └── checksums.json              ← SHA-256 / blake3 hashes
├── config/
│   ├── caddy.json                  ← reverse proxy config (localhost only)
│   ├── policies.toml               ← outbound allowlist (deny-by-default)
│   └── m0.toml                     ← M0 daemon config
├── logs/
│   ├── m0-access.ndjson
│   ├── m0-errors.ndjson
│   └── audit/
│       └── m0-audit-YYYYMMDD.ndjson
├── docs/
│   └── operations-runbook.md       ← rate limiting, log rotation, disk quotas
└── systemd/
    ├── m0d.service
    └── m0d.quadlet
```

**This structure is immutable.** Any deviation requires a constitution amendment.

---

## §08 — CI Gates

All gates are hard failures. No warnings.

| Gate | Check | Failure |
|------|-------|---------|
| Asset hash validation | All assets match `checksums.json` | Build failure |
| Native binary digest check | All engine binaries match registry digests | Build failure |
| Registry digest check | All entries pinned by digest | Build failure |
| Policy validation | `policies.toml` validates against schema | Build failure |
| `caddy.json` schema validation | Validates against `caddy.schema.json` — no wildcards | Build failure |
| Audit schema check | Audit entries validate against `audit.schema.json` | Build failure |
| Caddy localhost check | Caddy binds only to `127.0.0.1` — not `0.0.0.0` | Build failure |
| Direct pod access prevention | M1.x pod ports connection refused from host | Build failure |
| Podman network isolation | No `--network=host` in container definitions | Build failure |
| Startup sequence test | M0 healthy before Podman pod starts | Build failure |
| Health gate test | All §04.2 criteria pass within 30s | Build failure |
| Marketplace verification test | Unsigned/malformed engine rejected | Build failure |
| API schema check | All M0 callers use `m0-api.schema.json` — no raw values | Build failure |

---

## §09 — Amendment Process

1. Draft the amendment
2. Increment version (`2.0 → 2.1`)
3. Changelog entry with reason and impact
4. Update LineOS Constitution if OS-level rules are affected
5. CI must enforce new rules

Amendments are additive. No section may be deleted.
Lead Architect approval required.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 2.0 | 2026-04-09 | Full rewrite aligned with Creator OS Constitution v2.5 and current architecture. Added: §03.3 API freeze boundary (m0-api.schema.json), §03.7 Marketplace gatekeeper (Ed25519 + blake3, revocation), §03.8 WASM sandbox (Phase 2+ placeholder), §04.3 Podman --network=host invariant, §06.4 error behavior table (no retry rule). Updated: §03.1 AV engines in CDN (video-engine.wasm, av-forge.wasm), §07 repository tree (sandbox/, api/). Removed: concrete port numbers (principles only). Supersedes v1.3. |
| 1.3 | 2026-03-27 | Preamble binding statement; §03.1 module install-time rule; §03.6 lifecycle (update/revocation/rollback); §04.2 health independence; §04.3 Podman network isolation; §05 subprocess/WebSocket forbidden; §07 caddy.json gate + Podman isolation CI gate |
| 1.0 | 2026-03-26 | Initial constitution |

---

**Lead Architect:** Anestis
**System:** LineOS
**Document:** `lineos/m0/constitution/m0-constitution.md`
**Version:** 2.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED

---

*M0 is the moat.*
*M0 is the firewall.*
*M0 is the OS boundary.*
*If M0 fails, the system fails.*
*If M0 is compromised, the system is compromised.*
*If M0 is strong, LineOS is unbreakable.*
