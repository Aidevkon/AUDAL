use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::{MasterRequest, MixLevels};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use xaak::repo::DspState;

fn render_with_mix(
    input_path: &Path,
    name: &str,
    mix: Option<MixLevels>,
    out_dir: &Path,
) -> Option<(String, f32, f32, bool, bool, bool)> {
    let state_tmp = tempfile::TempDir::new().unwrap();
    let masters_dir = tempfile::TempDir::new().unwrap();
    let req = MasterRequest {
        audio_path: input_path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("w17".to_string()),
        track_id: Some(name.to_string()),
        mix_levels: mix,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: None,
    };

    let (blob, _, _pcm, _, _, artifacts) = run_dsp(
        &req,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        format!("w17-{}", name),
        state_tmp.path().to_str().unwrap(),
        masters_dir.path().to_str().unwrap(),
    )
    .expect(&format!("run_dsp failed for {}", name));

    if let Some(master_path) = artifacts.persisted_master {
        let dest = out_dir.join(format!("{}.flac", name));
        fs::copy(&master_path, &dest).unwrap();
    }

    Some((
        name.to_string(),
        blob.loudness().expect("test expects Certified").integrated_lufs,
        blob.loudness().expect("test expects Certified").true_peak_dbtp,
        blob.loudness().expect("test expects Certified").too_quiet_for_mobile,
        blob.loudness().expect("test expects Certified").spotify_compliant,
        blob.loudness().expect("test expects Certified").apple_podcasts_compliant,
    ))
}

#[test]
#[ignore = "θέλει /tmp/w9/podcast_realistic.wav (F-072, untracked). ΑΓΝΩΣΤΟΣ ΤΡΟΠΟΣ ΑΝΑΚΑΤΑΣΚΕΥΗΣ — συνταγή ΜΟΝΟ περιγραφική στο 52e2a31, το rebuild δίνει ΑΛΛΟ αρχείο. Το ξυπνά: scripts/audio_wire.sh · scripts/run-ignored.sh"]
fn w17_mix_balance() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    assert!(input_path.exists(), "Missing fixture: {} — συνταγή ΜΟΝΟ περιγραφική στο 52e2a31, πηγές ακαταγράφητες (F-072) — το rebuild δίνει ΑΛΛΟ αρχείο", input_path.display());

    let out_dir = Path::new("/tmp/w17");
    fs::create_dir_all(out_dir).unwrap();

    let configs: Vec<(&str, Option<MixLevels>)> = vec![
        ("A_current", Some(MixLevels { voice: 0.8, drums: 0.6, bass: 0.7, harmonics: 0.5, ambience: 0.4 })),
        ("B_equalized", Some(MixLevels { voice: 0.8, drums: 0.6, bass: 0.9, harmonics: 1.0, ambience: 1.0 })),
        ("C_voice_down", Some(MixLevels { voice: 0.5, drums: 0.6, bass: 0.7, harmonics: 0.5, ambience: 0.4 })),
        ("D_flat", None),
    ];

    println!("\n{:<18} | {:>8} | {:>6} | {:>5} | {:>7} | {:>9}",
        "name", "lufs", "peak", "quiet", "spotify", "apple_pod");
    println!("{}", "-".repeat(72));

    for (name, mix) in configs {
        if let Some((n, lufs, peak, quiet, spotify, apple)) = render_with_mix(input_path, name, mix, out_dir) {
            println!("{:<18} | {:>8.2} | {:>6.2} | {:>5} | {:>7} | {:>9}",
                n, lufs, peak, quiet, spotify, apple);
        }
    }

    // HTML player
    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>W17 Mix Balance A/B</title>
    <style>
        body { font-family: sans-serif; padding: 20px; }
        .player { margin-bottom: 20px; padding: 15px; border: 1px solid #ccc; background: #f9f9f9; }
        h2 { margin-top: 0; }
        audio { width: 100%; }
    </style>
</head>
<body>
    <h1>W17 Mix Balance A/B</h1>
    <div class="player">
        <h2>A: Current (v.8 d.6 b.7 h.5 a.4)</h2>
        <audio controls loop src="A_current.flac"></audio>
    </div>
    <div class="player">
        <h2>B: Equalized (v.8 d.6 b.9 h1.0 a1.0)</h2>
        <audio controls loop src="B_equalized.flac"></audio>
    </div>
    <div class="player">
        <h2>C: Voice Down (v.5 d.6 b.7 h.5 a.4)</h2>
        <audio controls loop src="C_voice_down.flac"></audio>
    </div>
    <div class="player">
        <h2>D: Flat (all 1.0)</h2>
        <audio controls loop src="D_flat.flac"></audio>
    </div>
</body>
</html>"#;
    fs::write(out_dir.join("index.html"), html).unwrap();
}
