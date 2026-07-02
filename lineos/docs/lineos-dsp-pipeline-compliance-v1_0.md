# lineos/docs/lineos-dsp-pipeline-compliance-v1_0.md
# Version: 1.0
# Date: 2026-07-02
# Owner: Lead Architect (Anestis)
# Authority: Development Protocol v1.0 §7
# Related: lineos/docs/reference-driven-sonic-vision-podcast-v1_1.md

# LineOS DSP Pipeline: Compliance & Verification White Paper

## 0. Executive Summary
This document outlines the forensic-grade verification methodology applied to the LineOS DSP engine (`sp314-dsp`), ensuring strict adherence to international broadcast standards and internal mastering requirements. The methodology enforces a "Measurement-First" doctrine: no code is trusted without runtime empirical proof.
**Note on Scope:** This document defines the *compliance laws* (LUFS, True Peak, Phase). For the *character* (the artistic spectral target), see the Reference-Driven Sonic Vision spec.

## 1. Loudness Gating & Integration (EBU R128 / BS.1770-5)
### The Challenge
Standard RMS or simple averaging fails to represent human loudness perception, especially in dynamic program material containing silent or quiet passages. EBU R128 mandates a two-stage gating mechanism (absolute at -70 LUFS, relative at -10 LU below the ungated average) to pause the measurement during drops in volume.

### Verification Strategy
To prove that our DSP correctly implements the relative gate, we designed the **EBU Gating Compliance Contract** (`ebu_compliance_contract.rs`).
*(Note: Complies with ITU-R BS.1770-5 (2018). The algorithm is identical to BS.1770-4, but wording is updated. See SPEC v1.1 §13 for full citation).*
- **Signal Design:** A 9-second synthetic stereo signal consisting of three 3-second phases: a 1kHz sine at -20dB, a drop to -40dB, and a return to -20dB.
- **Why -40dB?** Using a -40dB tone instead of absolute silence forces the *relative* gate to mathematically exclude the block, avoiding a naive `log10(0)` bypass.
- **Assertion:** The test asserts that the final Integrated LUFS measurement strictly matches the -20dB blocks (±1.0 LU tolerance), proving the -40dB block was successfully gated out.

### Real-World CI Regression
We introduced `e2e_real_world_loudness.rs`, utilizing a licensing-free 60-second music fixture. This ensures that dynamic, full-spectrum music successfully hits the designated target (e.g., `-16.0 LUFS` for Apple Music) in the CI environment.

## 2. True Peak & Intersample Overs (Nyquist Trap)
### The Challenge
Digital Sample Peak meters only measure the exact sampled points. The continuous analog waveform reconstructed by a DAC can peak significantly higher between samples (Intersample Peaks), causing hardware clipping.

### Verification Strategy
We implemented the **Quarter-Nyquist Reconstruction Contract** (`true_peak_contract.rs`) to verify the 4x oversampled polyphase FIR filter.
- **Signal Design:** We feed the algorithm the textbook Nyquist trap: a full-scale Fs/4 sine wave sampled at a -45° phase offset `[0.7071, 0.7071, -0.7071, -0.7071]`. 
- **The Trap:** A standard digital meter reads this as `-3 dBFS`. The continuous analog wave, however, peaks exactly at `1.0 (0 dBFS)`. 
- **Empirical Measurement:** Our 18-tap FIR filter correctly reconstructed this hidden peak, measuring `1.012591` (+0.109 dBFS).
- **Robust Assertion:** Instead of locking a snapshot test to `1.012591`, we assert an asymmetric range `[1.0, 1.05)`. This guarantees the algorithm never under-reads the peak while leaving room for future filter precision improvements.

## 3. Spatial Matrix Integrity (Mid/Side Phase Cancellation)
### The Challenge
Mid/Side processing is highly sensitive to phase and amplitude alignment. Any math error during the encode/decode cycle damages the stereo image.

### Verification Strategy
We created the **M/S Null Round-Trip Contract** (`midside_contract.rs`).
- **Signal Design:** A diverse 10-frame interleaved stereo buffer containing edge cases: silence, out-of-phase DC, pure mono, hard panning, asymmetrical decimals.
- **Null Test:** The buffer is passed through the `MidSideMatrix::encode()` and immediately through `MidSideMatrix::decode()`.
- **Assertion:** The reconstructed output must perfectly phase-cancel the original input, verified individually per-sample against a robust floating-point tolerance (`1e-6`). 

## 4. Phase Coherence (E2E)
Null tests (`assert_phase_coherence_null`) mathematically prove no sample-drift or broad-band phase distortion is introduced by the engine. 
**Streaming Block-Alignment Note:** The streaming engine operates with a block size of 512. The E2E phase tests automatically compensate for this block-latency before performing the null sum, ensuring deterministic verification across both batch and streaming topologies.

## 5. Denormal Flush (CPU Health)
Denormal or subnormal floats can cause catastrophic CPU spikes. 
**Constitutional Note:** Denormal prevention is handled explicitly via in-code mathematical flushing (e.g., in the limiter and masking EQ contracts) rather than relying on hardware FTZ/DAZ (Flush-to-Zero/Denormals-Are-Zero) flags, ensuring bit-identical reproducible results across all CPU architectures.

## 6. INV-QA 10-Gate Table
Our mastering quality suite (`e2e_mastering_quality.rs`) runs 10 empirical gates continuously (119 lib + 10 E2E tests):
1. **INV-QA-1:** Punch preservation (crest factor).
2. **INV-QA-2:** Harshness control (sibilance limits).
3. **INV-QA-3:** Spectral balance (no broadband shift).
4. **INV-QA-4:** Compressor activity bounds.
5. **INV-QA-5:** Headroom-aware LUFS makeup.
6. **INV-QA-6:** Mud correction (clarity).
7. **INV-QA-7:** Stereo phase coherence (correlation >= 0).
8. **INV-QA-8:** True Peak ceiling compliance.
9. **INV-QA-9:** Multi-genre fixture suite.
10. **INV-QA-10:** Over-scale input stress (+20dB finite stability).

## 7. Audio Definition Model (ADM) Compliance
The Creator OS spatial audio export pipeline was validated against the EBU ADM Renderer toolkit (ear-utils v2.1.0), an independent reference implementation of the ITU-R BS.2076 ADM standard developed by the EBU.
Validation confirmed:
- Six discrete audio track UIDs correctly mapped per EBU Tech 3364.
- All channels reference AP_00010009 (ITU 5.1 bed definition).
- Audio Track Format IDs follow AT_0001000N_01 for DirectSpeakers.
- AXML chunk contains well-formed XML conforming to EBU Core 2014.

## 8. Pending Validation
| Task | Status | Note |
|---|---|---|
| Logic Pro import test | 🟡 PENDING | Ensures Apple Music ingest compliance. |
| Apple Music distributor QC pass | 🟡 PENDING | Commercial certification. |

*These items elevate the pipeline from "spec compliant" to "production validated".*

## 9. Scope Boundary
Every document owns exactly one concern. 
- For empirical proofs and laws: *This Document*.
- For artistic character and references: see `lineos/docs/reference-driven-sonic-vision-podcast-v1_1.md`.
- For voice prediction/certainty: see `lineos/docs/CERTAINTY_AWARE_VOICE_SPEC.md`.
- For user preference learning: see `lineos/docs/corpus-learning-spec-v1_2.md`.
