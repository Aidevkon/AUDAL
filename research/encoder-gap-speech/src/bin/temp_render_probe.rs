//! Βοηθητικό, ΓΙΑ ΑΥΤΟ ΤΟ TASK ΜΟΝΟ: πλήρης αλυσίδα render (run_dsp,
//! ΙΔΙΟ μονοπάτι με INV-DET-1) πάνω σε ένα δοκίμιο, πριν/μετά την
//! προσωρινή αλλαγή TEMP_FLATNESS_RULE_20260917 στο trunk_pass.rs.
//! ΔΕΝ αγγίζει παραγωγή το ίδιο.
//! ΧΡΗΣΗ: cargo run --release --bin temp_render_probe -- <input.wav> <out_dir_label>
use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use xaak::repo::DspState;

fn get_sha256(path: &std::path::Path) -> String {
    let output = std::process::Command::new("sha256sum").arg(path).output().expect("sha256sum failed");
    String::from_utf8_lossy(&output.stdout).split_whitespace().next().unwrap().to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input_path = args[1].clone();
    let label = args[2].clone();

    let req = MasterRequest {
        audio_path: input_path,
        preset_id: "spotify".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("temp_flatness_probe".to_string()),
        track_id: Some(label.clone()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: Some(true),
    };

    let state_tmp = tempfile::TempDir::new().unwrap();
    let out_dir = format!("/tmp/temp_render_probe_{label}");
    std::fs::create_dir_all(&out_dir).unwrap();

    let (_blob, _, _pcm, _, _, artifacts) = run_dsp(
        &req,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        format!("temp-flatness-{label}"),
        state_tmp.path().to_str().unwrap(),
        &out_dir,
    )
    .unwrap_or_else(|e| panic!("run_dsp failed for {label}: {e}"));

    let master_path = artifacts.persisted_master.expect("Master not produced");
    let sha = get_sha256(&master_path);
    println!("label={label} master_path={} sha256={sha}", master_path.display());
}
