#!/usr/bin/env python3
"""
Oracle: Reference Resolver Fixture Generator
============================================
Generates the ground-truth fixture for the
ReferenceResolver contract test (INV-REF-2).

Method:
  For each synthetic test case, computes:
    - per-band gain offsets G_k = target - input
    - SBR delta ΔR_dB = R(signal) - R(reference)
  using the same math the Rust resolver will use.

Authority: lineos/docs/reference-driven-sonic-
  vision-podcast-v1_1.md §3.2 + §3.4
Source: Byrne et al. 1994 JASA
  DOI:10.1121/1.410152 (via podcast-v1.json)
"""

import json
import math
import os

# ── Paths ─────────────────────────────────────
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROFILE_PATH = os.path.join(
    SCRIPT_DIR,
    "../../../../shared/schema/reference-profiles"
    "/podcast-v1.json"
)
OUTPUT_PATH = os.path.join(
    SCRIPT_DIR,
    "reference_resolver_fixture.json"
)

# ── Load reference profile ────────────────────
with open(PROFILE_PATH) as f:
    profile = json.load(f)

bands = profile["spectral_target"]["bands"]
N_BANDS = len(bands)  # 8
TARGET = [b["target_db_relative"] for b in bands]

SBR_LO = profile["sbr"]["lower_band_index"]  # 3
SBR_HI = profile["sbr"]["upper_band_index"]  # 4

# ── SBR helpers ───────────────────────────────
def sbr_db(profile_db: list[float]) -> float:
    """
    R_dB(X) = 10·log10(E_upper / E_lower)
    where E = 10^(band_db/10) (linear energy).
    Spec §3.2, WO2007120453 A1.
    """
    e_lo = 10 ** (profile_db[SBR_LO] / 10)
    e_hi = 10 ** (profile_db[SBR_HI] / 10)
    return 10 * math.log10(e_hi / e_lo)

def delta_sbr(signal_db: list[float]) -> float:
    """ΔR_dB = R(signal) - R(reference)."""
    return sbr_db(signal_db) - sbr_db(TARGET)

def gains(signal_db: list[float],
          g_max: float) -> list[float]:
    """
    G_k = clamp(TARGET[k] - signal[k], -g_max, g_max)
    Spec §3.4 step 3.
    G_max is the proprietary craft boundary —
    value here is for oracle/test purposes only.
    """
    return [
        max(-g_max, min(g_max, TARGET[k] - signal_db[k]))
        for k in range(N_BANDS)
    ]

# ── Test cases ────────────────────────────────
# Each case represents a synthetic 8-band input
# spectral profile (relative dBFS shape, mean=0).
#
# The oracle computes expected G_k and ΔR_dB
# so the Rust contract test can assert the
# resolver produces identical values (libm).

G_MAX_TEST = 6.0  # test-only; real value proprietary

TEST_CASES = [
    {
        "id": "balanced_voice",
        "description":
            "Input matches the reference target "
            "exactly — resolver should output "
            "zero gains and ΔR_dB ≈ 0.",
        "input_profile_db": TARGET[:],  # copy
    },
    {
        "id": "muddy_voice",
        "description":
            "Excess energy in Mid-Low [3] (mud/"
            "honk), deficiency in Mid-High [4] "
            "(articulation). ΔR_dB < 0 — signal "
            "is darker than reference. Resolver "
            "should cut [3] and boost [4].",
        "input_profile_db": [
            TARGET[0],
            TARGET[1],
            TARGET[2],
            TARGET[3] + 4.0,   # too much mud
            TARGET[4] - 3.0,   # too little clarity
            TARGET[5],
            TARGET[6],
            TARGET[7],
        ],
    },
    {
        "id": "harsh_sibilant_voice",
        "description":
            "Deficiency in Mid-Low [3], excess "
            "in Mid-High [4] (sibilance/harshness). "
            "ΔR_dB > 0 — signal is brighter than "
            "reference. Resolver should boost [3] "
            "and cut [4].",
        "input_profile_db": [
            TARGET[0],
            TARGET[1],
            TARGET[2],
            TARGET[3] - 3.0,   # too little body
            TARGET[4] + 4.0,   # too much harshness
            TARGET[5],
            TARGET[6],
            TARGET[7],
        ],
    },
    {
        "id": "boomy_dark_voice",
        "description":
            "Excess bass [1], deficiency across "
            "mids. Overall dark character. Tests "
            "multi-band correction, not just SBR.",
        "input_profile_db": [
            TARGET[0] + 2.0,
            TARGET[1] + 5.0,   # boomy
            TARGET[2] + 3.0,
            TARGET[3] + 2.0,
            TARGET[4] - 4.0,   # dark
            TARGET[5] - 5.0,
            TARGET[6] - 3.0,
            TARGET[7] - 2.0,
        ],
    },
]

# ── Compute expected outputs ──────────────────
results = []
for case in TEST_CASES:
    inp = case["input_profile_db"]
    g = gains(inp, G_MAX_TEST)
    dr = delta_sbr(inp)

    # Verify: balanced case should be ~zero
    if case["id"] == "balanced_voice":
        assert all(abs(gk) < 1e-9 for gk in g), \
            "balanced_voice: expected zero gains"
        assert abs(dr) < 1e-9, \
            "balanced_voice: expected ΔR_dB ≈ 0"

    # SBR convergence check: after applying gains,
    # does the output SBR move toward reference?
    output = [inp[k] + g[k] for k in range(N_BANDS)]
    dr_after = delta_sbr(output)
    sbr_converges = abs(dr_after) <= abs(dr) + 1e-9

    results.append({
        "id": case["id"],
        "description": case["description"],
        "input_profile_db":
            [round(v, 6) for v in inp],
        "expected_gains_db":
            [round(v, 6) for v in g],
        "expected_delta_sbr_db": round(dr, 6),
        "expected_delta_sbr_after_db":
            round(dr_after, 6),
        "sbr_converges": sbr_converges,
        "_g_max_test_only": G_MAX_TEST,
        "_invariant": "INV-REF-2",
    })

# ── Write fixture ─────────────────────────────
fixture = {
    "_comment":
        "Reference Resolver oracle fixture. "
        "Generated by generate_reference_resolver"
        "_fixture.py. DO NOT edit by hand.",
    "_spec":
        "lineos/docs/reference-driven-sonic-"
        "vision-podcast-v1_1.md §3.2 §3.4",
    "_profile": "podcast-v1",
    "_sbr_bands": {
        "lower_index": SBR_LO,
        "upper_index": SBR_HI,
        "lower_label": "Mid-Low (500-1kHz)",
        "upper_label": "Mid-High (1-2kHz)",
    },
    "_reference_target": TARGET,
    "_reference_sbr_db": round(sbr_db(TARGET), 6),
    "test_cases": results,
}

with open(OUTPUT_PATH, "w") as f:
    json.dump(fixture, f, indent=2)

print(f"✅ Fixture written: {OUTPUT_PATH}")
print(f"   Profile: podcast-v1 ({N_BANDS} bands)")
print(f"   Reference SBR: "
      f"{fixture['_reference_sbr_db']:.3f} dB")
print(f"   Test cases: {len(results)}")
for r in results:
    converges = "✅" if r["sbr_converges"] else "❌"
    print(f"   {converges} {r['id']}: "
          f"ΔR={r['expected_delta_sbr_db']:+.3f} dB "
          f"→ {r['expected_delta_sbr_after_db']:+.3f} dB")
