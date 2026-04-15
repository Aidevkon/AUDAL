# cockpit/design-system/appendix-a-oklch.md

# Appendix A — OKLCH Color Space Specification (Rendering Layer)
**Status:** Informative (Non-Contractual)
**Scope:** Cockpit Rendering Engine
**Authority:** LineOS Constitution v1.1 · Cockpit Design Tokens v1.0
**Owner:** Lead Architect (Anestis)
**Date:** 2026-03-29

---

## A1. Purpose

Το Cockpit UI χρησιμοποιεί OKLCH ως εσωτερικό χρωματικό χώρο για rendering, ώστε να εξασφαλίζει:

- perceptual uniformity
- consistent contrast σε διαφορετικές οθόνες
- predictable severity colors
- future-proofing για HDR / wide-gamut displays
- deterministic rendering ανεξάρτητα από hardware

Το OKLCH δεν αλλάζει τα Design Tokens.
Είναι derived layer, όχι canonical layer.

---

## A2. Canonical vs Derived Values

### Canonical (Contractual)

Όλα τα Design Tokens v1.0 ορίζονται σε:

- HEX (sRGB)
- σταθερές τιμές
- immutable
- cockpit-wide

Παράδειγμα:
```
color.severity.high = #FF3B30
```

### Derived (Non-Contractual)

Το rendering engine μετατρέπει τα canonical HEX tokens σε OKLCH:
```
color.severity.high.oklch = oklch(#FF3B30)
```

Αυτές οι τιμές:
- δεν αποτελούν tokens
- δεν εκτίθενται στο UI
- δεν χρησιμοποιούνται σε specs
- δεν versionάρονται
- δεν αλλάζουν το contract

Είναι implementation detail.

---

## A3. Conversion Pipeline (Deterministic)

Η μετατροπή γίνεται με την εξής pipeline:

```
1. HEX → sRGB
2. sRGB → Linear RGB
3. Linear RGB → OKLab
4. OKLab → OKLCH
```

Η pipeline είναι:
- deterministic
- pure
- χωρίς hardware-dependent shortcuts
- χωρίς GPU approximations
- χωρίς adaptive behavior

Αυτό εξασφαλίζει ότι:
> Ίδιο token → ίδιο OKLCH → ίδιο rendering.

### A3.5 — Math Determinism Requirement

Όλα τα math operations στην OKLCH pipeline MUST χρησιμοποιούν `libm`.

```rust
// ✅ Correct
libm::sqrtf(x)
libm::atan2f(y, x)
libm::powf(x, exp)
libm::cbrtf(x)

// ❌ Forbidden
x.sqrt()
f32::atan2(y, x)
x.powf(exp)
x.cbrt()
```

**Γιατί:** Τα `std::f32` math methods παράγουν platform-dependent αποτελέσματα.
Χρησιμοποιώντας `libm` εξασφαλίζεται bit-exact output σε Windows / macOS / Linux.
Αυτό ευθυγραμμίζει το rendering pipeline με το DSP engine (ίδια απαίτηση).

---

## A4. Rendering Rules

1. Το Cockpit renderer πάντα χρησιμοποιεί OKLCH για:
   - severity colors
   - chip backgrounds
   - panel backgrounds
   - text contrast checks

2. Το UI δεν επιτρέπεται να χρησιμοποιήσει OKLCH απευθείας.
   Μόνο το renderer.

3. Το renderer δεν επιτρέπεται να αλλάξει hue/chroma/lightness των tokens.
   Μόνο να τα μετατρέψει.

4. Το renderer δεν επιτρέπεται να κάνει:
   - adaptive recoloring
   - auto-contrast adjustments
   - perceptual tweaks
   - theme-based modifications

Το OKLCH είναι representational, όχι transformational.

---

## A5. Determinism Requirements

1. Η μετατροπή HEX → OKLCH πρέπει να είναι bit-exact σε:
   - Windows
   - macOS
   - Linux

2. Δεν επιτρέπεται:
   - GPU fast-math
   - platform-dependent gamma curves
   - browser-dependent color management
   - OS-dependent color profiles
   - `std::f32` trig ή pow methods (βλ. §A3.5)

3. Το Cockpit πρέπει να χρησιμοποιεί δικό του deterministic color pipeline,
   όχι του browser/OS.

---

## A6. Forbidden Patterns

```
❌ Tokens ορισμένα σε OKLCH
❌ OKLCH τιμές σε specs ή contracts
❌ OKLCH rendering που αλλάζει το UI contract
❌ Perceptual adjustments στα severity colors
❌ OKLCH για dynamic theming
❌ OKLCH για "smart contrast"
❌ std::f32 math methods στην color pipeline
❌ GPU fast-math approximations
```

---

## A7. Future Extensions (Non-Binding)

Μελλοντικά, το OKLCH layer μπορεί να επεκταθεί για:

- HDR severity colors
- wide-gamut displays (P3, Rec.2020)
- perceptual contrast validation
- color-blind safe modes

Αλλά αυτά δεν αλλάζουν τα canonical tokens.

---

## A8. Relationship to Bootstrap

Το `bootstrap.md` Technology Stack Reference ορίζει:

```
CSS: HEX tokens (canonical, sRGB) — OKLCH derived at render-time
```

Αυτό είναι το authoritative statement για τον agent.
Το Appendix A παρέχει την πλήρη τεχνική εξήγηση.

Conflict resolution rule:
- Design Tokens v1.0 = canonical authority για token values
- Appendix A = canonical authority για rendering pipeline
- Bootstrap = summary reference — αν conflict, τα παραπάνω δύο κερδίζουν

---

## Ownership

Owner: Lead Architect (Anestis)
Maintainer: Cockpit Team
Status: 🔒 LOCKED

---

**Lead Architect:** Anestis
**System:** LineOS
**Document:** `cockpit/design-system/appendix-a-oklch.md`
**Version:** 1.0
**Date:** 2026-03-29
**Status:** 🔒 LOCKED
