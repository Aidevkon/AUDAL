# Creator OS — Amendment A-002
## Core is Internal, UI is Disposable

**Document:** `creator-os/constitution/amendments/A-002-ui-disposable.md`
**Version:** 1.0
**Date:** 2026-04-16
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.6 §13
**Inherits:** `creator-os-invariants.md` v1.1

---

## Preamble

This amendment formalizes the architectural separation between
the deterministic core and the disposable UI layer.

It defines what is "core", what is "surface", and what rules
govern the boundary between them.

If this amendment conflicts with the Creator OS Constitution v2.6,
the Constitution wins. If it conflicts with `creator-os-invariants.md`,
the invariants win.

---

## §1 — Core is Internal

The following components are **core**. They are internal.
They do not depend on any UI framework.
They do not change when the UI changes.

| Component | Layer | Notes |
|-----------|-------|-------|
| LineOS M0 | LineOS | Trust boundary, asset validation |
| sp314-dsp (E11) | LineOS M1 | 8-stage DSP pipeline |
| lineos-telemetry | LineOS M1 | BS.1770-4 windowed analysis |
| lineos-rule-engine | LineOS M1 | Deterministic findings |
| Golden Blob | LineOS M1 | Canonical output unit |
| Aether devices | Aether | ML enrichment (E1–E10) |
| adapter-runtime | Creator OS shared | LLM/Lyria sole entry point |
| CoachAdapter | Aether | LLM Adapter — validate_output enforced |
| Provider routing | adapter-runtime | phi3.5:3.8b, gemma2:9b, Lyria 3 |

**Rule:** Core components must never import any UI framework crate.
This is enforced by `ui-isolation-check.sh` on every commit.

---

## §2 — UI is Disposable

The following components are **surfaces**. They are disposable.
They can be rewritten, replaced, or run in parallel.
They must not contain core logic.

| Surface | Framework | Role |
|---------|-----------|------|
| Cockpit | Dioxus | Avionics UI — meters, telemetry, waveforms |
| Marketplace desktop viewer | Dioxus | Native desktop feel |
| Marketplace web frontend | Leptos | Browser — gallery, packs, account |
| Onboarding / Settings | Leptos | Web-oriented surfaces |

**Rule:** A surface may be replaced entirely without requiring
any change to core components. If replacing a surface requires
changing DSP, Aether, Coach, Telemetry, or Golden Blob — the
replacement is architecturally incorrect.

**Rule:** No surface may contain:
- Audio processing logic
- Loudness computation
- Rule evaluation
- LLM invocation (only via Tauri IPC → CoachAdapter)
- Provider routing decisions

---

## §3 — Surface ↔ Core Communication

The only permitted communication channel between a surface
and the core is **Tauri IPC commands**.

```
Surface (Dioxus / Leptos)
        │
        │  invoke("commandName", args)  ← Tauri IPC only
        ▼
Tauri backend (src-tauri/)
        │
        │  calls lineos-telemetry, lineos-rule-engine,
        │  CoachAdapter, M0 client, etc.
        ▼
Core (LineOS / Aether / adapter-runtime)
```

**Forbidden:**
```
❌ Surface importing lineos-telemetry directly
❌ Surface importing sp314-dsp directly
❌ Surface importing lineos-rule-engine directly
❌ Surface making HTTP calls to M0 directly (must go through Tauri IPC)
❌ Surface computing LUFS, LRA, True Peak itself
```

**HTTP API exception:** An HTTP surface (remote control, headless,
web Cockpit) is permitted only as a separately declared surface
in this document, with an explicit constitution amendment.
It does not replace the internal IPC channel.

---

## §4 — Framework Assignment

Each surface uses the best tool for its role.
There is no "Leptos or Dioxus" — there is the right tool per surface.

| Surface | Framework | Reason |
|---------|-----------|--------|
| Cockpit | **Dioxus** | Native rendering, avionics feel, desktop-first |
| Marketplace desktop | **Dioxus** | Consistent native experience |
| Marketplace web | **Leptos** | Browser-first, reactive, WASM |
| Gallery / Account | **Leptos** | Web-oriented, SSR-friendly |

**Rule:** Framework choice is per-surface, not per-project.
Adding a new surface with a different framework does not require
migrating existing surfaces.

---

## §5 — Telemetry Rule

The Telemetry layer is canonical. It is the only source of
loudness measurements.

```
PCM → sp314-dsp → lineos-telemetry → Golden Blob
                         ↑
              sole source of: LUFS, LRA, True Peak,
                              momentary, short-term
```

**Rules:**
- The UI never computes loudness measurements independently
- The UI reads metrics only from the Golden Blob via Tauri IPC
- `get_telemetry()` is the only permitted Tauri command for metrics
- Re-measurement after Golden Blob creation is forbidden

---

## §6 — Coach & Prompt Schema

The Coach is core (Aether / adapter-runtime).
The Coach prompt is not hardcoded — it lives in `assets/coach_prompt.toml`.

**Rules:**
- Prompt loaded at runtime, not compile time
- `include_str!` embedded fallback permitted for robustness
- Personas are optional UX layer — not core requirement
- Prompt changes do not require a code rebuild
- Prompt schema changes (new fields) require a patch version bump

---

## §7 — Migration Path (Leptos → Dioxus Cockpit)

The Cockpit migration from Leptos to Dioxus follows this sequence:

```
1. Write Dioxus Cockpit alongside existing Leptos Cockpit
   (both surfaces, same Tauri IPC commands)
2. Verify all Tauri IPC commands work identically from Dioxus
3. Verify all metrics display correctly (LRA, LUFS, Coach)
4. Switch Tauri window to Dioxus surface
5. Remove Leptos Cockpit code
6. Commit + tag
```

**Rule:** At no point during migration may core components be
modified. If a migration step requires touching DSP, Aether,
or adapter-runtime — stop and re-examine the approach.

---

## §8 — CI Enforcement

New CI gate: `ui-isolation-check.sh`

Uses `cargo tree` for **full transitive dependency** checking —
not just direct dependencies.

```bash
#!/usr/bin/env bash
# ui-isolation-check.sh — checks full transitive dep tree
# Authority: Amendment A-002 §8

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

UI_CRATES=("leptos" "dioxus" "dioxus-core" "dioxus-html" "leptos_dom" "leptos_macro")

CORE_CRATES=(
  "sp314-dsp"
  "lineos-telemetry"
  "lineos-rule-engine"
  "lineos-metadata"
  "lineos-insights"
  "m0d"
  "adapter-runtime"
)

VIOLATIONS=0

for crate in "${CORE_CRATES[@]}"; do
  DEPS=$(cargo tree -p "$crate" --prefix none 2>/dev/null \
         | awk '{print $1}' | sort -u)

  [ -z "$DEPS" ] && continue   # crate not yet in workspace — skip

  for ui_crate in "${UI_CRATES[@]}"; do
    if echo "$DEPS" | grep -q "^${ui_crate}$"; then
      echo "❌ VIOLATION: $crate → $ui_crate (transitive)"
      VIOLATIONS=$((VIOLATIONS + 1))
    fi
  done

  echo "  ✅ $crate — clean"
done

[ "$VIOLATIONS" -gt 0 ] && exit 1 || echo "✅ Core isolation: clean"
```

**Why `cargo tree` instead of `cargo metadata`:**
`cargo metadata --no-deps` only lists direct dependencies.
`cargo tree` resolves the full transitive closure — catching cases
where a helper crate pulls in a UI framework indirectly.

Add to `infra/ci/checks/ui-isolation-check.sh` and wire into `Justfile`:
```
ui-isolation:
    bash infra/ci/checks/ui-isolation-check.sh
```

---

## §9 — Consequences

Changing the UI must never require changing:
- DSP pipeline stages
- Aether devices or CoachAdapter
- Telemetry computation
- Provider routing
- Golden Blob schema
- adapter-runtime

A new UI surface = a new skin over the same Tauri IPC commands.
The core evolves independently of the UI.
The UI evolves independently of the core.

This is the architectural guarantee that makes Creator OS
maintainable at scale.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-04-16 | Initial amendment — formalizes core/UI separation, Dioxus+Leptos framework assignment, migration path, CI gate |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `creator-os/constitution/amendments/A-002-ui-disposable.md`
**Version:** 1.0
**Date:** 2026-04-16
**Status:** 🔒 LOCKED

---

*Core is what lasts. UI is what fits the moment.*
*The instruments never lie. The skin changes.*
