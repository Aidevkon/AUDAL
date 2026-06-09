# Corpus Learning — Spec v1.2 (Full Implementation)
# lineos/docs/corpus-learning-spec-v1_2.md

**Document:** `lineos/docs/corpus-learning-spec-v1_2.md`
**Version:** 1.2
**Date:** 2026-06-09
**Status:** 📝 READY FOR IMPLEMENTATION
**Authority:** Creator OS Constitution v2.5
**Owner:** Lead Architect (Anestis)
**Strategist:** Claude

---

## 0. Scope

This spec covers what is implemented TODAY.
For EDL Non-Destructive Editing → see `edl-spec-v1_0.md`.
For MFCCs / Spectral Flux → deferred until CB-P5 accuracy proven insufficient.

---

## 1. Philosophy

Pure mathematics. No neural networks. No GPU. No cloud.
Deterministic, explainable, <10KB models, no_std compatible.

INV-AB-1: Same WAV + same preset_id → same output forever.
Corpus learning NEVER modifies the default pipeline.
It produces explicit named Personas the user consciously selects.

---

## 2. Mode Collapse Prevention

Three-level hierarchy: User → Preset → Stem.

```
user: "anestis"
  preset: "techno"
    stem: "drums"   → own TransitionMatrix + EmissionHistograms
    stem: "bass"    → own TransitionMatrix + EmissionHistograms
  preset: "podcast"
    stem: "voice"   → own TransitionMatrix + EmissionHistograms

Techno drums P(transient) = 0.85 ← preserved
Jazz drums   P(transient) = 0.12 ← preserved
NEVER merged into a single "drums" model
```

INV-CB-8: Preset models NEVER cross-contaminate.

---

## 3. Feature Vector — CB-P1

```rust
pub struct StateFeatures {
    pub rms_db:            f32,   // [-144, 0] dBFS
    pub rms_delta:         f32,   // rms_db - prev_rms_db — KEY FEATURE
    pub transient_density: f32,   // [0, 1]
    pub spectral_centroid: f32,   // [20, 20000] Hz
    pub spectral_flatness: f32,   // [0, 1]
}

// rms_delta thresholds:
// > +6.0  → Attack
// < -3.0  → Decay
// ±1.0    → Sustain
// rms_db < -60 → Silence
```

Location: `lineos/m1/lineos-corpus/src/features.rs`

---

## 4. States Per Stem

```
voice:     ["silence", "breath", "consonant", "vowel", "tail"]
drums:     ["quiet", "transient", "sustain", "decay"]
bass:      ["silent", "punchy", "sustained", "rumble"]
harmonics: ["silent", "dense", "sparse"]
ambience:  ["dry", "subtle", "lush"]
```

---

## 5. Corpus Reader — CB-P2

```rust
// lineos/m1/lineos-corpus/src/reader.rs

pub fn read_corpus_sessions(corpus_dir: &Path) -> Vec<SessionCorpusEnvelope>

pub fn extract_state_sequence(
    envelope:  &SessionCorpusEnvelope,
    stem_type: &str,
) -> Vec<String>
// Output: ["silence", "vowel", "consonant", "tail", ...]
```

---

## 6. Transition Matrix — CB-P3

```rust
pub struct TransitionMatrix {
    pub states:        Vec<String>,      // sorted — determinism
    pub counts:        Vec<Vec<usize>>,  // counts[from][to]
    pub probs:         Vec<Vec<f32>>,    // normalized per row
    pub n_transitions: usize,
}

impl TransitionMatrix {
    pub fn from_sequence(sequence: &[String]) -> Self
    pub fn merge(&mut self, other: &TransitionMatrix)
    pub fn probability(&self, from: &str, to: &str) -> f32
}
// Training: counts[from][to] += 1, normalize row to sum 1.0
```

---

## 7. Emission Histogram — CB-P4

Histogram Binning, NOT Gaussian (audio features are skewed).

```rust
const RMS_DB_BINS:            [f32; 4] = [-80.0, -50.0, -30.0, -10.0];
const RMS_DELTA_BINS:         [f32; 4] = [-10.0, -2.0, +2.0, +10.0];
const TRANSIENT_DENSITY_BINS: [f32; 3] = [0.05, 0.15, 0.30];
const SPECTRAL_CENTROID_BINS: [f32; 3] = [500.0, 2000.0, 6000.0];
const SPECTRAL_FLATNESS_BINS: [f32; 2] = [0.1, 0.5];

pub struct EmissionHistogram {
    pub state:          String,
    pub feature_counts: FeatureBinCounts,
    pub total:          usize,
}

pub struct FeatureBinCounts {
    pub rms_db:            [usize; 5],
    pub rms_delta:         [usize; 5],
    pub transient_density: [usize; 4],
    pub spectral_centroid: [usize; 4],
    pub spectral_flatness: [usize; 3],
}

impl EmissionHistogram {
    pub fn likelihood(&self, features: &StateFeatures) -> f32
    // P(features|state) = product of bin probabilities (Naive Bayes)
}
```

---

## 8. Bayesian Inference — CB-P5

```rust
pub fn predict_state(
    features:   &StateFeatures,
    prev_state: &str,
    model:      &StemMarkovModel,
) -> StatePrediction
// Score = P(features|state) × P(state|prev_state)
// Replaces hardcoded confidence: 0.95

pub struct StatePrediction {
    pub state:      String,
    pub confidence: f32,  // real probability
    pub scores:     Vec<(String, f32)>,
}
```

---

## 9. Model Store — CB-P6

```rust
pub struct StemMarkovModel {
    pub stem_type:   String,
    pub transitions: TransitionMatrix,
    pub emissions:   HashMap<String, EmissionHistogram>,
    pub n_sessions:  usize,
}

pub struct PresetMarkovModel {
    pub preset_id: String,
    pub stems:     HashMap<String, StemMarkovModel>,
}

pub struct UserMarkovModel {
    pub user_id:    String,
    pub version:    u32,
    pub presets:    HashMap<String, PresetMarkovModel>,
    pub updated_at: u64,
}

impl UserMarkovModel {
    pub fn update(&mut self, preset_id: &str, session: &SessionCorpusEnvelope)
    pub fn to_json(&self) -> String
    pub fn from_json(json: &str) -> Result<Self, ...>
}
// Storage: {data_dir}/users/{user_id}/markov_model.json
```

---

## 10. Global Aggregator — CB-P7

```rust
// Per-preset aggregation — NEVER global merge
pub fn aggregate_preset(
    preset_id:   &str,
    user_models: &[&UserMarkovModel],
) -> Option<PresetMarkovModel>

// Storage: {data_dir}/global/{preset_id}_v{N}.json
// e.g.: global/techno_v2.json, global/podcast_v3.json
// NEVER: global/all_genres_v2.json
```

---

## 11. Album Mode — CB-P8

### 11a. API Extension

```json
// BatchMasterRequest — add batch_name
{
  "batch_name":    "Midnight Techno EP",
  "preset_id":     "techno",
  "anchor_index":  1,
  "items": [
    { "audioPath": "/music/track1.wav" },
    { "audioPath": "/music/track2_anchor.wav" },
    { "audioPath": "/music/track3.wav" }
  ]
}
```

UI default: `"Untitled Album - {date}"` — user can edit before submit.

### 11b. AlbumPersona (Temporary)

```
During batch mastering:
  1. Master anchor track first
  2. Build AlbumPersona from anchor's corpus data
  3. Use AlbumPersona as prior for remaining tracks
  4. Remaining tracks "pulled toward" anchor's characteristics:
     - Same drum decay profile
     - Same voice space
     - Same spectral balance
```

### 11c. Frozen Album Preset

```
After batch completes:
  → Store PresetMarkovModel as frozen snapshot
  → preset_id: "corpus-{batch_name_slug}-v1"
  → e.g.: "corpus-midnight-techno-ep-v1"

Use case — Bonus Track 6 months later:
  User selects "corpus-midnight-techno-ep-v1"
  System loads frozen model
  New track processed with same priors
  Result: sounds like it was recorded same day
```

### 11d. Macro-Dynamics Preservation

```
Markov state recognition prevents loudness war within album:
  Track 1: rock — P(transient)=0.7 — allow aggressive limiting
  Track 2: ballad — P(sustain)=0.9 — preserve dynamic range

Conductor reads state distribution per track BEFORE setting LUFS target.
Ballad is NOT pushed to same LUFS as rock.
Musical journey preserved.
```

---

## 12. JINI Auto-Naming — CB-P9

```
Trigger: user drops files into Batch UI
Input to JINI:
  - filenames (no audio content — INV-JINI-5 safe)
  - preset_id
  - pre-analysis stats if available (high/low transient, centroid range)
  
JINI prompt:
  "You are a music producer. User is mastering N tracks.
   Filenames: {names}. Genre: {preset_id}.
   DSP vibe: {stats}.
   Give 3 album/EP name ideas (max 4 words each). JSON only."

Output: 3 suggestions
UI: best suggestion pre-filled in "Album Name" field with ✨ icon
✨ click: cycles through alternatives
User edits freely or accepts as-is

Fallback (Ollama unavailable): "Untitled Album - {DD MMM YYYY}"
INV-JINI-1: user always confirms — never auto-applied
INV-JINI-5: filenames only — zero audio to cloud
```

---

## 13. dsp_pipeline.rs Integration — CB-P8

```
After mastering (existing):
  build_timeline() → write corpus.json ← already happens

Add (new):
  load UserMarkovModel for user_id + preset_id
  call model.update(preset_id, corpus_envelope)
  save UserMarkovModel

  if n_sessions % 10 == 0:
    recompute global preset snapshot

During mastering (if corpus preset selected):
  load PresetMarkovModel for preset_id
  use transition priors in state prediction
  use emission histograms for confidence scoring
```

---

## 14. Explicit Persona Selection (INV-AB-1)

```
UI preset picker:
  [spotify] [apple-music] [youtube]
  [corpus-techno-v3 ★] "Trained on 847 sessions"
  [corpus-midnight-techno-ep-v1] "Your album"

Rules:
  Default presets: deterministic forever, unchanged
  Corpus presets: deterministic for that frozen version
  User must explicitly upgrade to newer corpus version
  Same WAV + same preset_id → same output. Always.
```

---

## 15. Implementation Phases

| Phase | Task | Location | Gate |
|-------|------|----------|------|
| CB-P1 | StateFeatures + rms_delta | lineos-corpus/features.rs | cargo test |
| CB-P2 | Corpus reader | lineos-corpus/reader.rs | unit test |
| CB-P3 | TransitionMatrix | lineos-corpus/model.rs | unit test |
| CB-P4 | EmissionHistogram (binning) | lineos-corpus/model.rs | unit test |
| CB-P5 | Bayesian inference | lineos-corpus/inference.rs | accuracy test |
| CB-P6 | Model store (3-level hierarchy) | lineos-corpus/store.rs | roundtrip test |
| CB-P7 | Global aggregator per-preset | lineos-corpus/store.rs | merge test |
| CB-P8 | dsp_pipeline + Album Mode + Frozen Persona | m0-daemon | E2E test |
| CB-P9 | JINI Auto-Naming | m0-daemon/jini + Dioxus | UX test |

---

## 16. Invariants

| ID | Invariant |
|----|-----------|
| INV-CB-1 | Corpus never modifies default DSP behavior |
| INV-CB-2 | Model updates are incremental — no full retrain |
| INV-CB-3 | Global snapshots frozen and immutable after creation |
| INV-CB-4 | All model math is no_std compatible |
| INV-CB-5 | Models <10KB JSON per preset |
| INV-CB-6 | Cold start: new user begins with global preset model |
| INV-CB-7 | Same model + same features → same prediction |
| INV-CB-8 | Preset models NEVER cross-contaminate |
| INV-AB-1 | Same WAV + same preset_id → same output forever |
| INV-JINI-1 | User always confirms — JINI never auto-applies |
| INV-JINI-5 | Filenames only to JINI — zero audio to cloud |

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-09 | Initial spec |
| 1.1 | 2026-06-09 | PresetMarkovModel hierarchy (Mode Collapse prevention) |
| 1.2 | 2026-06-09 | Album Mode + JINI Auto-Naming + Macro-Dynamics |

---

**Lead Architect:** Anestis / **Strategist:** Claude
**Status:** 📝 READY FOR IMPLEMENTATION — CB-P1 next
