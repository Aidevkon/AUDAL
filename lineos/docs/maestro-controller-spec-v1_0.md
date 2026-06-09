# Maestro Auto-Tuning Controller — Spec v1.0 (Skeleton)
# lineos/docs/maestro-controller-spec-v1_0.md

**Status:** 📋 NEXT SESSION — First priority
**Date:** 2026-06-10

## 0. Why This Exists

The Psychoacoustic Collision Matrix uses hardcoded COLLISION_DUCKING_GAIN=0.707.
The UserMarkovModel and MfccAnalyzer exist but do not influence real-time DSP.
This spec closes that loop. The Maestro bridges the AI Brain and the DSP Muscle.

## 1. The Correct Architecture (4 steps)

Pass 1 scout() EXTENDED:
  Current: NMF only
  New:     NMF + MfccAnalyzer per stem
  Output:  ScoutResult { ..., stem_mfccs: StemMfccs }

Between Pass 1 and Pass 2 — AutoTuningController:
  Input:  ScoutResult.stem_mfccs (current track timbre)
  Input:  UserMarkovModel (historical sessions)
  Output: RenderParams { ducking_gain }

Pass 2 process_chunks() PARAMETERIZED:
  Current: hardcoded COLLISION_DUCKING_GAIN
  New:     receives ducking_gain from RenderParams

## 2. New Types

pub struct StemMfccs {
    voice, drums, bass, harmonics, ambience: [f32; 13] each
}

ScoutResult + stem_mfccs: StemMfccs

pub struct RenderParams {
    pub ducking_gain: f32,  // replaces COLLISION_DUCKING_GAIN
}

pub struct AutoTuningController;
impl AutoTuningController {
    pub fn compute_render_params(
        scout: &ScoutResult,
        model: Option<&UserMarkovModel>,
        preset: &str,
    ) -> RenderParams
}

## 3. MFCC Distance → Ducking Gain Mapping

bass_drums_distance = L2(stem_mfccs.bass, stem_mfccs.drums)

distance < 5.0:   ducking_gain = 0.5   (-6dB, aggressive)
distance < 15.0:  ducking_gain = 0.707 (-3dB, default)
distance >= 15.0: ducking_gain = 0.9   (-1dB, subtle)

Historical modifier from UserMarkovModel:
  high collision history → multiply 0.8
  low collision history  → multiply 1.1
  clamped to [0.3, 1.0]

## 4. Invariants

INV-AB-1:       Same WAV + same model + same preset → same output
INV-MAESTRO-1:  ducking_gain clamped to [0.3, 1.0]
INV-MAESTRO-2:  AutoTuningController is pure function
INV-MAESTRO-3:  No UserMarkovModel → distance-only logic

## 5. Implementation Phases

M-P1: StemMfccs + ScoutResult extension
M-P2: MfccAnalyzer in scout()
M-P3: AutoTuningController + RenderParams
M-P4: process_chunks_with_params()
M-P5: dsp_pipeline.rs integration
M-P6: E2E proof test

## 6. The Proof Test

assert!(params_heavy.ducking_gain < params_clean.ducking_gain,
    "Maestro must apply more aggressive ducking to bass-heavy track");
