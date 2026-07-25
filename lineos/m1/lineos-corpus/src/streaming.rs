//! StreamingCorpusBuilder — Stage 1: PerStemBuilder
//!
//! Incremental, per-window equivalent of batch build_windowed_stem +
//! extract_state_sequence + extract_features_sequence + StemMarkovModel::train.
//!
//! Mathematical identity proven by recon:
//!   TransitionMatrix::from_sequence is pure sequential bigram.
//!   merge() is associative count-addition.
//!   MfccAnalyzer::compute is stateless per window.
//!   EmissionHistogram::observe is purely per-event.
//!
//! Streaming carry state across push_chunk() calls:
//!   - `leftover`:         samples < one 100ms window (carried to next call)
//!   - `prev_state`:       last classified window state (for transition bigram)
//!   - `prev_rms_db`:      previous window's rms_db (for rms_delta, mirrors
//!     extract_features_sequence's prev_rms_db init of -144.0)
//!   - `observed_states`:  every state ever output by classify_for_stem during
//!     push_chunk — matches from_sequence's BTreeSet collection
//!     of observed states (NOT the full closed vocab).
//!
//! INV-SCB-1: push_chunk produces IDENTICAL windows to batch (same tail-drop).
//! INV-SCB-2: finish() transition counts == from_sequence over the same state seq.
//! INV-SCB-3: finish() emission histograms == per-event observe() over same features.
//! INV-SCB-4: finish() states == observed-only, sorted alphabetically — NOT full vocab.

use std::collections::{BTreeSet, HashMap};

use crate::{
    builder::{classify_for_stem, features_for}, // single sources of truth — no local copies
    contract::{EnrichedAttributes, RiskFlags},
    features::StateFeatures,
    inference::StemMarkovModel,
    mfcc::MfccAnalyzer,
    model::{normalize_rows, EmissionHistogram, TransitionMatrix}, // normalize_rows: single source
    store::PresetMarkovModel,
};
use lineos_types::pre_analysis::PreAnalysisData;
use lineos_types::StemFeatures;

/// Incremental builder for one stem's Markov model.
///
/// Call `push_chunk(&[f32], &mut MfccAnalyzer)` for each audio chunk.
/// Call `finish()` when all chunks have been pushed.
///
/// MfccAnalyzer is borrowed per push_chunk so the caller (Stage 2)
/// can share one instance across all 5 PerStemBuilders.
pub struct PerStemBuilder {
    stem_type: &'static str,
    base_attrs: EnrichedAttributes,
    #[allow(dead_code)] // carried for Stage 2 completeness; not used in transition/emission path
    base_risk: RiskFlags,
    td: f32,
    window_samples: usize,

    // ── streaming carry state ──────────────────────────────────────────────────
    /// Samples from the last push_chunk call that didn't fill a complete window.
    leftover: Vec<f32>,
    /// State produced by the last completed window. Used to form the next bigram.
    prev_state: Option<&'static str>,
    /// RMS dB of the last completed window.
    /// Init -144.0 mirrors extract_features_sequence reader.rs:58 (prev_rms_db = -144.0).
    prev_rms_db: f32,
    /// Every state ever produced by classify_for_stem — including the final window's
    /// state which has no outgoing transition. Matches from_sequence's BTreeSet
    /// collection over the FULL state sequence (not just bigram endpoints).
    observed_states: BTreeSet<&'static str>,

    // ── accumulators ──────────────────────────────────────────────────────────
    /// Bigram counts: (from_state, to_state) → count.
    /// &'static str keys — no heap alloc per window.
    transition_counts: HashMap<(&'static str, &'static str), usize>,
    /// Per-state emission histograms. Key is &'static str.
    emissions: HashMap<&'static str, EmissionHistogram>,
}

impl PerStemBuilder {
    /// Create a new builder for one stem.
    ///
    /// `base_attrs` and `base_risk` are the same global-scalar structs that batch
    /// `build_timeline` injects via `features_for()` and `pre_analysis`.
    pub fn new(
        stem_type: &'static str,
        base_attrs: EnrichedAttributes,
        base_risk: RiskFlags,
        td: f32,
        sample_rate: u32,
    ) -> Self {
        let window_samples = ((sample_rate as f32 * 0.1) as usize).max(1);
        Self {
            stem_type,
            base_attrs,
            base_risk,
            td,
            window_samples,
            leftover: Vec::new(),
            prev_state: None,
            prev_rms_db: -144.0,
            observed_states: BTreeSet::new(),
            transition_counts: HashMap::new(),
            emissions: HashMap::new(),
        }
    }

    /// Feed a chunk of mono audio samples for this stem.
    ///
    /// Chunks may be any size — windows are extracted correctly across chunk
    /// boundaries via the leftover buffer.
    /// INV-SCB-1: integer division matches batch's `n_windows = len / window_samples`.
    pub fn push_chunk(&mut self, samples: &[f32], mfcc: &mut MfccAnalyzer) {
        self.leftover.extend_from_slice(samples);

        while self.leftover.len() >= self.window_samples {
            let window = self.leftover[..self.window_samples].to_vec();
            self.process_window(&window, mfcc);
            self.leftover.drain(..self.window_samples);
        }
        // Remainder (< window_samples) kept for next push_chunk call.
        // At finish(), it is silently discarded — matching batch's tail-drop.
    }

    /// Process exactly one 100ms window.
    /// All computations mirror their batch counterparts referenced inline.
    fn process_window(&mut self, window: &[f32], mfcc: &mut MfccAnalyzer) {
        // ── 1. Per-window RMS (mirrors build_windowed_stem builder.rs:87-92) ──
        let sum_sq: f32 = window.iter().map(|s| s * s).sum();
        let rms_db = if sum_sq > 1e-30 {
            10.0 * (sum_sq / window.len() as f32).log10()
        } else {
            -144.0
        };

        // ── 2. State classification (single source: builder::classify_for_stem) ──
        let curr_state: &'static str = classify_for_stem(self.stem_type, rms_db, self.td);

        // ── 3. Track observed states (INV-SCB-4: observed-only, matching
        //       from_sequence's BTreeSet over the FULL sequence incl. final window) ──
        self.observed_states.insert(curr_state);

        // ── 4. Transition bigram (mirrors from_sequence's sequence.windows(2) loop) ──
        if let Some(prev) = self.prev_state {
            *self
                .transition_counts
                .entry((prev, curr_state))
                .or_insert(0) += 1;
        }
        self.prev_state = Some(curr_state);

        // ── 5. MFCC (mirrors build_windowed_stem builder.rs:97) ──
        let window_mfcc = mfcc.compute(window);

        // ── 6. StateFeatures (mirrors extract_features_sequence reader.rs:60-79) ──
        let feat = StateFeatures::with_mfcc(
            rms_db,
            self.prev_rms_db, // rms_delta = rms_db - prev_rms_db
            self.td,
            self.base_attrs.spectral_centroid,
            self.base_attrs.spectral_flatness,
            window_mfcc,
        );
        self.prev_rms_db = rms_db;

        // ── 7. Emission accumulation (mirrors StemMarkovModel::train inference.rs:56-61) ──
        self.emissions
            .entry(curr_state)
            .or_insert_with(|| EmissionHistogram::new(curr_state))
            .observe(&feat);
    }

    /// Consume the builder and produce the equivalent StemMarkovModel.
    ///
    /// Produces the same result as:
    ///   `build_windowed_stem → extract_state/features_sequence → StemMarkovModel::train`
    /// for a single session, with n_sessions = 1.
    ///
    /// The leftover buffer (< one 100ms window) is discarded here,
    /// matching batch's `n_windows = len / window_samples` tail-drop.
    pub fn finish(self) -> StemMarkovModel {
        // ── Build states vector: observed-only, sorted alphabetically ──
        // Mirrors from_sequence's BTreeSet collection (model.rs:28-34).
        // observed_states is a BTreeSet, so iteration is already sorted.
        let states: Vec<String> = self.observed_states.iter().map(|s| s.to_string()).collect();

        let n = states.len();

        if n == 0 {
            // No windows processed — mirrors from_sequence's n==0 early return.
            return StemMarkovModel {
                stem_type: self.stem_type.to_string(),
                transitions: TransitionMatrix {
                    states: vec![],
                    counts: vec![],
                    probs: vec![],
                    n_transitions: 0,
                },
                emissions: HashMap::new(),
                n_sessions: 1,
            };
        }

        // ── Build index map over observed states (mirrors from_sequence:47-51) ──
        let idx: HashMap<&str, usize> = states
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();

        // ── Populate counts matrix from accumulated bigrams ──
        // Invariant: both `from` and `to` in transition_counts are &'static str
        // values produced by classify_for_stem, so both are in observed_states
        // and therefore in idx. A missing key here would be a bug in process_window.
        let mut counts = vec![vec![0usize; n]; n];
        let mut n_transitions = 0usize;
        for ((from, to), count) in &self.transition_counts {
            let fi = idx[*from];
            let ti = idx[*to];
            counts[fi][ti] += count;
            n_transitions += count;
        }

        // ── Normalize rows: single source — calls pub(crate) model::normalize_rows ──
        let probs = normalize_rows(&counts);

        let transitions = TransitionMatrix {
            states,
            counts,
            probs,
            n_transitions,
        };

        // ── Convert emissions to String-keyed HashMap (StemMarkovModel uses String) ──
        let emissions: HashMap<String, EmissionHistogram> = self
            .emissions
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        StemMarkovModel {
            stem_type: self.stem_type.to_string(),
            transitions,
            emissions,
            n_sessions: 1,
        }
    }
}

// ══════════════════════════════════════════════════════════════════
// Stage 2: StreamingCorpusBuilder — 5-stem orchestrator
// ══════════════════════════════════════════════════════════════════

/// Canonical stem index mapping — matches builder.rs:148-162 stem_pairs and
/// store.rs:13 STEM_TYPES exactly.
/// 0=voice, 1=drums, 2=bass, 3=harmonics, 4=ambience.
pub const STEM_NAMES: [&str; 5] = ["voice", "drums", "bass", "harmonics", "ambience"];

/// 5-stem streaming orchestrator.
///
/// Wraps 5 PerStemBuilders sharing one MfccAnalyzer.
/// `finish(preset_id)` produces a PresetMarkovModel bit-identical to:
///   `build_timeline(...) → CorpusEnvelope → PresetMarkovModel::new(id).train(&envelope)`
///
/// Per-stem audio is pushed independently via `push_chunk_for_stem(stem_idx, &[f32])`.
/// Stems advance at their own pace — no lockstep required.
pub struct StreamingCorpusBuilder {
    /// voice=0, drums=1, bass=2, harmonics=3, ambience=4
    builders: [PerStemBuilder; 5],
    /// Shared across all 5 stems — stateless per window (proven Stage 1).
    mfcc: MfccAnalyzer,
}

impl StreamingCorpusBuilder {
    /// Construct from the same inputs batch `build_timeline` receives.
    ///
    /// Derives all 5 base_attrs and the shared base_risk upfront — EXACTLY as
    /// builder.rs:169-194. Equivalence is line-verifiable:
    ///
    /// builder.rs field                      streaming.rs (m = features_for(name, features))
    /// ─────────────────────────────────── ─────────────────────────────────────────────────
    /// rms_db: features_for(n,f).rms_db    m.rms_db
    /// crest_factor_db: features_for(n,f)  m.crest_factor_db
    /// transient_density: *td              m.transient_density  (td ≡ m.transient_density)
    /// spectral_centroid: …_hz             m.spectral_centroid_hz
    /// lufs_integrated: pre_analysis.…     pre_analysis.integrated_lufs  (global, all stems)
    /// spectral_flatness: features_for(…)  m.spectral_flatness
    ///
    /// base_risk identical for all 5 stems (no stem-specific zone flags):
    ///   artifact_risk  = 0.0  (always)
    ///   sibilance_risk = zone_cymbal_harsh ? 0.7 : 0.0
    ///   phase_issue    = zone_phase_issue  ? 0.8 : 0.0
    ///   sub_rumble     = zone_sub_rumble   ? 1.0 : 0.0
    pub fn new(features: &StemFeatures, pre_analysis: &PreAnalysisData, sample_rate: u32) -> Self {
        // Mirrors builder.rs:177-194 — identical flag mapping, identical float literals.
        let base_risk = RiskFlags {
            artifact_risk: 0.0,
            sibilance_risk: if pre_analysis.zone_flags.zone_cymbal_harsh {
                0.7
            } else {
                0.0
            },
            phase_issue: if pre_analysis.zone_flags.zone_phase_issue {
                0.8
            } else {
                0.0
            },
            sub_rumble: if pre_analysis.zone_flags.zone_sub_rumble {
                1.0
            } else {
                0.0
            },
        };

        // Mirrors builder.rs:166-208: one PerStemBuilder per stem, same order.
        // Array::map iterates in index order — STEM_NAMES[0..4] == stem_pairs[0..4].
        let builders = STEM_NAMES.map(|name| {
            let m = features_for(name, features); // single source of truth: builder::features_for
            let base_attrs = EnrichedAttributes {
                rms_db: m.rms_db,
                crest_factor_db: m.crest_factor_db,
                transient_density: m.transient_density, // td for PerStemBuilder = same value
                spectral_centroid: m.spectral_centroid_hz,
                lufs_integrated: pre_analysis.integrated_lufs, // global — same for all 5 stems
                spectral_flatness: m.spectral_flatness,
            };
            // m.transient_density == *td in builder.rs stem_pairs (identical derivation)
            PerStemBuilder::new(
                name,
                base_attrs,
                base_risk.clone(),
                m.transient_density,
                sample_rate,
            )
        });

        Self {
            builders,
            mfcc: MfccAnalyzer::new(),
        }
    }

    /// Push a chunk of audio for one stem.
    ///
    /// `stem_idx`: 0=voice, 1=drums, 2=bass, 3=harmonics, 4=ambience (STEM_NAMES order).
    ///
    /// Borrow-checker: `self.builders[stem_idx]` borrows the `builders` field via
    /// IndexMut, and `&mut self.mfcc` borrows the `mfcc` field. These are disjoint
    /// named struct fields — Rust NLL allows simultaneous mutable borrows of different
    /// fields. No split_at_mut needed.
    pub fn push_chunk_for_stem(&mut self, stem_idx: usize, samples: &[f32]) {
        self.builders[stem_idx].push_chunk(samples, &mut self.mfcc);
    }

    /// Consume the orchestrator and produce a PresetMarkovModel.
    ///
    /// Equivalent output to:
    ///   `build_timeline → CorpusEnvelope → PresetMarkovModel::new(id).train(&envelope)`
    ///
    /// HashMap keys are canonical stem name strings matching store.rs:40's insertion keys.
    pub fn finish(self, preset_id: &str) -> PresetMarkovModel {
        let stems: HashMap<String, StemMarkovModel> = self
            .builders
            .into_iter()
            .zip(STEM_NAMES.iter())
            .map(|(builder, &name)| (name.to_string(), builder.finish()))
            .collect();

        PresetMarkovModel {
            preset_id: preset_id.to_string(),
            stems,
        }
    }
}

// ══════════════════════════════════════════════════════════════════
// ORACLE — bit-identical proof against batch path
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        builder::build_windowed_stem,
        contract::{CorpusEnvelope, EnrichedAttributes, RiskFlags, StemTimeline},
        inference::StemMarkovModel,
        mfcc::MfccAnalyzer,
        reader::{extract_features_sequence, extract_state_sequence},
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Reference StemMarkovModel via the BATCH path.
    /// Mirrors corpus_node.rs exactly:
    ///   build_windowed_stem → extract_state/features_sequence → StemMarkovModel::train
    fn batch_stem_model(
        signal: &[f32],
        stem_type: &'static str,
        base_attrs: &EnrichedAttributes,
        base_risk: &RiskFlags,
        td: f32,
        sample_rate: u32,
    ) -> StemMarkovModel {
        let session_id = "test-oracle";
        let mut mfcc = MfccAnalyzer::new();

        let events = build_windowed_stem(
            signal,
            stem_type,
            session_id,
            td,
            base_attrs,
            base_risk,
            sample_rate,
            &mut mfcc,
        );

        let envelope = CorpusEnvelope {
            protocol_version: "900".to_string(),
            session_id: session_id.to_string(),
            stems: vec![StemTimeline {
                stem_type: stem_type.to_string(),
                events,
            }],
        };

        let states = extract_state_sequence(&envelope, stem_type);
        let features = extract_features_sequence(&envelope, stem_type);

        let mut model = StemMarkovModel::new(stem_type);
        model.train(&states, &features);
        model
    }

    /// StemMarkovModel via the STREAMING path at a given chunk size.
    fn streaming_stem_model(
        signal: &[f32],
        stem_type: &'static str,
        base_attrs: EnrichedAttributes,
        base_risk: RiskFlags,
        td: f32,
        sample_rate: u32,
        chunk_size: usize,
    ) -> StemMarkovModel {
        let mut mfcc = MfccAnalyzer::new();
        let mut builder = PerStemBuilder::new(stem_type, base_attrs, base_risk, td, sample_rate);
        for chunk in signal.chunks(chunk_size) {
            builder.push_chunk(chunk, &mut mfcc);
        }
        builder.finish()
    }

    /// Assert two StemMarkovModels are bit-identical on counts and emission bins.
    /// probs is a deterministic function of counts — identical counts ↔ identical probs.
    fn assert_models_identical(batch: &StemMarkovModel, streaming: &StemMarkovModel, label: &str) {
        assert_eq!(
            batch.transitions.states, streaming.transitions.states,
            "{label}: states mismatch (batch={:?}, streaming={:?})",
            batch.transitions.states, streaming.transitions.states,
        );
        assert_eq!(
            batch.transitions.n_transitions, streaming.transitions.n_transitions,
            "{label}: n_transitions mismatch"
        );
        assert_eq!(
            batch.transitions.counts, streaming.transitions.counts,
            "{label}: transition counts mismatch"
        );
        assert_eq!(
            batch.transitions.probs, streaming.transitions.probs,
            "{label}: transition probs mismatch"
        );
        assert_eq!(
            batch.emissions.len(),
            streaming.emissions.len(),
            "{label}: emission state count mismatch (batch={} streaming={})",
            batch.emissions.len(),
            streaming.emissions.len(),
        );
        for (state, bh) in &batch.emissions {
            let sh = streaming
                .emissions
                .get(state)
                .unwrap_or_else(|| panic!("{label}: streaming missing emission for '{state}'"));
            assert_eq!(
                bh.rms_db, sh.rms_db,
                "{label}: rms_db bins mismatch '{state}'"
            );
            assert_eq!(
                bh.rms_delta, sh.rms_delta,
                "{label}: rms_delta bins mismatch '{state}'"
            );
            assert_eq!(
                bh.transient_density, sh.transient_density,
                "{label}: transient_density mismatch '{state}'"
            );
            assert_eq!(
                bh.spectral_centroid, sh.spectral_centroid,
                "{label}: spectral_centroid mismatch '{state}'"
            );
            assert_eq!(
                bh.spectral_flatness, sh.spectral_flatness,
                "{label}: spectral_flatness mismatch '{state}'"
            );
            assert_eq!(
                bh.mfcc_bins, sh.mfcc_bins,
                "{label}: mfcc_bins mismatch '{state}'"
            );
            assert_eq!(bh.total, sh.total, "{label}: total mismatch '{state}'");
        }
    }

    fn make_attrs() -> EnrichedAttributes {
        EnrichedAttributes {
            rms_db: -20.0,
            crest_factor_db: 12.0,
            transient_density: 0.1,
            spectral_centroid: 1200.0,
            lufs_integrated: -16.0,
            spectral_flatness: 0.3,
        }
    }

    fn make_risk() -> RiskFlags {
        RiskFlags {
            artifact_risk: 0.0,
            sibilance_risk: 0.0,
            phase_issue: 0.0,
            sub_rumble: 0.0,
        }
    }

    /// 5-second signal: loud first half (→ "vowel"/"sustain"), silent second half.
    /// 50 windows of 100ms @ 48kHz. Exercises multiple states and transitions.
    fn make_two_state_signal(sample_rate: u32) -> Vec<f32> {
        let total = sample_rate as usize * 5;
        let half = total / 2;
        let mut sig = vec![0.0f32; total];
        for s in sig.iter_mut().take(half) {
            *s = 0.5;
        }
        sig
    }

    // ── Core oracle tests ─────────────────────────────────────────────────────

    #[test]
    fn perstem_voice_chunk_777() {
        let sr = 48_000u32;
        let sig = make_two_state_signal(sr);
        let batch = batch_stem_model(&sig, "voice", &make_attrs(), &make_risk(), 0.05, sr);
        let streaming =
            streaming_stem_model(&sig, "voice", make_attrs(), make_risk(), 0.05, sr, 777);
        assert_models_identical(&batch, &streaming, "voice/chunk=777");
    }

    #[test]
    fn perstem_drums_chunk_exact_window() {
        let sr = 48_000u32;
        let sig = make_two_state_signal(sr);
        let ws = (sr as f32 * 0.1) as usize;
        // chunk == window: no leftover boundary ever crossed
        let batch = batch_stem_model(&sig, "drums", &make_attrs(), &make_risk(), 0.4, sr);
        let streaming = streaming_stem_model(&sig, "drums", make_attrs(), make_risk(), 0.4, sr, ws);
        assert_models_identical(&batch, &streaming, "drums/chunk=window");
    }

    #[test]
    fn perstem_bass_chunk_1() {
        let sr = 48_000u32;
        let sig = make_two_state_signal(sr);
        // chunk=1: every byte-boundary exercised
        let batch = batch_stem_model(&sig, "bass", &make_attrs(), &make_risk(), 0.1, sr);
        let streaming = streaming_stem_model(&sig, "bass", make_attrs(), make_risk(), 0.1, sr, 1);
        assert_models_identical(&batch, &streaming, "bass/chunk=1");
    }

    #[test]
    fn perstem_harmonics_chunk_larger_than_signal() {
        let sr = 48_000u32;
        let sig = make_two_state_signal(sr);
        // single chunk > signal: no boundary crossings at all
        let batch = batch_stem_model(&sig, "harmonics", &make_attrs(), &make_risk(), 0.1, sr);
        let streaming = streaming_stem_model(
            &sig,
            "harmonics",
            make_attrs(),
            make_risk(),
            0.1,
            sr,
            sig.len() + 1,
        );
        assert_models_identical(&batch, &streaming, "harmonics/chunk=signal+1");
    }

    #[test]
    fn perstem_chunk_size_invariant_ambience() {
        let sr = 48_000u32;
        let sig = make_two_state_signal(sr);
        let batch = batch_stem_model(&sig, "ambience", &make_attrs(), &make_risk(), 0.2, sr);
        for chunk_size in [1usize, 777, 4800, 99999] {
            let s = streaming_stem_model(
                &sig,
                "ambience",
                make_attrs(),
                make_risk(),
                0.2,
                sr,
                chunk_size,
            );
            assert_models_identical(&batch, &s, &format!("ambience/chunk={chunk_size}"));
        }
    }

    #[test]
    fn perstem_tail_drop_matches_batch() {
        // Signal length NOT a multiple of window_samples.
        // Both batch and streaming must drop the partial tail.
        let sr = 48_000u32;
        let ws = (sr as f32 * 0.1) as usize; // 4800
                                             // 3 full windows + 2400 leftover samples (half a window)
        let n_samples = ws * 3 + 2400;
        let mut sig = vec![0.5f32; n_samples];
        // Window 2 silent → state transition: vowel→silence→vowel
        for s in sig.iter_mut().take(ws * 2).skip(ws) {
            *s = 0.0;
        }

        let batch = batch_stem_model(&sig, "voice", &make_attrs(), &make_risk(), 0.1, sr);
        let streaming =
            streaming_stem_model(&sig, "voice", make_attrs(), make_risk(), 0.1, sr, 777);
        // 3 windows → 2 bigrams
        assert_eq!(
            batch.transitions.n_transitions, 2,
            "batch should have 2 transitions for 3 windows"
        );
        assert_models_identical(&batch, &streaming, "tail_drop/3_windows");
    }

    // ── THE OBSERVED-ONLY ORACLE (the fix that the original design would fail) ──

    /// Signal that hits ONLY 2 of the 5 voice states: "vowel" and "silence".
    /// No "breath", "consonant", or "tail" are ever produced.
    ///
    /// batch.transitions.states.len() == 2 (observed-only from from_sequence).
    /// streaming finish() must ALSO produce states.len() == 2, not 5.
    /// This is the exact case the original design (full-vocab) would have failed.
    #[test]
    fn perstem_observed_only_states_matches_batch() {
        let sr = 48_000u32;
        let ws = (sr as f32 * 0.1) as usize; // 4800 samples

        // 6 windows: alternating loud (→ "vowel") and silent (→ "silence").
        // rms_db of 0.5 amplitude ≈ -6 dBFS  → vowel (rms > -30)
        // rms_db of 0.0 amplitude = -144.0   → silence (rms < -50)
        let mut sig = vec![0.0f32; ws * 6];
        for w in 0..6 {
            if w % 2 == 0 {
                for i in 0..ws {
                    sig[w * ws + i] = 0.5; // loud → "vowel"
                }
            }
            // odd windows stay 0.0 → "silence"
        }
        // td=0.0 ensures we never trigger "consonant" (requires td > 0.15)

        let batch = batch_stem_model(&sig, "voice", &make_attrs(), &make_risk(), 0.0, sr);

        assert_eq!(
            batch.transitions.states.len(),
            2,
            "batch must have exactly 2 states (silence, vowel); got {:?}",
            batch.transitions.states
        );
        assert_eq!(
            batch.transitions.states,
            vec!["silence".to_string(), "vowel".to_string()],
            "batch states must be alphabetically sorted observed-only"
        );

        // Streaming at multiple chunk sizes — all must produce len==2, not 5.
        for chunk_size in [1usize, 777, ws, sig.len() + 1] {
            let s = streaming_stem_model(
                &sig,
                "voice",
                make_attrs(),
                make_risk(),
                0.0,
                sr,
                chunk_size,
            );
            assert_eq!(
                s.transitions.states.len(),
                2,
                "streaming chunk={chunk_size}: states.len() must be 2, got {:?}",
                s.transitions.states
            );
            assert_models_identical(&batch, &s, &format!("observed_only/chunk={chunk_size}"));
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Stage 2 oracle — 5-stem StreamingCorpusBuilder bit-identical to batch
    // ══════════════════════════════════════════════════════════════════

    use crate::{builder::build_timeline, store::PresetMarkovModel};
    use lineos_types::{
        analysis::{StemFeatures, StemMetrics},
        pre_analysis::{PreAnalysisData, ZoneActivationFlags},
    };

    /// Batch reference: build_timeline → envelope → PresetMarkovModel::train.
    /// Mirrors corpus_node.rs exactly.
    fn batch_preset_model(
        features: &StemFeatures,
        signals: [&[f32]; 5],
        pre_analysis: &PreAnalysisData,
        sample_rate: u32,
    ) -> PresetMarkovModel {
        let [voice, drums, bass, harmonics, ambience] = signals;
        let envelope = build_timeline(
            features,
            voice,
            drums,
            bass,
            harmonics,
            ambience,
            pre_analysis,
            "test-oracle",
            sample_rate,
            "warm",
        );
        let mut preset = PresetMarkovModel::new("test");
        preset.train(&envelope);
        preset
    }

    /// Streaming path at a given chunk size.
    fn streaming_preset_model(
        features: &StemFeatures,
        signals: [&[f32]; 5],
        pre_analysis: &PreAnalysisData,
        sample_rate: u32,
        chunk_size: usize,
    ) -> PresetMarkovModel {
        let mut builder = StreamingCorpusBuilder::new(features, pre_analysis, sample_rate);
        for (idx, signal) in signals.iter().enumerate() {
            for chunk in signal.chunks(chunk_size) {
                builder.push_chunk_for_stem(idx, chunk);
            }
        }
        builder.finish("test")
    }

    /// Assert two PresetMarkovModels are bit-identical across all 5 stems.
    fn assert_presets_identical(batch: &PresetMarkovModel, streaming: &PresetMarkovModel) {
        assert_eq!(batch.preset_id, streaming.preset_id, "preset_id mismatch");
        assert_eq!(
            batch.stems.len(),
            streaming.stems.len(),
            "stem count mismatch (batch={} streaming={})",
            batch.stems.len(),
            streaming.stems.len(),
        );
        for name in STEM_NAMES {
            let b = batch
                .stems
                .get(name)
                .unwrap_or_else(|| panic!("batch missing stem '{name}'"));
            let s = streaming
                .stems
                .get(name)
                .unwrap_or_else(|| panic!("streaming missing stem '{name}'"));
            assert_models_identical(b, s, &format!("5stem/{name}"));
        }
    }

    /// StemFeatures with DISTINCT scalars per stem.
    /// If push_chunk_for_stem mis-routes base_attrs (e.g. voice attrs on drums),
    /// emission histogram bins will differ (spectral_centroid, spectral_flatness,
    /// rms_db all feed EmissionHistogram::observe) and the oracle catches it.
    fn make_distinct_features() -> StemFeatures {
        let mk = |rms_db: f32, centroid: f32, flatness: f32, td: f32| StemMetrics {
            rms_db,
            spectral_centroid_hz: centroid,
            spectral_flatness: flatness,
            transient_density: td,
            crest_factor_db: 10.0 + rms_db.abs() * 0.1,
            ..StemMetrics::default()
        };
        StemFeatures {
            voice: mk(-18.0, 1800.0, 0.15, 0.05),
            drums: mk(-12.0, 3500.0, 0.40, 0.45),
            bass: mk(-22.0, 200.0, 0.05, 0.08),
            harmonics: mk(-25.0, 2200.0, 0.30, 0.10),
            ambience: mk(-35.0, 4000.0, 0.60, 0.20),
            mix: Default::default(),
        }
    }

    fn make_pre_analysis_with_flags() -> PreAnalysisData {
        PreAnalysisData {
            integrated_lufs: -14.0,
            zone_flags: ZoneActivationFlags {
                zone_cymbal_harsh: true,
                zone_sub_rumble: true,
                ..ZoneActivationFlags::default()
            },
            ..PreAnalysisData::silent()
        }
    }

    fn make_5stem_signals(sample_rate: u32) -> [Vec<f32>; 5] {
        let make = |amplitude: f32| {
            let total = sample_rate as usize * 3;
            let half = total / 2;
            let mut sig = vec![0.0f32; total];
            for s in sig.iter_mut().take(half) {
                *s = amplitude;
            }
            sig
        };
        [make(0.5), make(0.8), make(0.3), make(0.15), make(0.05)]
    }

    #[test]
    fn streaming_corpus_5stem_matches_batch_chunk_777() {
        let sr = 48_000u32;
        let features = make_distinct_features();
        let pre = make_pre_analysis_with_flags();
        let sigs = make_5stem_signals(sr);
        let sig_refs: [&[f32]; 5] = [&sigs[0], &sigs[1], &sigs[2], &sigs[3], &sigs[4]];

        let batch = batch_preset_model(&features, sig_refs, &pre, sr);
        let streaming = streaming_preset_model(&features, sig_refs, &pre, sr, 777);
        assert_presets_identical(&batch, &streaming);
    }

    #[test]
    fn streaming_corpus_5stem_chunk_size_invariant() {
        let sr = 48_000u32;
        let features = make_distinct_features();
        let pre = make_pre_analysis_with_flags();
        let sigs = make_5stem_signals(sr);
        let sig_refs: [&[f32]; 5] = [&sigs[0], &sigs[1], &sigs[2], &sigs[3], &sigs[4]];

        let batch = batch_preset_model(&features, sig_refs, &pre, sr);
        for chunk_size in [1usize, 777, 4800, 999_999] {
            let s = streaming_preset_model(&features, sig_refs, &pre, sr, chunk_size);
            assert_presets_identical(&batch, &s);
        }
    }

    #[test]
    fn streaming_corpus_routing_guard() {
        let sr = 48_000u32;
        let features = make_distinct_features();
        let pre = make_pre_analysis_with_flags();
        let sigs = make_5stem_signals(sr);
        let sig_refs: [&[f32]; 5] = [&sigs[0], &sigs[1], &sigs[2], &sigs[3], &sigs[4]];

        let batch = batch_preset_model(&features, sig_refs, &pre, sr);
        let streaming = streaming_preset_model(&features, sig_refs, &pre, sr, 777);

        for (idx, name) in STEM_NAMES.iter().enumerate() {
            let b = batch.stems.get(*name).unwrap();
            let s = streaming.stems.get(*name).unwrap();
            assert_models_identical(b, s, &format!("routing/{name}(idx={idx})"));
        }
    }
}
