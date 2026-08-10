use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::{MasterRequest, MixLevels};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use xaak::repo::DspState;

fn render_boost(
    input_path: &Path,
    name: &str,
    harmonics: f32,
    ambience: f32,
    out_dir: &Path,
) -> Option<(String, f32, f32, bool)> {
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
        project_id: Some("w17_nonorm".to_string()),
        track_id: Some(name.to_string()),
        mix_levels: Some(MixLevels {
            voice: 0.5,
            drums: 0.6,
            bass: 0.7,
            harmonics,
            ambience,
        }),
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: Some(true),
    };

    let (blob, _, _pcm, _, _, artifacts) = run_dsp(
        &req,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        format!("w17-nonorm-{}", name),
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
        blob.loudness.integrated_lufs,
        blob.loudness.true_peak_dbtp,
        blob.loudness.too_quiet_for_mobile,
    ))
}

#[test]
#[ignore]
fn w17_bed_sweep() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    if !input_path.exists() {
        println!("SKIPPED: /tmp/w9/podcast_realistic.wav missing");
        return;
    }

    let out_dir = Path::new("/tmp/w17_nonorm");
    fs::create_dir_all(out_dir).unwrap();

    let configs = vec![
        ("h1", 1.0, 1.0),
        ("h2", 2.0, 2.0),
        ("h4", 4.0, 4.0),
        ("h8", 8.0, 8.0),
    ];

    println!("\n{:<10} | {:>8} | {:>6} | {:>9}",
        "name", "lufs", "peak", "too_quiet");
    println!("{}", "-".repeat(45));

    for (name, harmonics, ambience) in configs {
        if let Some((n, lufs, peak, quiet)) =
            render_boost(input_path, name, harmonics, ambience, out_dir)
        {
            println!("{:<10} | {:>8.2} | {:>6.2} | {:>9}",
                n, lufs, peak, quiet);
        }
    }

    // HTML A/B Player
    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>W17 Bed Sweep (No Normalizer Bypass)</title>
    <style>
        body { font-family: sans-serif; padding: 20px; background: #121212; color: #e0e0e0; }
        .player { margin-bottom: 15px; padding: 15px; background: #1e1e1e; border-radius: 6px; }
        h1 { color: #fff; }
        h3 { color: #bb86fc; margin: 0 0 10px 0; }
        audio { width: 100%; }
    </style>
</head>
<body>
    <h1>W17 Bed Boost Audition (Bypass Normalizer)</h1>
    <div class="player">
        <h3>h1: Harmonics 1.0, Ambience 1.0 (Voice 0.5, Bass 0.7, Drums 0.6)</h3>
        <audio controls loop src="h1.flac"></audio>
    </div>
    <div class="player">
        <h3>h2: Harmonics 2.0, Ambience 2.0</h3>
        <audio controls loop src="h2.flac"></audio>
    </div>
    <div class="player">
        <h3>h4: Harmonics 4.0, Ambience 4.0</h3>
        <audio controls loop src="h4.flac"></audio>
    </div>
    <div class="player">
        <h3>h8: Harmonics 8.0, Ambience 8.0</h3>
        <audio controls loop src="h8.flac"></audio>
    </div>
</body>
</html>"#;
    fs::write(out_dir.join("index.html"), html).unwrap();
}
