use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use xaak::repo::DspState;

#[test]
#[ignore = "audition — writes files for listening"]
fn audition() {
    let default_input = "/home/aidevcon/Downloads/DATASET/musdb18hq/test/Ben Carrigan - We'll Talk About It All Tonight/mixture.wav";
    let dataset_path = std::env::var("AUDITION_INPUT").unwrap_or_else(|_| default_input.to_string());
    
    // Παλιά αρχεία από προηγούμενες συνεδρίες μπερδεύουν.
    // Σβήνουμε ΜΟΝΟ ό,τι γράφουμε εμείς.
    for f in ["A_source.wav", "B_master.wav", "index.html"] {
        let _ = std::fs::remove_file(format!("/tmp/audition/{f}"));
    }

    if !std::path::Path::new(&dataset_path).exists() {
        eprintln!("Missing input file: {}", dataset_path);
        return;
    }

    let start_sec: usize = std::env::var("AUDITION_START")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(30);
    let secs: usize = std::env::var("AUDITION_SECS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(10);

    let mut reader = hound::WavReader::open(&dataset_path).expect("failed to open input");
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let sr = spec.sample_rate as usize;

    let start_sample = sr * start_sec * channels;
    let end_sample = sr * (start_sec + secs) * channels;

    let all_samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            if spec.bits_per_sample == 16 {
                reader.samples::<i16>().map(|s| s.unwrap_or(0) as f32 / 32768.0).collect()
            } else if spec.bits_per_sample == 24 {
                reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / 8388608.0).collect()
            } else if spec.bits_per_sample == 32 {
                reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / 2147483648.0).collect()
            } else {
                panic!("Unsupported bits per sample");
            }
        }
    };

    let actual_start = start_sample.min(all_samples.len());
    let actual_end = end_sample.min(all_samples.len());
    if actual_end <= actual_start {
        panic!("File is too short!");
    }

    if actual_end < end_sample {
        println!("WARNING: Source file is shorter than expected. Took available samples.");
    }

    let segment = &all_samples[actual_start..actual_end];

    std::fs::create_dir_all("/tmp/audition").unwrap();
    let source_path = "/tmp/audition/A_source.wav";
    let mut w = hound::WavWriter::create(
        source_path,
        hound::WavSpec {
            channels: spec.channels,
            sample_rate: spec.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for &s in segment {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();

    let req = MasterRequest {
        audio_path: source_path.to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: None,
        track_id: Some("audition-001".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
        use_nmfd: None,
    };

    let head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));
    let state_tmp = tempfile::TempDir::new().unwrap();
    let out_dir = "/tmp/audition";

    let result = run_dsp(
        &req,
        std::time::Instant::now(),
        head_state,
        None,
        None,
        "audition".to_string(),
        state_tmp.path().to_str().unwrap(),
        out_dir,
    );
    assert!(result.is_ok(), "run_dsp failed: {:?}", result.err());

    let (stereo_blob, _, pcm, _, _, _) = result.unwrap();
    let master_path = "/tmp/audition/B_master.wav";
    let ref_path = "/tmp/audition/C_reference.wav";

    let master_sr = stereo_blob.core.sample_rate;

    // Το pcm.path() είναι ΩΜΟ PCM: f32 LE interleaved,
    // χωρίς header — γι' αυτό το hound σκάει με
    // "no RIFF tag found". Το τυλίγουμε σε WAV ώστε ο
    // browser να το παίξει.
    let raw = std::fs::read(pcm.path()).unwrap();
    let samples_f32: Vec<f32> = raw
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    let mut m_w = hound::WavWriter::create(
        master_path,
        hound::WavSpec {
            channels: 2,
            sample_rate: master_sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    ).unwrap();
    for &s in &samples_f32 {
        m_w.write_sample(s).unwrap();
    }
    m_w.finalize().unwrap();

    // AUDITION_SNAPSHOT=1 → το ΤΡΕΧΟΝ master γίνεται
    // ΚΑΙ baseline. Χωρίς αυτό, το C μένει ό,τι ήταν.
    //
    // ΓΙΑΤΙ ΡΗΤΑ ΚΑΙ ΟΧΙ ΑΥΤΟΜΑΤΑ: ένα baseline που
    // γράφεται μόνο του είναι πάντα ένα βήμα πίσω και
    // κανείς δεν ξέρει ποιου commit είναι. Έτσι το
    // ορίζεις εσύ, όταν το θέλεις.
    //
    //   AUDITION_SNAPSHOT=1 cargo test ...   ← κλείδωσε baseline
    //   cargo test ...                        ← σύγκρινε με αυτό
    if std::env::var("AUDITION_SNAPSHOT").is_ok() {
        std::fs::copy(pcm.path(), ref_path).ok();
        println!("[AUDITION] snapshot: C_reference updated");
    }

    let measure = |path: &str| {
        let mut r = hound::WavReader::open(path).unwrap();
        let sp = r.spec();
        let samps: Vec<f32> = match sp.sample_format {
            hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
            hound::SampleFormat::Int => {
                if sp.bits_per_sample == 16 {
                    r.samples::<i16>().map(|s| s.unwrap_or(0) as f32 / 32768.0).collect()
                } else if sp.bits_per_sample == 24 {
                    r.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / 8388608.0).collect()
                } else if sp.bits_per_sample == 32 {
                    r.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / 2147483648.0).collect()
                } else {
                    panic!("Unsupported bits");
                }
            }
        };
        let mut sum_sq_l = 0.0;
        let mut sum_sq_r = 0.0;
        let mut sum_sq_side = 0.0;
        let mut peak_l = 0.0_f32;
        let mut peak_r = 0.0_f32;

        let frames = samps.len() / 2;
        for i in 0..frames {
            let l = samps[i * 2];
            let r = samps[i * 2 + 1];
            sum_sq_l += l * l;
            sum_sq_r += r * r;
            let side = (l - r) * 0.5;
            sum_sq_side += side * side;
            if l.abs() > peak_l {
                peak_l = l.abs();
            }
            if r.abs() > peak_r {
                peak_r = r.abs();
            }
        }
        let rms_l = (sum_sq_l / frames as f32).sqrt();
        let rms_r = (sum_sq_r / frames as f32).sqrt();
        let side_rms = (sum_sq_side / frames as f32).sqrt();
        let total_rms = ((sum_sq_l + sum_sq_r) / (frames * 2) as f32).sqrt();
        (rms_l, rms_r, peak_l, peak_r, side_rms, total_rms)
    };

    let (s_rmsl, s_rmsr, s_pl, s_pr, s_side, s_tot) = measure(source_path);
    let (m_rmsl, m_rmsr, m_pl, m_pr, m_side, m_tot) = measure(master_path);

    let (lufs, tp) = if let m0d::blob_store::BlobVariant::Certified { loudness, .. } = &stereo_blob.variant {
        (loudness.integrated_lufs, loudness.true_peak_dbtp)
    } else {
        (-99.0, -99.0)
    };

    let survival = if s_side > 0.0 { m_side / s_side } else { 0.0 };
    let level_diff_db = if s_tot > 0.0 && m_tot > 0.0 {
        20.0 * (m_tot / s_tot).log10()
    } else {
        0.0
    };

    let source_sr = spec.sample_rate;
    if source_sr != master_sr {
        println!("[AUDITION] ⚠ sample rate: source {} · master {}", source_sr, master_sr);
        println!("[AUDITION] ⚠ ΤΟ A/B ΔΕΝ ΣΥΓΧΡΟΝΙΖΕΤΑΙ — σύγκρινε χαρακτήρα, όχι θέση");
    }

    println!("[AUDITION] source: rms_l={:.4} rms_r={:.4} peak_l={:.4} peak_r={:.4}", s_rmsl, s_rmsr, s_pl, s_pr);
    println!("                   side_rms={:.6}", s_side);
    println!("[AUDITION] master: rms_l={:.4} rms_r={:.4} peak_l={:.4} peak_r={:.4}", m_rmsl, m_rmsr, m_pl, m_pr);
    println!("                   side_rms={:.6} lufs={:.2} tp={:.2}", m_side, lufs, tp);
    println!("[AUDITION] side survival = {:.6}", survival);
    println!("[AUDITION] level diff = {:.2} dB", level_diff_db);
    println!("[AUDITION] files:");
    println!("  A: {}", source_path);
    println!("  B: {}", master_path);
    if std::path::Path::new(ref_path).exists() {
        println!("  C: {}", ref_path);
    }
    println!("[AUDITION] listen: cd /tmp/audition && python3 -m http.server 8080");
    println!("[AUDITION] baseline: AUDITION_SNAPSHOT=1 to lock current as C");

    let c_display = if std::path::Path::new(ref_path).exists() { "inline-block" } else { "none" };
    let warning_html = if source_sr != master_sr {
        format!(r#"<p style="color: red; font-weight: bold; margin: 10px;">⚠ sample rate: source {} · master {}<br>ΤΟ A/B ΔΕΝ ΣΥΓΧΡΟΝΙΖΕΤΑΙ — σύγκρινε χαρακτήρα, όχι θέση</p>"#, source_sr, master_sr)
    } else {
        "".to_string()
    };
    
    let html = format!(r#"<!DOCTYPE html>
<html>
<head>
<title>Audition Player</title>
<style>
  body {{ background: #1a1a1a; color: #fff; font-family: sans-serif; text-align: center; padding-top: 50px; }}
  button {{ padding: 20px 40px; font-size: 24px; margin: 10px; cursor: pointer; border-radius: 8px; border: none; background: #333; color: white; }}
  button.active {{ background: #4CAF50; }}
  #now-playing {{ font-size: 20px; margin-bottom: 30px; color: #888; }}
</style>
</head>
<body>
  <h1>A/B/C Audition</h1>
  <p id="now-playing">Playing: None</p>
  {warning_html}
  <button id="btnA" onclick="playA()">Source (A)</button>
  <button id="btnB" onclick="playB()">Master (B)</button>
  <button id="btnC" onclick="playC()" style="display:{c_display};">Reference (C)</button>

<script>
  const a = new Audio('A_source.wav'); a.loop = true;
  const b = new Audio('B_master.wav'); b.loop = true;
  const c = new Audio('C_reference.wav'); c.loop = true;
  
  a.volume = 0; b.volume = 0; c.volume = 0;
  
  let started = false;
  function startAll() {{
    if(!started) {{
      a.play(); b.play(); c.play();
      started = true;
    }}
  }}

  function playA() {{
    startAll();
    a.volume = 1; b.volume = 0; c.volume = 0;
    document.getElementById('now-playing').innerText = "Playing: Source (A)";
    updateBtns('btnA');
  }}
  function playB() {{
    startAll();
    a.volume = 0; b.volume = 1; c.volume = 0;
    document.getElementById('now-playing').innerText = "Playing: Master (B)";
    updateBtns('btnB');
  }}
  function playC() {{
    startAll();
    a.volume = 0; b.volume = 0; c.volume = 1;
    document.getElementById('now-playing').innerText = "Playing: Reference (C)";
    updateBtns('btnC');
  }}
  function updateBtns(active) {{
    ['btnA','btnB','btnC'].forEach(id => {{
      let el = document.getElementById(id);
      if(el) el.classList.toggle('active', id === active);
    }});
  }}
</script>
</body>
</html>"#);

    std::fs::write("/tmp/audition/index.html", html).unwrap();

    assert!(s_rmsl > 0.0);
    assert!(m_rmsl > 0.0);
}
