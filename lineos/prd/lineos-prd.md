# LineOS — Product Requirements Document

**Document:** `lineos/prd/lineos-prd.md`
**Version:** 2.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.5
**Supersedes:** LineOS PRD v1.1

---

## §01 — Product Vision

LineOS is the deterministic execution substrate of Creator OS. For the end
user, it is invisible — they use Still Air (A1). For developers, it is the
foundation that makes all Creator OS applications deterministic, offline-capable,
and secure.

**Phase 1 (Tokyo) scope:** Audio mastering via Still Air (A1) with AV preview.
**Phase 2+ (Osaka+) scope:** MotionCraft (A2), additional apps, marketplace ecosystem.

---

## §02 — Scope

### §02.1 — In Scope (Phase 1)

| Feature | Module | Notes |
|---------|--------|-------|
| Audio mastering pipeline (8-stage) | sp314-dsp | Immutable between releases |
| AV preview (read-only) | av-core + E12/E13 | Still Air only — no editing |
| EBU R128 Levels 1–9 | telemetry | Full measurement stack |
| BMR-128 + EBU compliance | insights | Comparator — never re-measures |
| Mastering reports | metadata | JSON + human-readable |
| Deterministic coaching hints | rule-engine | No LLM, no Aether |
| Cockpit UI | Still Air | Tauri + Leptos, 3-panel |
| Offline-first operation | M0 | Full functionality without network |
| Local CDN + version pinning | M0 | All assets served locally |
| Marketplace gatekeeper | M0 | Ed25519 + blake3, sandbox Phase 2+ |
| Opt-in cloud sync | M1.6 | Manifest + reports + hash only |
| Audit log | M0 | Append-only, local |
| Deployment | infra | Podman rootless + Quadlet + systemd |

### §02.2 — Out of Scope (Phase 1)

| Feature | Target |
|---------|--------|
| AV timeline editing | MotionCraft A2 (Phase 2+) |
| Marketplace engine publishing | Phase 2+ |
| WASM sandbox activation | Phase 2+ |
| Multi-user / team workflows | Post-1.0 |
| Windows / macOS builds | Post-1.0 |
| Raw audio cloud sync | Post-1.0 |
| Aether ML enrichment | Aether layer (not LineOS) |

---

## §03 — Functional Requirements

### §03.1 — sp314-dsp (Audio Engine)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-DSP-1 | Accept WAV, MP3, FLAC, AIFF, OGG via symphonia | P0 |
| FR-DSP-2 | Run 8-stage mastering pipeline (per Pipeline Spec) | P0 |
| FR-DSP-3 | Produce Golden Blob (mastered FLAC + QualityMetrics) | P0 |
| FR-DSP-4 | Deterministic output: same input + seed → identical binary | P0 |
| FR-DSP-5 | Run in WASM mode (Cockpit) and native mode (pod/CI) | P0 |
| FR-DSP-6 | Support 5 presets: spotify, youtube, apple_music, tidal, raw | P0 |
| FR-DSP-7 | Use libm for all float math — never std::f32 methods | P0 |

### §03.2 — av-core (AV)

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-AV-1 | Accept MP4, MOV, MKV containers | P0 |
| FR-AV-2 | Orchestrate E12 (frame ops) + E13 (composition) via M0 IPC | P0 |
| FR-AV-3 | Batch IPC dispatch — never per-frame | P0 |
| FR-AV-4 | Produce Golden Blob (AV type) | P0 |
| FR-AV-5 | AV preview only in Still Air — no timeline editing | P0 |

### §03.3 — Telemetry

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-TEL-1 | EBU R128 Levels 1–9 (K-weighting, gating, LUFS, LRA, TP) | P0 |
| FR-TEL-2 | Video metrics: bitrate, resolution, HDR analysis | P0 |
| FR-TEL-3 | Read from Golden Blob only — never raw audio/video | P0 |
| FR-TEL-4 | Deterministic — same input → same measurements | P0 |

### §03.4 — Metadata + Insights

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-MET-1 | Generate BMR-128 report, EBU R128 report, project manifest | P0 |
| FR-INS-1 | Evaluate QualityMetrics against EBU R128 + BMR-128 profiles | P0 |
| FR-INS-2 | Output structured pass/fail per profile with recommendations | P0 |
| FR-INS-3 | Comparator only — never re-measures audio or video | P0 |

### §03.5 — rule-engine

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-RUL-1 | Evaluate compliance rules deterministically | P0 |
| FR-RUL-2 | No LLM calls, no Aether dependency, no randomness | P0 |
| FR-RUL-3 | Rules defined in version-controlled TOML/JSON files | P0 |
| FR-RUL-4 | Output feeds Cockpit Coach Island — does not modify pipeline | P0 |

### §03.6 — M0

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-M0-1 | Serve all assets from local CDN with hash verification | P0 |
| FR-M0-2 | Proxy all traffic to M1 services via Caddy (localhost only) | P0 |
| FR-M0-3 | Maintain append-only audit log | P0 |
| FR-M0-4 | Enforce policy rules from policies.toml | P0 |
| FR-M0-5 | Marketplace engine: manifest + signature + permissions + digest | P0 |
| FR-M0-6 | Health gate: all §04.2 criteria before signalling ready | P0 |

### §03.7 — Cockpit

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-UI-1 | Three-panel layout: Session / Insights / Coach | P0 |
| FR-UI-2 | File drop zone + A/B switch + preset selector | P0 |
| FR-UI-3 | Real-time LUFS / TP / LRA meters (visualization only) | P0 |
| FR-UI-4 | AV preview panel (read-only: playback, meters, export) | P0 |
| FR-UI-5 | Coach panel: rule-engine hints + Aether narrative (non-blocking) | P0 |
| FR-UI-6 | Export: FLAC + reports (via M0 policy gate) | P0 |
| FR-UI-7 | Full offline operation — zero network calls without M0 | P0 |
| FR-UI-8 | WASM boundary enforced: zero engine_wasm imports in ui/ | P0 |

---

## §04 — Non-Functional Requirements

| ID | Requirement | Target |
|----|-------------|--------|
| NFR-1 | Pipeline determinism | 100% binary match |
| NFR-2 | Audio mastering latency (native mode) | < 2× file duration |
| NFR-3 | Cockpit startup (M0 healthy, assets cached) | < 3 seconds |
| NFR-4 | M0 startup to healthy | < 30 seconds |
| NFR-5 | AV IPC batch size | ≥ 10 frames per call |
| NFR-6 | Zero data exfiltration without user consent | Hard requirement |
| NFR-7 | All containers rootless | Hard requirement |
| NFR-8 | WASM bundle size | < 5 MB gzip |
| NFR-9 | Audit log write latency | < 10ms per entry |

---

## §05 — Compliance Presets (Phase 1)

| Preset | LUFS Integrated | True Peak |
|--------|-----------------|-----------|
| spotify | −14 LUFS | −1.0 dBTP |
| youtube | −14 LUFS | −1.0 dBTP |
| apple_music | −16 LUFS | −1.0 dBTP |
| tidal | −14 LUFS | −1.0 dBTP |
| raw | No normalization | −0.1 dBTP |

All thresholds are defined in `lineos/shared/schema/bmr-128.schema.json`.
No hardcoded thresholds anywhere in code.

---

## §06 — Definition of Done (Phase 1)

- [ ] All P0 FRs pass automated tests
- [ ] Determinism test: pipeline ×2, binary diff = 0
- [ ] All JSON schemas validate (validate-schemas.sh — zero errors)
- [ ] cargo deny check — zero violations
- [ ] WASM boundary CI gate — zero violations
- [ ] Invariant inheritance check passes for all modules
- [ ] Container images pinned by digest
- [ ] M0 starts before pod — verified by integration test
- [ ] M0 audit log validates after full mastering session
- [ ] Cockpit renders 3 panels in offline mode
- [ ] AV preview functional (read-only)
- [ ] Opt-in sync requires explicit user confirmation
- [ ] All constitution documents LOCKED

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 2.0 | 2026-04-09 | Rewrite aligned with Creator OS v2.5 and LineOS v2.0. Added AV requirements (av-core, E12/E13, AV preview). Added rule-engine FRs. M0 FRs updated with marketplace gatekeeper. Cockpit AV preview panel added. Removed standalone product framing — LineOS is the deterministic substrate. |
| 1.1 | 2026-03-26 | Golden Blob canonical artifact; sync payload clarified |
| 1.0 | 2026-03-26 | Initial PRD |

---

**Lead Architect:** Anestis
**System:** LineOS
**Version:** 2.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
