//! dsp_node — pre-analysis, autotune, AetherBridge, Corpus, DspAdapter.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P4
//!
//! Only `build_intent_and_config`/`IntentConfig` moved here 21/09 —
//! the engine's own need. `run()`/`DspOutput` (the Music DSP-graph
//! path, DspAdapter-dependent) stay in m0-daemon's domain/nodes/
//! dsp_node.rs, called only by run_dsp_internal, and call this via
//! full path.

use lineos_types::{LoudnessTarget, MasteringIntent, StemFeatures};

/// Configuration built from a master
/// request — intent, aether config, and
/// the certificate provenance objects.
/// Shared between the standard dsp_node
/// path and the Episode streaming path.
pub struct IntentConfig {
    pub intent: MasteringIntent,
    pub aether_req: aether_bridge::AetherRequest,
    pub dsp_config: integration::config::DspConfig,
    pub proof_log: integration::proof_log::ProofLog,
    pub persona_config: aether::personas::config::PersonaConfig,
}

/// Build the MasteringIntent and Aether DSP
/// config for a master request. Does NOT run
/// corpus or the DSP graph — only constructs
/// the configuration objects that both the
/// standard and Episode paths need before
/// rendering.
#[allow(clippy::too_many_arguments)]
pub fn build_intent_and_config(
    preset_id: &str,
    flavour_id: &str,
    req_tone: Option<f32>,
    req_dynamics: Option<f32>,
    chaos_seed: Option<u64>,
    project_id: Option<&str>,
    track_id: Option<&str>,
    target_lufs: Option<f32>,
    streaming_features: &StemFeatures,
    pre_analysis: &lineos_types::pre_analysis::PreAnalysisData,
) -> Result<IntentConfig, String> {
    // Το target ΠΡΟΚΥΠΤΕΙ από το preset, δεν χτίζεται
    // από hardcoded τιμές.
    //
    // ΗΤΑΝ: max_true_peak_db: -1.0 σταθερό, platform
    // "default". Ο ACX ορίζει -3.0· η τιμή ταξίδευε
    // σωστά από το DeliverySpec μέχρι εδώ και πετιόταν
    // σε αυτή τη γραμμή.
    //
    // Το target_lufs ΔΕΧΕΤΑΙ override (το cohesion
    // pre-pass του album path το χρειάζεται)· το ceiling
    // ΟΧΙ — είναι απαίτηση της πλατφόρμας, όχι προτίμηση.
    let preset_target =
        lineos_types::config::LoudnessTarget::from_preset(preset_id);

    let intent = MasteringIntent {
        target: LoudnessTarget {
            target_lufs: target_lufs
                .unwrap_or(preset_target.target_lufs),
            max_true_peak_db: preset_target.max_true_peak_db,
            max_lra_lu: preset_target.max_lra_lu,
            platform: preset_target.platform.clone(),
        },
        preset_name: preset_id.to_string(),
        stem_mode: false,
        target_makeup_db: 0.0,
        limiter_blend_release_ms: {
            // Control Plane: lerp here,
            // NOT in DSP engine.
            // 0.0 (Smooth) -> 200ms
            // 0.5 (default) -> 105ms
            // ⚠ ΔΙΟΡΘΩΣΗ 2026-08-25: prior comment said "95ms".
            // Formula 200.0 - (0.5 × 190.0) = 105ms. Endpoints correct (0→200✓, 1→10✓).
            // 1.0 (Punchy) -> 10ms
            let d = req_dynamics.unwrap_or(0.5).clamp(0.0, 1.0);
            200.0 - (d * 190.0)
        },
        // ΓΙΑΤΙ 2.0: το branch στο dsp/mod.rs ψαλιδίζει το
        // correction ώστε ο limiter να μην κόβει πάνω από
        // αυτό — θυσιάζει στάθμη αντί για δυναμική. Το 6.0
        // επέτρεπε μετρημένη ζημιά: crest 0-2k −66% στα
        // 5-6 dB reduction (V1↔V3 probe, 2026-08-15).
        // Τυφλή ισοσταθμισμένη ακρόαση σε τρία υλικά:
        //   2.03 dB → οριακά διακριτό (γυμνό acoustic)
        //   5.23 dB → «σίγουρα, με διαφορά» (kick snap,
        //             φωνή στον χώρο)
        //   6.00 dB → συγκαλυμμένο μόνο σε πυκνό synthetic
        // Η ασυμμετρία κάνει το χαμηλό clamp ασφαλές και
        // για τους δύο πληθυσμούς: ό,τι άντεχε το κόψιμο
        // χάνει μόνο LUFS (που το platform normalization
        // επιστρέφει), ό,τι δεν το άντεχε γλιτώνει.
        // ΚΟΣΤΟΣ: pre-mastered υλικό με μεγάλο peak_over
        // βγαίνει ως −19 LUFS σε non-normalized διαδρομές.
        // Το −1 dBTP ceiling ΔΕΝ αγγίζεται.
        max_limiter_gr_db: 2.0,
    };

    use crate::content_type::{ContentType, ContentTypeExt};
    let mapped_persona = crate::dsp_pipeline_helpers::map_flavour_to_persona(flavour_id);
    let aether_req = aether_bridge::AetherRequest {
        persona_id: Some(mapped_persona.to_string()),
        tone: req_tone,
        dynamics: req_dynamics,
        ambience: None,
        chaos_seed,
        project_id: project_id.map(|s| s.to_string()),
        track_id: track_id.map(|s| s.to_string()),
        preset_name: Some(preset_id.to_string()),
        content_type: ContentType::from_preset(preset_id),
    };
    let (dsp_config, proof_log, persona_config) =
        aether_bridge::build_dsp_config(&aether_req, streaming_features, Some(pre_analysis))
            .map_err(|e| format!("AetherBridge error: {}", e))?;

    Ok(IntentConfig {
        intent,
        aether_req,
        dsp_config,
        proof_log,
        persona_config,
    })
}
