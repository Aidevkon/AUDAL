# LineOS — Phase 1 Master Prompt

**Document:** `lineos/plan/phase-1/master-prompt.md`
**Version:** 2.0
**Phase:** 1 — M0 Moat
**Tag:** `v0.1.0-m0`
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · M0 Constitution v2.0 · Phased Plan
**Date:** 2026-04-09

---

## ⚠️ PRE-CONDITIONS

```bash
# 1. Phase 0 tag exists
git tag | grep v0.0.0-constitution

# 2. Constitution documents in place
ls lineos/constitution/lineos-constitution.md
ls lineos/m0/constitution/m0-constitution.md
ls lineos/creator-os/constitution/creator-os-constitution-v2.5.md

# 3. Working directory and branch
pwd && git branch --show-current
# Expected: .../LineOS, canonical

# 4. Clean working tree
git status
# Expected: nothing to commit

# 5. Podman rootless
podman info | grep -i rootless
# Expected: rootless: true
```

**Any failure → STOP. Report. Do not proceed.**

---

## 📖 Reading Order (Mandatory)

```bash
cat lineos/creator-os/constitution/creator-os-constitution-v2.5.md  # §02, §09
cat lineos/m0/constitution/m0-constitution.md                        # PRIMARY
cat lineos/architecture/lineos-architecture.md                       # §2.1
cat lineos/plan/phase-1/task-decomposition.md
```

**Conflict rule:** M0 Constitution v2.0 supersedes all Phase 1 task-level decisions.
If a task conflicts with M0 Constitution → M0 Constitution wins. Report the conflict.

---

## 🎯 Phase 1 Goal

Build and verify M0 — the trust boundary of LineOS.

At the end of this phase:
- `m0d` daemon compiles, starts, passes all 7 health gate criteria
- `GET /health` → `{"status":"ok"}` within 30 seconds
- Caddy proxy routes via `127.0.0.1` only
- All WASM engine slots present in registry (stubs ok for Phase 1)
- Asset hash verification enforced — mismatch = fatal halt
- Marketplace gatekeeper rejects unsigned/malformed engines
- Audit log written for every significant operation
- `policies.toml` blocks undeclared outbound traffic
- Pod ports unreachable from host
- `m0-api.schema.json` present as API freeze boundary
- All M0 CI gates pass
- `just gate-phase1` passes cleanly

---

## 🔒 Forbidden in Phase 1

```
❌ Writing any DSP, AV, or mastering code
❌ Creating m1/ crates
❌ Creating cockpit/ code
❌ Binding Caddy to 0.0.0.0
❌ Using --network=host
❌ Downloading assets at runtime
❌ Making health gate depend on M1.x services
❌ Spawning external processes from m0d
❌ Implementing rate limiting or log rotation
   (operational — Operations Runbook only)
❌ Activating the WASM sandbox (Phase 2+ only)
```

---

## 🏁 Exit Criteria (Gate)

```bash
# 1. m0d compiles clean
cargo build --release -p m0d 2>&1 | grep "^error" | head -5
# Expected: no output

# 2. All M0 unit tests pass
just m0::test

# 3. All CI gates pass
just ci

# 4. Health endpoint
# (start m0d first: just m0::start &)
just m0::health
# Expected: {"status":"ok"}

# 5. Caddy localhost-only
just m0::validate-caddy-binding
# Expected: ✅

# 6. Pod ports blocked from host
for port in 7411 7412 7413 7414 7415; do
    curl -s --connect-timeout 2 http://127.0.0.1:$port > /dev/null 2>&1 \
        && echo "❌ PORT $port EXPOSED" || echo "✅ $port blocked"
done

# 7. Audit log present
ls lineos/m0/logs/audit/m0-audit-$(date +%Y%m%d).ndjson

# 8. Policy violation blocked
curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7400/undeclared-route
# Expected: 403 or 404

# 9. Marketplace rejection test
just m0::test-marketplace
# Expected: all tests pass (unsigned engine rejected)

# 10. Full integration gate
just gate-phase1
# Expected: ✅ Phase 1 gate complete

# 11. cargo deny
just deny
# Expected: zero violations
```

**All checks must pass. Then commit and tag.**

```bash
git add -A
git commit -m "feat(m0): Phase 1 — M0 Moat complete

- m0d daemon: CDN, registry, health gate, audit, policy enforcement
- Caddy reverse proxy: 127.0.0.1 only, deny-by-default routes
- Health endpoint: /health with all 7 criteria (M0 Constitution §04.2)
- Asset hash verification: fatal halt on mismatch
- Marketplace gatekeeper: manifest + signature + permissions + digest
- m0-api.schema.json: API freeze boundary in place
- systemd integration: m0d.service + m0d.quadlet
- All M0 Constitution v2.0 §08 CI gates pass"

git tag v0.1.0-m0
git log --oneline -3
```

---

## 📣 Completion Report

```
✅ Phase 1 — M0 Moat — COMPLETE

m0d daemon:              compiled ✅  started ✅  healthy ✅
Health endpoint:         {"status":"ok"} ✅
Caddy proxy:             127.0.0.1 only ✅
Asset verification:      SHA-256 + blake3 on startup ✅
Audit log:               append-only NDJSON ✅
Policy enforcement:      deny-by-default ✅
Marketplace gatekeeper:  unsigned engine rejected ✅
m0-api.schema.json:      API freeze boundary present ✅
Pod port isolation:      7411–7415 blocked from host ✅
just ci:                 ✅
just gate-phase1:        ✅

Tag: v0.1.0-m0 ✅

Ready for: Phase 2 — sp314-dsp (Audio Engine)
```

---

**Lead Architect:** Anestis
**System:** LineOS
**Phase:** 1 — M0 Moat
**Version:** 2.0
**Status:** 🔒 LOCKED
