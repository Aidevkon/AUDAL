import numpy as np
import json, os

SAMPLE_RATE = 48000

def release_coeff(release_ms, sample_rate):
    """
    EMA release coefficient.
    Same formula as current PeakFollower:
    1.0 - exp(-2.2 / (release_ms * 0.001 * sample_rate))
    """
    return 1.0 - np.exp(-2.2 / (release_ms * 0.001 * sample_rate))

# Industry standard dual-release times:
# Fast: ~30ms  (transients — snare, kick attack)
# Slow: ~300ms (sustained — 808, bassline, pads)
FAST_RELEASE_MS = 30.0
SLOW_RELEASE_MS = 300.0

fast_coeff = release_coeff(FAST_RELEASE_MS, SAMPLE_RATE)
slow_coeff = release_coeff(SLOW_RELEASE_MS, SAMPLE_RATE)

print(f"Fast release ({FAST_RELEASE_MS}ms): coeff = {fast_coeff:.10f}")
print(f"Slow release ({SLOW_RELEASE_MS}ms): coeff = {slow_coeff:.10f}")
print(f"Ratio fast/slow: {fast_coeff/slow_coeff:.2f}x")

# Verify: simulate transient decay
# After N samples, envelope should decay to 1/e of peak
def decay_to_half(coeff, sample_rate):
    """How many ms to decay to 50% of peak"""
    n = 0
    val = 1.0
    while val > 0.5 and n < sample_rate:
        val *= (1.0 - coeff)
        n += 1
    return n / sample_rate * 1000.0

fast_half_ms = decay_to_half(fast_coeff, SAMPLE_RATE)
slow_half_ms = decay_to_half(slow_coeff, SAMPLE_RATE)

print(f"Fast half-decay: {fast_half_ms:.1f}ms")
print(f"Slow half-decay: {slow_half_ms:.1f}ms")

# Blend factor calculation:
# blend = 0.0 → pure fast release (transient)
# blend = 1.0 → pure slow release (sustained LF)
# 
# Blend is determined by envelope slope:
# If envelope is falling steeply → transient → blend toward fast
# If envelope is falling slowly  → sustained → blend toward slow
#
# blend_coeff: how fast the blend itself transitions
# Should be ~10ms — fast enough to catch transients
BLEND_ATTACK_MS  = 1.0   # fast blend attack (catch transients)
BLEND_RELEASE_MS = 100.0 # slow blend release (smooth transitions)

blend_attack_coeff  = release_coeff(BLEND_ATTACK_MS,  SAMPLE_RATE)
blend_release_coeff = release_coeff(BLEND_RELEASE_MS, SAMPLE_RATE)

print(f"Blend attack  ({BLEND_ATTACK_MS}ms):  {blend_attack_coeff:.10f}")
print(f"Blend release ({BLEND_RELEASE_MS}ms): {blend_release_coeff:.10f}")

output = {
    "sample_rate": SAMPLE_RATE,
    "fast_release": {
        "ms": FAST_RELEASE_MS,
        "coeff": float(fast_coeff),
        "half_decay_ms": float(fast_half_ms)
    },
    "slow_release": {
        "ms": SLOW_RELEASE_MS,
        "coeff": float(slow_coeff),
        "half_decay_ms": float(slow_half_ms)
    },
    "blend": {
        "attack_ms": BLEND_ATTACK_MS,
        "attack_coeff": float(blend_attack_coeff),
        "release_ms": BLEND_RELEASE_MS,
        "release_coeff": float(blend_release_coeff)
    },
    "theory": {
        "blend_0_means": "pure fast release — transient mode",
        "blend_1_means": "pure slow release — sustained LF mode",
        "slope_threshold": 0.001
    }
}

os.makedirs("tests/fixtures", exist_ok=True)
with open("tests/fixtures/adaptive_release.json", "w") as f:
    json.dump(output, f, indent=2)
print("\nWritten: tests/fixtures/adaptive_release.json")
