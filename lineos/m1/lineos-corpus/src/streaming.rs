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
    builder::classify_for_stem, // single source of truth — no local copy
    contract::{EnrichedAttributes, RiskFlags},
    features::StateFeatures,
    inference::StemMarkovModel,
    mfcc::MfccAnalyzer,
    model::{normalize_rows, EmissionHistogram, TransitionMatrix}, // normalize_rows: single source
};

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
}
