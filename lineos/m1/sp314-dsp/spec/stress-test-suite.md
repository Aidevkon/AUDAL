# sp314-dsp Stress Test Suite — Spec v1.0
# Status: LOCKED 2026-05-09
# Owner: Lead Architect (Anestis)
#
# Test Categories:
# S1 — Amplitude extremes (full-scale, clipped, near-silence, silence, high-dynamic)
# S2 — Frequency extremes (infrasonic, DC, ultrasonic, pink noise)
# S3 — Duration extremes (single block, silence padding, max length, over-limit)
# S4 — Signal structure (mono-in-stereo, anti-phase, impulse train)
# S5 — InputProfile boundary cases
# S6 — Preset boundary cases
# S7 — Filter stability
# S8 — Multi-pass numerical stability
#
# Current implementation scope (2026-06-09):
# process_offline() returns Telemetry{peak_db, rms_db, lufs}
# GoldenBlobV3, InputProfile, PipelineWarning not yet implemented
# → Implement: TestSignalGenerator + S1 amplitude tests (NaN/Inf/bounds)
# → Defer: S2..S8 until GoldenBlobV3 exists
#
# TestSignalGenerator:
#   sine(freq_hz, amp_dbfs, duration_s, sample_rate) -> Vec<f32>
#   white_noise(rms_dbfs, duration_s, sample_rate, seed) -> Vec<f32>
#   silence(duration_s, sample_rate) -> Vec<f32>
#   mono_to_stereo(mono) -> (Vec<f32>, Vec<f32>)
#   All deterministic — no rand::thread_rng()
