# LineOS — Architecture Decision Records

**Document:** `lineos/architecture/adr.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.5
**Scope:** Phase 1 (M0 Moat) — all binding decisions before implementation

---

## Index

| ADR | Title | Status |
|-----|-------|--------|
| ADR-001 | WASM target: `wasm32-unknown-unknown` | 🔒 Accepted |
| ADR-002 | M0 runs as host process, not container | 🔒 Accepted |
| ADR-003 | Pure Rust signing: `ed25519-dalek` + `blake3`, not `ring` | 🔒 Accepted |
| ADR-004 | Podman rootless + Quadlet + systemd for M1 services | 🔒 Accepted |
| ADR-005 | Caddy as M0 reverse proxy (localhost-only) | 🔒 Accepted |
| ADR-006 | Dual-hash asset verification: blake3 (primary) + SHA-256 (secondary) | 🔒 Accepted |
| ADR-007 | WASM boundary: only `state.rs` imports engine WASM | 🔒 Accepted |

---

## ADR-001 — WASM Target: `wasm32-unknown-unknown`

**Date:** 2026-03-15
**Status:** 🔒 Accepted
**Decider:** Lead Architect

### Context

LineOS needs a WASM target for sp314-dsp and marketplace engines.
Two options existed: `wasm32-unknown-unknown` (browser-compatible, no system calls)
and `wasm32-wasi` (server-side WASM with system access).

### Decision

`wasm32-unknown-unknown` is the mandatory WASM target for all LineOS WASM artifacts.
`wasm32-wasi` is not used in LineOS 1.0.

### Rationale

- `wasm32-unknown-unknown` runs in the Cockpit WebView (browser environment)
- `wasm32-wasi` requires a WASI runtime and grants filesystem/network access
- Granting filesystem/network access to engine WASM violates the trust model
- M0 sandbox isolation is simpler with `wasm32-unknown-unknown`
- Leptos frontend requires `wasm32-unknown-unknown`

### Consequences

- All WASM crates compile with `--target wasm32-unknown-unknown`
- No `std::fs`, `std::net`, or process APIs in WASM code
- `serde_wasm_bindgen` at WASM boundary (not `serde_json`)
- wasm-opt post-processes all artifacts before placement in `m0/assets/wasm/`

---

## ADR-002 — M0 Runs as Host Process, Not Container

**Date:** 2026-04-09
**Status:** 🔒 Accepted
**Decider:** Lead Architect

### Context

M0 is the trust boundary of LineOS. It must start before all other services
and gate their startup. Two deployment options existed:
- M0 as a Podman container (same isolation model as M1 services)
- M0 as a native host process supervised by systemd

### Decision

M0 runs as a **native host process** supervised by systemd.
M0 is never containerised. M1 services run inside a rootless Podman pod.

### Rationale

- M0 must start before Podman — it cannot depend on Podman being running
- M0 gates pod startup via `/run/lineos/m0-healthy` — impossible if M0 is inside the pod
- The trust boundary cannot be inside the sandboxed environment it is meant to guard
- Host process has direct access to `/run/lineos/` for the health signal file
- systemd `Type=notify` gives reliable startup sequencing
- Running M0 in a container would require privileged access to manage other containers — worse security posture, not better

### Consequences

- `m0d` binary installs to `/usr/local/bin/m0d`
- `m0d.service` unit in `lineos/m0/systemd/`
- M1 Quadlet has `ConditionPathExists=/run/lineos/m0-healthy`
- M0 writes `touch /run/lineos/m0-healthy` when health gate passes
- M0 does not use Podman APIs — it is Podman-agnostic
- Caddy runs as child process of m0d (not a separate container)

---

## ADR-003 — Pure Rust Signing: `ed25519-dalek` + `blake3`, Not `ring`

**Date:** 2026-04-09
**Status:** 🔒 Accepted
**Decider:** Lead Architect

### Context

The M0 marketplace gatekeeper requires cryptographic signing and digest verification
for all marketplace engines. Three options were considered:
- `ring` — widely used Rust crypto library, C/assembly dependencies
- `ed25519-dalek` + `blake3` — pure Rust implementations
- `openssl` bindings — C library, system dependency

### Decision

**`ed25519-dalek` (v2) for Ed25519 signing and `blake3` (v1) for digests.**
`ring`, `openssl`, and all C-backed crypto libraries are forbidden.

### Rationale

- LineOS is pure Rust — C FFI in the signing path breaks this invariant
- `ring` uses C and assembly code; build failures on non-standard targets
- `ed25519-dalek` v2 is pure Rust, audited, and covers all required signing operations
- `blake3` is pure Rust, significantly faster than SHA-256 for large artifacts, and deterministic
- Pure Rust = reproducible builds across all targets without C toolchain dependency
- `ed25519-dalek` + `blake3` is sufficient for all Phase 1 signing requirements

### Signing model

- Developer signs engine artifact with their Ed25519 private key
- Registry holds developer's Ed25519 public key
- M0 verifies: developer signature + registry co-signature
- Asset integrity: blake3 digest in manifest matches computed digest of artifact
- SHA-256 retained as secondary checksum in `checksums.json` for compatibility

### Consequences

- `ed25519-dalek = "2"` in `m0d` Cargo.toml
- `blake3 = "1"` in `m0d` Cargo.toml
- `ring` is in the `deny.toml` ban list — build failure if added
- `just m0::hash-asset` outputs both blake3 and SHA-256
- `just digest file` → blake3 (primary)
- `just hash file` → SHA-256 (secondary)
- Revocation list stored in `m0-registry.json` as `revoked_keys: []`

---

## ADR-004 — Podman Rootless + Quadlet + systemd for M1 Services

**Date:** 2026-04-09
**Status:** 🔒 Accepted
**Decider:** Lead Architect

### Context

M1 services (telemetry, metadata, insights, rule-engine, M1.6 sync) need
an isolation and lifecycle model. Options:
- Docker (requires daemon running as root)
- Podman rootless + manual pod management
- Podman rootless + Quadlet (systemd-native container management)

### Decision

**Podman rootless + Quadlet + systemd** is the mandatory deployment model
for all M1 pod services.

### Rationale

- Rootless Podman: no root privileges required — security invariant satisfied
- Quadlet: systemd-native, no separate daemon, pod lifecycle tied to systemd
- `ConditionPathExists=/run/lineos/m0-healthy` in Quadlet gives M0-gated startup
- systemd handles restart, logging, and dependency ordering natively
- `--network=host` forbidden — pod uses `slirp4netns` (rootless user-space networking)
- Pod ports are not accessible from host — all access via M0 reverse proxy
- Reproducible: Quadlet files are declarative and version-controlled

### Consequences

- `infra/quadlet/m0d.quadlet` — M0 daemon unit
- `infra/quadlet/m1.quadlet` — M1 pod unit with `ConditionPathExists`
- Network mode: `slirp4netns` — never `host`
- All container images pinned by digest in `m0-registry.json`
- `just check-network` CI gate enforces no `--network=host`
- No Docker, no docker-compose, no Kubernetes

---

## ADR-005 — Caddy as M0 Reverse Proxy (Localhost-Only)

**Date:** 2026-04-09
**Status:** 🔒 Accepted
**Decider:** Lead Architect

### Context

M0 needs a reverse proxy to route traffic from Apps/Aether to M1 pod services.
Options: Caddy, nginx, custom Axum router, HAProxy.

### Decision

**Caddy** is the M0 reverse proxy. All bindings are `127.0.0.1`.
Caddy runs as a child process of `m0d`.

### Rationale

- Caddy has JSON-native configuration — version-controllable, no DSL
- Zero-config TLS (not needed now, available for post-1.0 remote nodes)
- Lightweight, single binary, no external dependencies
- `admin.disabled: true` prevents runtime config changes — configuration is immutable at runtime
- Running as m0d child process ties lifecycle to M0 health gate
- If Caddy dies, M0 reports unhealthy — clean failure model
- nginx requires separate config format; HAProxy is overkill for localhost routing

### Consequences

- `caddy.json` in `lineos/m0/config/` — version-controlled, no wildcards
- `127.0.0.1` only — `0.0.0.0` in `caddy.json` is a build failure
- All routes explicitly declared — undeclared routes → 404
- `just m0::validate-caddy-binding` CI gate
- Caddy version pinned in `m0-registry.json`
- Timeout per route declared explicitly (default: 10s)

---

## ADR-006 — Dual-Hash Asset Verification: blake3 (Primary) + SHA-256 (Secondary)

**Date:** 2026-04-09
**Status:** 🔒 Accepted
**Decider:** Lead Architect

### Context

M0 CDN must verify all assets before serving. Hash algorithm choice affects
performance, security, and tooling compatibility.

### Decision

**blake3 is the primary digest** for all assets in `m0-registry.json`.
**SHA-256 is the secondary checksum** in `checksums.json` for compatibility.
Hash mismatch on either → fatal halt, never warning.

### Rationale

- blake3: pure Rust, 3–5× faster than SHA-256, cryptographically sound, tree-hashing
- SHA-256: universally supported, required for compatibility with external tooling
- Dual-hash: blake3 for runtime verification speed, SHA-256 for interoperability
- `b3sum` CLI available on all target platforms (`cargo install b3sum`)
- Fatal halt on mismatch: tampered asset must never be served — this is non-negotiable

### Consequences

- `m0-registry.json` entries have both `blake3` and `sha256` fields
- `checksums.json` retains SHA-256 as primary for legacy compatibility
- `just digest file` → blake3
- `just hash file` → SHA-256
- `just m0::hash-asset file` → outputs both + registry entry template
- `b3sum` added to bootstrap pre-conditions (v1.3)
- CI: `just m0::verify-assets` checks SHA-256; runtime: M0 checks blake3

---

## ADR-007 — WASM Boundary: Only `state.rs` Imports Engine WASM

**Date:** 2026-03-15
**Status:** 🔒 Accepted
**Decider:** Lead Architect

### Context

In the Cockpit frontend (Leptos), WASM engine functions could be called
from any component. This creates invisible coupling and makes the boundary
impossible to audit.

### Decision

`apps/stillair/cockpit/src/state.rs` is the **sole permitted importer**
of `engine_wasm` functions in the frontend. No component, page, or module
may import engine WASM directly.

```
Components / Pages → AppState signals only → state.rs → engine_wasm
```

### Rationale

- Single import point = auditable boundary
- Components never know engine internals exist
- CI can enforce with a single grep gate
- Architectural separation: UI layer does not know how computation happens
- Matches the M0 principle: one controlled entry point for all traffic

### Consequences

- `grep -r 'engine_wasm\|wasm_bindgen_futures' cockpit/src/components/ cockpit/src/pages/` must return zero
- `just check-boundary` CI gate enforces this on every commit
- `state.rs` is the only file in the frontend with `use engine_wasm::`
- Signal updates from engine calls flow through `AppState` reactive signals only

---

## Amendment Process

New ADRs follow this process:
1. New entry added to this document
2. Status: Draft → Accepted (after Lead Architect review)
3. Version bump of this document
4. If the ADR introduces a new constitutional rule → Constitution amendment required

**Rule:** ADRs are immutable once Accepted. Superseding an ADR requires
a new ADR with explicit reference to the superseded one.

---

**Lead Architect:** Anestis
**System:** LineOS
**Document:** `lineos/architecture/adr.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
