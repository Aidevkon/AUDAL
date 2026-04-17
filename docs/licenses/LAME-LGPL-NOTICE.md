# LAME MP3 Encoder — LGPL License Notice

**Document:** `docs/licenses/LAME-LGPL-NOTICE.md`
**Version:** 1.0
**Date:** 2026-04-17
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.6 §07.3

---

## Overview

Creator OS uses the LAME MP3 encoder for MP3 export in Still Air (A1).
LAME is licensed under the **GNU Lesser General Public License v2.1 (LGPL)**.

This notice documents the compliance requirements for LGPL usage
in Creator OS.

---

## LGPL Compliance Requirements

### 1. Dynamic Linking (Mandatory)

LAME must be linked **dynamically** — never statically.

```
✅ Dynamic linking:  libmp3lame.so → LGPL compliant
❌ Static linking:   libmp3lame.a  → LGPL violation
```

Verification:
```bash
ldd target/debug/m0d | grep -i "mp3\|lame"
# Must show: libmp3lame.so.0 => /usr/lib/... (0x...)
```

### 2. System Dependency

LAME is a system library, not vendored into the repository.

```bash
# Ubuntu/Debian installation
sudo apt install libmp3lame-dev

# Verification
pkg-config --libs mp3lame
# Expected: -lmp3lame
```

### 3. Cargo.toml Declaration

```toml
[dependencies]
# LGPL — dynamic linking only (see docs/licenses/LAME-LGPL-NOTICE.md)
mp3lame-encoder = "0.1"
```

The comment is **mandatory** — it documents the license exception
and links to this notice.

### 4. Build Script Requirement

The `mp3lame-encoder` crate uses `pkg-config` to find the system
LAME library. If `libmp3lame-dev` is not installed, the build fails
with a clear error.

CI environments must install `libmp3lame-dev` before building m0d.

---

## User Distribution Rights

Under LGPL, users of Creator OS have the right to:

1. Replace the LAME library with a compatible version
2. Receive information about which components use LAME
3. Link Creator OS against a modified version of LAME

Creator OS satisfies these requirements through:
- Dynamic linking (replaceability)
- This notice (transparency)
- Standard system package management (libmp3lame-dev)

---

## MP3 Patent Status

MP3 patents expired worldwide by 2017.
No patent licensing is required for MP3 encoding or decoding.

---

## LAME Project

- Website: https://lame.sourceforge.io
- License: LGPL v2.1
- Crate: `mp3lame-encoder` (Rust bindings)

---

## Summary

| Requirement | Status |
|-------------|--------|
| Dynamic linking | ✅ Required and enforced |
| Static linking | ❌ Forbidden |
| System library | ✅ libmp3lame-dev |
| Vendoring LAME source | ❌ Forbidden |
| MP3 patents | ✅ Expired (2017) |
| User replacement rights | ✅ Satisfied by dynamic linking |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `docs/licenses/LAME-LGPL-NOTICE.md`
**Version:** 1.0
**Status:** 🔒 LOCKED
