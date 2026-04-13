# Creator OS — Marketplace Constitution

**Document:** `marketplace/constitution/marketplace-constitution.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.5 — peer ecosystem layer
**Inherits:** `creator-os/invariants/creator-os-invariants.md` v1.1

---

## Preamble

This document is the constitution of the Creator OS Marketplace.

The Marketplace is a peer of Apps — not a dependency of them. Applications
function fully without the Marketplace. The Marketplace extends the system
with third-party engines, packs, and tools — but never replaces or bypasses
any core layer.

If any rule in this document conflicts with the Creator OS Constitution, the
Creator OS Constitution wins. If it conflicts with the invariants, the
invariants win.

---

## §01 — What the Marketplace Is

The Marketplace is the developer ecosystem of Creator OS. It provides:

- A **publish** path for third-party engine and pack developers
- A **verification** layer (M0) that enforces security before any asset loads
- A **registry** of signed, version-pinned assets
- A **revenue model** tied to the Creator Pool flywheel
- A **lifecycle** for every asset: publish → validate → sign → pin → load → deprecate → remove

### §01.1 — What the Marketplace Is Not

- ❌ A plugin host — marketplace engines are isolated WASM components, not plugins
- ❌ A backdoor — no marketplace asset may bypass M0 policy enforcement
- ❌ A dependency of core apps — Still Air works fully without Marketplace
- ❌ A DSP layer — marketplace engines extend functionality, never replace M1
- ❌ A source of non-deterministic computation in LineOS — stochastic engines are Aether devices only

---

## §02 — Asset Types

| Asset type | ID range | Owner | Description |
|------------|----------|-------|-------------|
| Engine plugins | E100–E999 | Third-party | WASM engines with WIT interface |
| Community engines | E1000+ | Community | WASM engines, lighter validation |
| Preset packs | — | Third-party | Mastering presets, EQ curves |
| Noise profiles | — | Third-party | Denoising profiles for E4 |
| Stem packs | — | Third-party | Pre-trained stem separation models |
| Voice packs | — | Third-party | TTS voice models for E1 (SpeakForge) |
| AV templates | — | Third-party | Scene/transition templates for AV pipeline |

### §02.1 — Engine ID Assignment

Engine IDs are permanent once published. A developer who publishes E247 owns E247 forever.
No ID reassignment. No ID recycling.

Assignment process:
1. Developer requests an ID (or a block of IDs for multiple engines) via marketplace registry
2. Creator OS team assigns the next available ID(s) in E100–E999
3. ID is reserved immediately — before the engine is published
4. If the engine is never published, the ID remains reserved (not recycled)

---

## §03 — Engine Lifecycle (Binding)

Every marketplace engine follows this exact lifecycle. No engine may skip a stage.

```
publish → validate → sign → pin → load → deprecate → remove
```

### §03.1 — publish

The developer submits:
- WASM artifact (compiled, wasm-opt optimized)
- WIT interface file (`.wit`)
- Manifest (`engine-manifest.json`) declaring:
  - Engine ID (pre-assigned)
  - Version (semver: `major.minor.patch`)
  - Declared permissions (explicit allowlist)
  - Input/output contract schemas
  - Determinism declaration (`deterministic: true | false`)
  - `blake3` digest of the WASM artifact

**Rules:**
- WASM artifact must be compiled from Rust only (pure Rust constraint)
- `wasm-opt` optimization is required — unoptimized artifacts are rejected
- `wit-bindgen` must be used for WIT → Rust bindings
- No C FFI, no C++ dependencies, no external subprocess calls
- Manifest must be complete — no optional fields without explicit justification

### §03.2 — validate

M0 runs automated validation before any human review:

```
1. Manifest schema validation    → engine-manifest.json validates against manifest.schema.json
2. WIT interface validation      → wasm-tools validate .wit file
3. WASM component validation     → wasm-tools validate WASM artifact
4. Digest verification           → blake3 digest in manifest matches artifact
5. Permission scope check        → declared permissions are within allowed scope
6. Contract schema check         → input/output schemas validate against creator-os/contracts/
7. Determinism check             → if deterministic: true, determinism test runs
```

Any validation failure → submission rejected. No partial acceptance.

### §03.3 — sign

After validation passes, the engine is signed:

- **Developer signature:** Developer signs the artifact with their `ed25519-dalek` private key
- **Registry co-signature:** Creator OS registry co-signs with the registry key
- Both signatures are stored in the registry alongside the `blake3` digest
- Signing is pure Rust: `ed25519-dalek` + `blake3` — no `ring`, no C FFI

**Key management:**
- Developer keys are issued by the Creator OS registry on developer onboarding
- Keys are Ed25519 keypairs — generated locally by the developer, public key registered
- Key revocation is immediate and propagated to all M0 instances
- A revoked key renders all engines signed with that key unloadable

### §03.4 — pin

After signing, the engine is pinned in the registry:

```json
{
  "engine_id": "E247",
  "name": "example-eq",
  "version": "1.0.0",
  "blake3_digest": "<hex>",
  "developer_signature": "<ed25519-hex>",
  "registry_cosignature": "<ed25519-hex>",
  "permissions": ["audio.read", "audio.write"],
  "deterministic": true,
  "status": "active",
  "published_at": "2026-04-09T00:00:00Z"
}
```

Registry entries are immutable once pinned. A new version requires a new entry.
Old versions are not removed — they remain pinned for rollback purposes.

### §03.5 — load

M0 enforces all-or-nothing loading:

```
1. Fetch engine manifest from registry
2. Verify developer_signature with developer's registered public key
3. Verify registry_cosignature with registry public key
4. Compute blake3 digest of local artifact — must match registry entry
5. Validate declared permissions against user-approved permission set
6. If all pass → load engine into WASM sandbox
7. If any fail → reject, write audit event, never load partially
```

**M0 sandbox constraints (per loaded engine):**
- Memory limit: configurable per engine, default 64 MB
- CPU time limit: configurable, default 5s per invocation
- No filesystem access outside declared sandbox path
- No network access
- No host process access
- No access to other engines' memory

### §03.6 — deprecate

Deprecation is a two-phase process:

**Phase 1 — Soft deprecation:**
- Registry status set to `deprecated`
- New installs are warned but not blocked
- Existing installs continue to work
- Minimum soft deprecation window: 90 days

**Phase 2 — Hard deprecation:**
- Registry status set to `removed`
- Engine no longer loadable by M0
- Existing users see deprecation notice in Cockpit
- No automatic uninstall — user must explicitly remove

Deprecation requires a published migration path or replacement engine.
Silent deprecation (no notice, no migration path) is forbidden.

### §03.7 — remove

An engine is removed from active availability but its registry entry is never deleted.
Historical entries are preserved for audit and rollback purposes.

Reasons for forced removal (outside normal deprecation):
- Security vulnerability confirmed in the engine
- Developer key revocation (compromise)
- License violation
- Determinism violation discovered post-publish

Forced removal is immediate — no deprecation window.
M0 propagates the removal and blocks loading within one registry sync cycle.

---

## §04 — Permissions System

Every engine declares its permissions in the manifest. M0 enforces them.

### §04.1 — Permission Scope

| Permission | Description | Requires user consent |
|------------|-------------|----------------------|
| `audio.read` | Read audio data from session | No (implied by install) |
| `audio.write` | Write processed audio back | No (implied by install) |
| `av.read` | Read AV data from session | No (implied by install) |
| `av.write` | Write processed AV back | No (implied by install) |
| `preset.read` | Read user presets | No |
| `preset.write` | Write/create presets | Yes |
| `filesystem.sandbox` | Read/write engine sandbox dir only | No |
| `network.none` | No network access (default) | N/A — always enforced |

**Forbidden permissions (no engine may declare these):**
```
❌ filesystem.host       — access outside sandbox
❌ network.outbound      — any outbound network call
❌ process.spawn         — subprocess execution
❌ ipc.direct            — bypass M0 for IPC
❌ memory.shared         — access other engines' memory
❌ audio.raw_pcm         — direct PCM access bypassing sanitization
```

### §04.2 — Permission Enforcement

M0 enforces permissions at the WASM boundary — not at the engine level.
An engine that attempts an undeclared operation is terminated immediately
and the attempt is written to the audit log.

---

## §05 — Determinism Contract

### §05.1 — Determinism Declaration

Every engine must declare `"deterministic": true` or `"deterministic": false`
in its manifest. This declaration is binding.

- `"deterministic": true` — same input + same seed → identical binary output.
  Validated by automated determinism test during `validate` stage.
  A determinism violation post-publish triggers forced removal.

- `"deterministic": false` — engine contains stochastic computation (e.g. ML inference).
  Stochastic engines must be declared as Aether devices in their manifest.
  Stochastic engines may not be placed in the LineOS deterministic processing path.

### §05.2 — Determinism Test

For `"deterministic": true` engines:
```bash
# Run engine twice with identical inputs and seed
engine_run --input test.flac --seed 42 --output out1.flac
engine_run --input test.flac --seed 42 --output out2.flac
diff out1.flac out2.flac   # Must be zero
```

Failure = manifest rejected. The engine cannot be published as deterministic.

---

## §06 — Revenue Model

### §06.1 — Creator Pool Flywheel

```
Creator output (Apps)
    ↓ published to
PublicGallery
    ↓ drives discovery →
Marketplace purchases
    ↓ revenue funds
Creator Pool storage capacity
    ↓ enables more
PublicGallery storage → more creator output
```

### §06.2 — Revenue Share

Revenue from marketplace engine and pack sales is split:
- Developer: primary share (percentage defined in marketplace terms)
- Creator Pool: infrastructure allocation (fixed percentage)
- Creator OS: platform fee (fixed percentage)

Exact percentages are defined in marketplace terms — not in this constitution.
The split model is constitutional; the exact numbers are operational.

### §06.3 — Revenue Rules

- Revenue is computed automatically — no manual allocation
- Developers receive payment per install or per use (model defined in terms)
- Creator Pool allocation is non-negotiable — it funds the public infrastructure
- No engine may charge for core functionality that Creator OS provides for free

---

## §07 — SDK Requirements

Every marketplace developer must use the Creator OS SDK.

### §07.1 — Required SDK Tools

| Tool | Purpose |
|------|---------|
| `wit-bindgen` | WIT → Rust bindings |
| `wasm-tools` | WASM validation and component model |
| `wasm-opt` | WASM optimization (required before submission) |
| `ed25519-dalek` | Engine signing (pure Rust) |
| `blake3` | Artifact digest (pure Rust) |
| `clap` | CLI tooling for publish workflow |

### §07.2 — SDK Constraints

- All SDK tooling is pure Rust — no C FFI
- The publish CLI (`creator publish`) wraps the full lifecycle
- Developers must not call registry APIs directly — use the CLI
- The CLI validates the engine locally before submission

---

## §08 — CI Requirements

All marketplace CI gates are hard failures.

| Gate | What it checks |
|------|---------------|
| Manifest schema | `engine-manifest.json` validates against `manifest.schema.json` |
| WIT validation | `.wit` interface is valid (wasm-tools) |
| WASM validation | WASM artifact is valid component (wasm-tools) |
| Digest match | `blake3` digest in manifest matches artifact |
| Permission scope | No forbidden permissions declared |
| Determinism test | For `deterministic: true` engines — binary diff = 0 |
| Signature verification | Developer signature valid against registered public key |
| Pure Rust check | No C FFI, no C++ dependencies in engine source |
| License audit | `cargo deny check licenses` — MIT/Apache2 only |

---

## §09 — Forbidden Work (Marketplace-Level)

```
❌ An engine loading without passing all M0 verification steps
❌ An engine declaring permissions it does not use
❌ An engine accessing resources outside its declared permissions
❌ An engine bypassing M0 for any operation
❌ An engine containing C FFI or C++ dependencies
❌ An engine spawning subprocesses
❌ An engine making outbound network calls
❌ An engine accessing raw PCM without the audio sanitization boundary
❌ An engine claiming deterministic: true without passing the determinism test
❌ A developer publishing under a revoked key
❌ Silent deprecation — no notice, no migration path
❌ ID recycling — a retired engine ID is never reassigned
❌ Revenue manipulation — no engine may misrepresent install or use metrics
❌ Stochastic engine placed in the LineOS deterministic processing path
```

---

## §10 — Amendment Process

1. Draft the amendment
2. Increment the version (`1.0 → 1.1`)
3. Document reason and impact in the Changelog
4. Update Creator OS Constitution if the amendment affects OS-level rules
5. CI must enforce any new rules

Amendments are additive. No section may be deleted.
Amendments require Lead Architect approval.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-04-09 | Initial constitution — full engine lifecycle (publish → validate → sign → pin → load → deprecate → remove), permissions system, determinism contract, revenue model, SDK requirements, CI gates |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `marketplace/constitution/marketplace-constitution.md`
**Version:** 1.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED

---

*Every engine that loads has earned the right to load.*
*M0 is the judge. The manifest is the contract. The signature is the bond.*
