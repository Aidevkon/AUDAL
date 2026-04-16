# LineOS — Phase 8 Master Prompt

**Document:** `lineos/plan/phase-8/master-prompt.md`
**Version:** 1.0
**Phase:** 8 — Aether Coach (LLM Narrative)
**Tag:** `v0.8.0-coach`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.6 · LLM Adapter Amendment v1.1
**Date:** 2026-04-16

---

## ⚠️ PRE-CONDITIONS

```bash
git tag | grep v0.7.0-decode
pwd && git branch --show-current
git status --short
ollama --version
ollama list | grep -E "phi|gemma"
```

**If ollama not installed:**
```bash
curl -fsSL https://ollama.ai/install.sh | sh
ollama pull phi3.5:3.8b
ollama pull gemma2:9b
```

**Any failure → STOP.**

---

## 📖 Reading Order (Mandatory)

```bash
cat creator-os/constitution/amendments/llm-adapter-amendment-v1.1.md
cat lineos/constitution/lineos-constitution.md   # §Coach rules
cat lineos/plan/phase-8/task-decomposition.md
```

**The LLM Adapter Amendment is the primary authority for this phase.**
Every LLM integration decision must be consistent with it.

---

## 🎯 Phase 8 Goal

Wire the Aether Coach — LLM narrative layer that explains
CoachFindings to the user in natural language.

**Architecture (binding):**
```
CoachFindings (from rule-engine) — read-only input
        │
        ▼
Aether Coach Adapter (apps layer)
        │  build_prompt(findings) → LLM
        │  validate_output() → CoachNarrative schema
        ▼
[ADAPTER BOUNDARY]
        │  typed, schema-validated CoachNarrative
        ▼
Cockpit Right MFD — display only
```

**LLM providers (Ollama local):**
- **Phi-3.5 Mini** — structured JSON output, fast
- **Gemma 2 9B** — narrative quality, explanation text
- A/B split via config — no code change to switch

**Coach identity (immutable):**
- Teacher, not engineer
- Explains "why" — never gives DSP instructions
- Never modifies findings or severity
- Never re-evaluates rules

**At end of phase:**
- Ollama running with phi3.5:3.8b + gemma2:9b
- `adapter-runtime` has Ollama provider
- Coach adapter registered in adapter-registry.json
- Right MFD shows real LLM narrative for each finding
- `just ci` passes
- Tag `v0.8.0-coach`

---

## 🔒 Forbidden in Phase 8

```
❌ LLM calls outside adapter-runtime
❌ Raw LLM output crossing Adapter Boundary
❌ Coach modifying CoachFindings severity or issues
❌ Coach adding new issues
❌ DSP instructions in narrative ("reduce gain by 2dB at 3kHz")
❌ Plugin installation suggestions
❌ Direct Ollama HTTP calls from frontend
❌ serde_json::Value in adapter I/O
❌ Retry logic in adapter (use adapter-runtime/retry.rs)
❌ LLM in LineOS or sp314-dsp
```

---

## 🏁 Exit Criteria

```bash
# Ollama responding
curl -s http://localhost:11434/api/tags | python3 -m json.tool | grep phi

# Full flow (manual)
# LOAD NEW → gargar.mp3 → Spotify → MASTER → FM5
# Right MFD shows:
#   - RECOMMENDATION: real LLM text
#   - Each finding has explanation text
#   - No "Rule evaluation unavailable."

cargo test --workspace
just ci
just deny
```

Commit + tag `v0.8.0-coach`.

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1) — Aether Layer
**Phase:** 8
**Status:** 🔒 LOCKED


---