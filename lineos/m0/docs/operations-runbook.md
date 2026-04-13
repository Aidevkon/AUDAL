# LineOS — Operations Runbook

**Document:** `lineos/m0/docs/operations-runbook.md`
**Version:** 1.1
**Date:** 2026-04-09
**Status:** OPERATIONAL — not a constitution document
**Authority:** LineOS Constitution v2.0 · M0 Constitution v2.0
**Changes from v1.0:** Authority references updated to current versions.

This document contains operational policies. Unlike constitution
documents, these may be updated without a constitution amendment.
Changes require Lead Architect approval and a version bump.

---

## Audit Log Management

### Bounds and Rotation Policy

1. Maximum audit log size per file: 1 GB.
2. Automatic gzip rotation when size limit is approached (at 900 MB).
3. Rotated files are archived — never deleted automatically.
4. Retention policy: 7 days for active logs, 30 days for archives.
5. Manual deletion requires explicit user action.

### Audit Degraded Mode

Activates when available storage drops below 500 MB:
- DSP and mastering pipeline continue uninterrupted
- A "Audit Degraded" warning is surfaced in the Cockpit
- Audit entries are buffered in memory (max 1000 entries)
- If memory buffer is exhausted, oldest entries are dropped
  (counter logged to stderr, never silently)

**Note:** "No audit failure may halt the DSP pipeline" is an
architectural rule — see `architecture/service-model.md §4`.

---

## Rate Limiting Policy

M0 Caddy proxy rate limits (configurable in `m0/config/caddy.json`):

| Route | Limit | Window |
|-------|-------|--------|
| `/telemetry` | 100 req/s | 1s |
| `/metadata` | 50 req/s | 1s |
| `/insights` | 50 req/s | 1s |
| `/coach` | 20 req/s | 1s |
| `/sync-egress` | 5 req/min | 60s |

Exceeding limits: 429 Too Many Requests + audit log entry.

---

## Audio Device Access (Podman Rootless)

### Enabling /dev/snd Access

```bash
# Add user to audio group
sudo usermod -aG audio $USER

# Verify
groups | grep audio

# In m1.quadlet, add device mapping:
# AddDevice=/dev/snd
```

### Auto-Detection

The Cockpit detects audio backend in this order:
1. PipeWire (preferred on modern Linux)
2. PulseAudio
3. ALSA direct

### Fallback Behavior

"No audio device" MUST NOT block application startup.
If no audio device is detected:
- Cockpit starts normally
- A non-blocking warning is displayed in the Session panel
- File processing (mastering, analysis) continues unaffected
- Playback features are disabled until a device is available

---

## Disk Quota Policy

| Directory | Soft limit | Hard limit |
|-----------|-----------|-----------|
| `m0/logs/` | 800 MB | 1 GB |
| `m0/assets/` | 2 GB | 5 GB |
| Golden Blob cache | 10 GB | 20 GB |

---

**Lead Architect:** Anestis
**System:** LineOS
