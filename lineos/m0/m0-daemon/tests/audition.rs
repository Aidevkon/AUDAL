use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use xaak::repo::DspState;

fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 0.0;
    }
    let a = &a[..n];
    let b = &b[..n];
    let mean_a = a.iter().sum::<f32>() / n as f32;
    let mean_b = b.iter().sum::<f32>() / n as f32;
    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;
    for i in 0..n {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }
    let std_dev = (var_a * var_b).sqrt();
    if std_dev > 1e-9 { cov / std_dev } else { 0.0 }
}

fn resample_linear(input: &[f32], in_sr: f32, out_sr: f32) -> Vec<f32> {
    if (in_sr - out_sr).abs() < 1.0 {
        return input.to_vec();
    }
    let ratio = in_sr / out_sr;
    let out_len = (input.len() as f32 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let in_idx_f = i as f32 * ratio;
        let in_idx = in_idx_f.floor() as usize;
        let frac = in_idx_f - in_idx as f32;
        if in_idx + 1 < input.len() {
            out.push(input[in_idx] * (1.0 - frac) + input[in_idx + 1] * frac);
        } else if in_idx < input.len() {
            out.push(input[in_idx]);
        }
    }
    out
}

#[test]
#[ignore = "audition — writes files for listening"]
fn audition() {
    let default_input = "/home/aidevcon/Downloads/DATASET/musdb18hq/test/Ben Carrigan - We'll Talk About It All Tonight/mixture.wav";
    let dataset_path = std::env::var("AUDITION_INPUT").unwrap_or_else(|_| default_input.to_string());
    
    let input_tag = std::path::Path::new(&dataset_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(24)
        .collect::<String>();
        
    let source_tagged = format!("/tmp/audition/A_source_{}.wav", input_tag);
    let master_tagged = format!("/tmp/audition/B_master_{}.wav", input_tag);
    let stems_sum_tagged = format!("/tmp/audition/E_stems_sum_{}.wav", input_tag);

    // Παλιά αρχεία από προηγούμενες συνεδρίες μπερδεύουν.
    // Σβήνουμε ΜΟΝΟ ό,τι γράφουμε εμείς (το C μένει).
    for f in ["A_source.wav", "B_master.wav", "E_stems_sum.wav", "index.html"] {
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
    ).unwrap();
    for &s in segment {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
    std::fs::copy(source_path, &source_tagged).unwrap();
    
    // ── E_stems_sum ──
    let mut left = Vec::with_capacity(segment.len() / 2);
    let mut right = Vec::with_capacity(segment.len() / 2);
    for chunk in segment.chunks_exact(2) {
        left.push(chunk[0]);
        right.push(chunk[1]);
    }
    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(l, r)| (l + r) * 0.5).collect();

    let mut engine = sp314_dsp::stft::two_pass::TwoPassEngine::new();
    let scout_res = engine.scout(&left, &right, spec.sample_rate, None, None, true);

    let mut e_left = Vec::with_capacity(segment.len() / 2);
    let mut e_right = Vec::with_capacity(segment.len() / 2);
    let callback = |stems: &sp314_dsp::stft::two_pass::FiveStemsChunk| {
        let len = stems.voice.l.len();
        for i in 0..len {
            e_left.push(stems.voice.l[i] + stems.drums.l[i] + stems.bass.l[i] + stems.harmonics.l[i] + stems.ambience.l[i]);
            e_right.push(stems.voice.r[i] + stems.drums.r[i] + stems.bass.r[i] + stems.harmonics.r[i] + stems.ambience.r[i]);
        }
    };
    engine.process_slices_with_params(&mono, &left, &right, &scout_res, 1.0, true, callback).unwrap();

    let stems_sum_path = "/tmp/audition/E_stems_sum.wav";
    let mut e_w = hound::WavWriter::create(
        stems_sum_path,
        hound::WavSpec {
            channels: 2,
            sample_rate: spec.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    ).unwrap();
    for i in 0..e_left.len() {
        e_w.write_sample(e_left[i]).unwrap();
        e_w.write_sample(e_right[i]).unwrap();
    }
    e_w.finalize().unwrap();
    std::fs::copy(stems_sum_path, &stems_sum_tagged).unwrap();

    // ── B_master ──
    let req = MasterRequest {
        audio_path: source_path.to_string(),
        preset_id: std::env::var("AUDITION_PRESET").unwrap_or_else(|_| "Transparent".to_string()),
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
    std::fs::copy(master_path, &master_tagged).unwrap();

    if std::env::var("AUDITION_SNAPSHOT").is_ok() {
        std::fs::copy(pcm.path(), ref_path).ok();
        println!("[AUDITION] snapshot: C_reference updated");
    }

    // ── MUD PROBE ──
    let mut ref_l = Vec::new();
    let mut ref_r = Vec::new();
    let mut ref_tot_rms = 0.0;
    let ref_sr: u32;
    
    // Read source for reference
    {
        let mut r = hound::WavReader::open(source_path).unwrap();
        ref_sr = r.spec().sample_rate;
        let samps: Vec<f32> = r.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect();
        let mut sum_sq = 0.0;
        for chunk in samps.chunks_exact(2) {
            ref_l.push(chunk[0]);
            ref_r.push(chunk[1]);
            sum_sq += chunk[0]*chunk[0] + chunk[1]*chunk[1];
        }
        ref_tot_rms = (sum_sq / samps.len() as f32).sqrt();
    }

    let measure_mud = |name: &str, path: &str| {
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

        let frames = samps.len() / 2;
        let mut l_vec = Vec::with_capacity(frames);
        let mut r_vec = Vec::with_capacity(frames);
        let mut mono = Vec::with_capacity(frames);
        
        let mut sum_sq_l = 0.0;
        let mut sum_sq_r = 0.0;
        let mut sum_sq_side = 0.0;

        for i in 0..frames {
            let l = samps[i * 2];
            let r = samps[i * 2 + 1];
            l_vec.push(l);
            r_vec.push(r);
            mono.push((l + r) * 0.5);
            
            sum_sq_l += l * l;
            sum_sq_r += r * r;
            let side = (l - r) * 0.5;
            sum_sq_side += side * side;
        }

        let rms_l = (sum_sq_l / frames as f32).sqrt();
        let rms_r = (sum_sq_r / frames as f32).sqrt();
        let side_rms = (sum_sq_side / frames as f32).sqrt();
        let total_rms = ((sum_sq_l + sum_sq_r) / (frames * 2) as f32).sqrt();

        let corr_l_raw = pearson(&l_vec, &ref_l);
        let corr_r_raw = pearson(&r_vec, &ref_r);

        let corr_l_resamp = if sp.sample_rate != ref_sr {
            let resamp_l = resample_linear(&l_vec, sp.sample_rate as f32, ref_sr as f32);
            pearson(&resamp_l, &ref_l)
        } else {
            corr_l_raw
        };

        let corr_r_resamp = if sp.sample_rate != ref_sr {
            let resamp_r = resample_linear(&r_vec, sp.sample_rate as f32, ref_sr as f32);
            pearson(&resamp_r, &ref_r)
        } else {
            corr_r_raw
        };

        use sp314_dsp::dsp::biquad::{butter_hp2_prewarped, butter_lp2_prewarped};
        let sr_f32 = sp.sample_rate as f32;
        let mut lp_2k = butter_lp2_prewarped(2000.0, sr_f32);
        
        let mut hp_2k = butter_hp2_prewarped(2000.0, sr_f32);
        let mut lp_8k = butter_lp2_prewarped(8000.0, sr_f32);
        
        let mut hp_8k = butter_hp2_prewarped(8000.0, sr_f32);

        let (mut s1, mut p1) = (0.0_f32, 0.0_f32);
        let (mut s2, mut p2) = (0.0_f32, 0.0_f32);
        let (mut s3, mut p3) = (0.0_f32, 0.0_f32);

        for &m in &mono {
            let v1 = lp_2k.process(m);
            s1 += v1 * v1;
            if v1.abs() > p1 { p1 = v1.abs(); }

            let v2 = lp_8k.process(hp_2k.process(m));
            s2 += v2 * v2;
            if v2.abs() > p2 { p2 = v2.abs(); }

            let v3 = hp_8k.process(m);
            s3 += v3 * v3;
            if v3.abs() > p3 { p3 = v3.abs(); }
        }

        let r1 = (s1 / frames as f32).sqrt();
        let r2 = (s2 / frames as f32).sqrt();
        let r3 = (s3 / frames as f32).sqrt();

        let c1 = if r1 > 1e-9 { p1 / r1 } else { 0.0 };
        let c2 = if r2 > 1e-9 { p2 / r2 } else { 0.0 };
        let c3 = if r3 > 1e-9 { p3 / r3 } else { 0.0 };

        let level_diff = if ref_tot_rms > 0.0 && total_rms > 0.0 {
            20.0 * (total_rms / ref_tot_rms).log10()
        } else {
            0.0
        };

        println!("[MUD-PROBE] {}:", name);
        println!("  Level diff vs A: {:>5.2} dB", level_diff);
        println!("  rms_l={:.4} rms_r={:.4} side_rms={:.6}", rms_l, rms_r, side_rms);
        println!("  Pearson raw:    L={:.4} R={:.4}", corr_l_raw, corr_r_raw);
        println!("  Pearson resamp: L={:.4} R={:.4}", corr_l_resamp, corr_r_resamp);
        println!("  0-2k:   RMS={:.4} Crest={:.2}", r1, c1);
        println!("  2k-8k:  RMS={:.4} Crest={:.2}", r2, c2);
        println!("  8k-24k: RMS={:.4} Crest={:.2}\n", r3, c3);
    };

    println!();
    measure_mud("A_source", source_path);
    measure_mud("E_stems_sum", stems_sum_path);
    measure_mud("B_master", master_path);

    let source_sr = spec.sample_rate;
    if source_sr != master_sr {
        println!("[AUDITION] ⚠ sample rate: source {} · master {}", source_sr, master_sr);
        println!("[AUDITION] ⚠ ΤΟ A/B ΔΕΝ ΣΥΓΧΡΟΝΙΖΕΤΑΙ — σύγκρινε χαρακτήρα, όχι θέση\n");
    }

    println!("[AUDITION] files:");
    println!("  A: {}", source_path);
    println!("  B: {}", master_path);
    if std::path::Path::new(ref_path).exists() {
        println!("  C: {}", ref_path);
    }
    println!("  E: {}", stems_sum_path);
    println!("[AUDITION] tagged copies:");
    println!("  A: {}", source_tagged);
    println!("  B: {}", master_tagged);
    println!("  E: {}", stems_sum_tagged);
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
  <h1>A/B/C/E Audition</h1>
  <p id="now-playing">Playing: None</p>
  {warning_html}
  <button id="btnA" onclick="playA()">Source (A)</button>
  <button id="btnB" onclick="playB()">Master (B)</button>
  <button id="btnC" onclick="playC()" style="display:{c_display};">Reference (C)</button>
  <button id="btnE" onclick="playE()">Stems Sum (E)</button>

<script>
  const a = new Audio('A_source.wav'); a.loop = true;
  const b = new Audio('B_master.wav'); b.loop = true;
  const c = new Audio('C_reference.wav'); c.loop = true;
  const e = new Audio('E_stems_sum.wav'); e.loop = true;
  
  a.volume = 0; b.volume = 0; c.volume = 0; e.volume = 0;
  
  let started = false;
  function startAll() {{
    if(!started) {{
      a.play(); b.play(); c.play(); e.play();
      started = true;
    }}
  }}

  function playA() {{
    startAll();
    a.volume = 1; b.volume = 0; c.volume = 0; e.volume = 0;
    document.getElementById('now-playing').innerText = "Playing: Source (A)";
    updateBtns('btnA');
  }}
  function playB() {{
    startAll();
    a.volume = 0; b.volume = 1; c.volume = 0; e.volume = 0;
    document.getElementById('now-playing').innerText = "Playing: Master (B)";
    updateBtns('btnB');
  }}
  function playC() {{
    startAll();
    a.volume = 0; b.volume = 0; c.volume = 1; e.volume = 0;
    document.getElementById('now-playing').innerText = "Playing: Reference (C)";
    updateBtns('btnC');
  }}
  function playE() {{
    startAll();
    a.volume = 0; b.volume = 0; c.volume = 0; e.volume = 1;
    document.getElementById('now-playing').innerText = "Playing: Stems Sum (E)";
    updateBtns('btnE');
  }}
  function updateBtns(active) {{
    ['btnA','btnB','btnC','btnE'].forEach(id => {{
      let el = document.getElementById(id);
      if(el) el.classList.toggle('active', id === active);
    }});
  }}
</script>
</body>
</html>"#);

    std::fs::write("/tmp/audition/index.html", html).unwrap();
}
