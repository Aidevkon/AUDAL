use hound;
use rustfft::num_complex::Complex;
use sp314_dsp::analysis::onset_flux::SuperFluxOnset;
use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
use sp314_dsp::analysis::vad_model::{FixedPriors, VadClassifier};
use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use sp314_dsp::stft::two_pass::apply_spectral_mask_to_chunk;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::{StreamingStftEncoder, N_BINS};

fn read_audio(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    let samples: Vec<i16> = reader.samples().map(|s| s.unwrap()).collect();
    let mut mono = Vec::new();
    if spec.channels == 2 {
        for i in 0..(samples.len() / 2) {
            let l = samples[2 * i] as f32 / 32768.0;
            let r = samples[2 * i + 1] as f32 / 32768.0;
            mono.push((l + r) * 0.5);
        }
    } else {
        for s in samples {
            mono.push(s as f32 / 32768.0);
        }
    }
    mono
}

fn compute_cplx_spectrogram(signal: &[f32]) -> Vec<Vec<Complex<f32>>> {
    let mut encoder = StreamingStftEncoder::new();
    let mut cplx = encoder.feed_chunk(signal);
    cplx.extend(encoder.finish());
    cplx
}

fn rms_db(signal: &[f32]) -> f32 {
    if signal.is_empty() { return -144.0; }
    let mut sum = 0.0;
    for &x in signal { sum += x * x; }
    10.0 * (sum / signal.len() as f32).max(1e-12).log10()
}

fn crest_factor(signal: &[f32]) -> f32 {
    if signal.is_empty() { return 0.0; }
    let mut sum = 0.0;
    let mut max_sq = 0.0_f32;
    for &x in signal {
        let sq = x * x;
        sum += sq;
        if sq > max_sq { max_sq = sq; }
    }
    max_sq.sqrt() / (sum / signal.len() as f32).sqrt().max(1e-6)
}

fn compute_vad_speech_pct(audio: &[f32]) -> f32 {
    let mut ext = VadFeatureExtractor::new();
    let feats = ext.process_chunk(audio, audio, audio);
    let mut vad = VadClassifier::new(FixedPriors);
    let mut speech_frames = 0;
    for f in &feats {
        if vad.process(f, -144.0).is_speech {
            speech_frames += 1;
        }
    }
    100.0 * speech_frames as f32 / feats.len().max(1) as f32
}

#[test]
fn test_p3_jury() {
    let signal = read_audio("tests/fixtures/bodleasons_mid.wav");
    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&signal, 48000, None, None);

    let tensor_w = scout.tensor_w.clone();
    let proxy_w = scout.w.clone();
    let tau = scout.tau;
    let k_a = 5;
    let k_b = 8;

    let full_cplx = compute_cplx_spectrogram(&signal);
    let n_frames = full_cplx.len();

    let mut full_mag = Vec::new();
    let mut mel_frames = Vec::new();
    for f in &full_cplx {
        let mut mag = [0.0_f32; 1025];
        for b in 0..1025 {
            mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
        }
        full_mag.push(mag.to_vec());
        mel_frames.push(sp314_dsp::analysis::mel_128::fold_to_mel(&mag));
    }

    let mut full_v = vec![0.0_f32; 128 * n_frames];
    for f in 0..n_frames {
        for b in 0..128 {
            full_v[b * n_frames + f] = mel_frames[f][b];
        }
    }

    // A (Plain NMF)
    let mut nmf_a = NmfEngine::new(k_a);
    nmf_a.w = proxy_w.clone();
    let mut a_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k_a];

    // B (NMFD chunked)
    let mut nmf_b = NmfEngine::new(k_b);
    let mut b_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k_b];
    let mut b_h_sum = vec![0.0_f32; k_b];

    let chunk_len = 96000;
    let history_len = 10240;
    let mut pos = 0;
    let mut written_b = vec![false; n_frames];
    let mut num_boundaries = 0;

    while pos < signal.len() {
        let start = if pos > history_len { pos - history_len } else { 0 };
        let end = std::cmp::min(pos + chunk_len, signal.len());
        let is_last_chunk = pos + chunk_len >= signal.len();
        let chunk_signal = &signal[start..end];
        let chunk_cplx = compute_cplx_spectrogram(chunk_signal);
        let c_frames = chunk_cplx.len();

        let mut chunk_mag = Vec::new();
        let mut chunk_mel = Vec::new();
        for f in &chunk_cplx {
            let mut mag = vec![0.0_f32; 1025];
            for b in 0..1025 {
                mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
            }
            chunk_mag.push(mag.clone());
            let mag_arr: [f32; 1025] = mag.try_into().unwrap();
            chunk_mel.push(sp314_dsp::analysis::mel_128::fold_to_mel(&mag_arr));
        }

        let pad_samples = if pos == 0 { 0 } else { history_len };
        let pad_frames = pad_samples / 512;
        let global_start_frame = (start + pad_samples) / 512;

        // A (Plain NMF)
        let h_chunk_a = nmf_a.transform(&proxy_w, &chunk_mag);
        for comp in 0..k_a {
            let c_mask = nmf_a.component_mask_chunk(comp, &h_chunk_a, c_frames, N_BINS);
            let core_mask = if c_mask.len() > pad_frames { &c_mask[pad_frames..] } else { &[] };
            let mut i = 0;
            while i < core_mask.len() && global_start_frame + i < n_frames {
                a_masks[comp][global_start_frame + i] = core_mask[i].clone();
                i += 1;
            }
            if is_last_chunk && !core_mask.is_empty() {
                while global_start_frame + i < n_frames {
                    a_masks[comp][global_start_frame + i] = core_mask.last().unwrap().clone();
                    i += 1;
                }
            }
        }

        // B (NMFD chunked)
        let mut c_v = vec![0.0_f32; 128 * c_frames];
        for f in 0..c_frames {
            for b in 0..128 { c_v[b * c_frames + f] = chunk_mel[f][b]; }
        }
        let init_h_chunk = vec![0.1_f32; k_b * c_frames];
        let (chunk_h, _) = nmfd_f32_h_only(&c_v, &tensor_w, &init_h_chunk, 128, k_b, c_frames, tau, 12);

        for comp in 0..k_b {
            for f in 0..c_frames { b_h_sum[comp] += chunk_h[comp * c_frames + f]; }
            let c_mask = nmf_b.nmfd_component_mask_chunk(comp, &chunk_h, &tensor_w, c_frames, N_BINS, tau);
            let core_mask = if c_mask.len() > pad_frames { &c_mask[pad_frames..] } else { &[] };
            let mut i = 0;
            while i < core_mask.len() && global_start_frame + i < n_frames {
                b_masks[comp][global_start_frame + i] = core_mask[i].clone();
                if comp == 0 { written_b[global_start_frame + i] = true; }
                i += 1;
            }
            if is_last_chunk && !core_mask.is_empty() {
                while global_start_frame + i < n_frames {
                    b_masks[comp][global_start_frame + i] = core_mask.last().unwrap().clone();
                    if comp == 0 { written_b[global_start_frame + i] = true; }
                    i += 1;
                }
            }
        }

        if pos > 0 { num_boundaries += 1; }
        pos += chunk_len;
    }

    let unwritten_count = written_b.iter().filter(|&&w| !w).count();
    println!("UNWRITTEN|count={}", unwritten_count);

    // ==========================================
    // PREPARE A STEMS & UNGATED B
    // ==========================================
    // ==========================================
    // PREPARE A STEMS & UNGATED B
    // ==========================================
    let mut a_stems = Vec::new();
    let mut b_stems_raw = Vec::new();
    for comp in 0..k_a { a_stems.push(apply_spectral_mask_to_chunk(&full_cplx, &a_masks[comp], signal.len())); }
    for comp in 0..k_b { b_stems_raw.push(apply_spectral_mask_to_chunk(&full_cplx, &b_masks[comp], signal.len())); }
    
    // UNGATED B VOICE
    let mut b_voice_raw = vec![0.0_f32; signal.len()];
    for i in 0..signal.len() {
        b_voice_raw[i] = b_stems_raw[0][i] + b_stems_raw[1][i] + b_stems_raw[2][i] + b_stems_raw[3][i];
    }
    
    let mut ext = VadFeatureExtractor::new();
    let feats = ext.process_chunk(&signal, &signal, &signal);
    let mut vad = VadClassifier::new(FixedPriors);
    let mut mix_posterior_map = Vec::new();
    let mut mix_is_speech = Vec::new();
    for f in &feats {
        let dec = vad.process(f, -144.0);
        mix_posterior_map.push(dec.posterior);
        mix_is_speech.push(dec.is_speech);
    }
    
    let all_idx = [scout.voice_idx, scout.bass_idx, scout.harmonics_idx, scout.ambience_idx];
    let drums_idx = (0..5).find(|&i| !all_idx.contains(&i)).unwrap();
    
    let mut free_flatness = [0.0f32; 4];
    let mut free_centroids = [0.0f32; 4];
    for i in 0..4 {
        let c = 4 + i;
        let mut log_sum = 0.0f32;
        let mut arith = 0.0f32;
        let mut sum_mw = 0.0f32;
        for m in 0..128 {
            let mut avg_t = 0.0_f32;
            for t in 0..tau { avg_t += tensor_w[m * (k_b * tau) + c * tau + t]; }
            avg_t /= tau as f32;
            log_sum += libm::logf(avg_t + 1e-10);
            arith += avg_t;
            sum_mw += m as f32 * avg_t;
        }
        let geom = libm::expf(log_sum / 128.0);
        let mean = arith / 128.0;
        free_flatness[i] = if mean > 1e-10 { (geom / mean).clamp(0.0, 1.0) } else { 0.0 };
        free_centroids[i] = if mean > 1e-10 { sum_mw / arith } else { 0.0 };
    }
    let ambience_idx_free = (0..4).max_by(|&a, &b| free_flatness[a].partial_cmp(&free_flatness[b]).unwrap()).unwrap_or(0);
    let mut remaining_free: Vec<usize> = (0..4).filter(|&i| i != ambience_idx_free).collect();
    remaining_free.sort_by(|&a, &b| free_centroids[a].partial_cmp(&free_centroids[b]).unwrap());
    
    println!("\n=== S2 ===");
    let mut gate_target = vec![0.0_f32; n_frames];
    for f in 0..n_frames {
        let vad_frame = std::cmp::min(f / 3, mix_posterior_map.len().saturating_sub(1));
        let p = mix_posterior_map[vad_frame];
        gate_target[f] = p.max(0.3).min(1.0); // S2
    }
    
    let n_ramp = 3;
    let mut smoothed_gate = gate_target.clone();
    let mut is_ramp = vec![false; n_frames];
    let mut f_idx = 0;
    let mut gate_transitions = Vec::new();
    while f_idx < n_frames - 1 {
        if gate_target[f_idx] != gate_target[f_idx+1] {
            gate_transitions.push(f_idx * 512);
            let start_val = gate_target[f_idx];
            let end_val = gate_target[f_idx+1];
            for i in 1..=n_ramp {
                if f_idx + i < n_frames {
                    let alpha = i as f32 / n_ramp as f32;
                    smoothed_gate[f_idx + i] = start_val + (end_val - start_val) * alpha;
                    is_ramp[f_idx + i] = true;
                }
            }
            f_idx += n_ramp;
        } else {
            f_idx += 1;
        }
    }
    
    let b_boundaries = gate_transitions.clone();
    let mut b_controls = Vec::new();
    for &b in &b_boundaries {
        if b > 44100 { b_controls.push(b - 44100); }
    }
    
    // APPLY GATE & REDISTRIBUTE MATH (fixed normalization order)
    for f in 0..n_frames {
        let g = smoothed_gate[f];
        if g < 1.0 {
            for b in 0..N_BINS {
                let mut non_voice_sum = 0.0_f32;
                for comp in 4..k_b { non_voice_sum += b_masks[comp][f][b]; }
                let mut voice_sum = 0.0_f32;
                for comp in 0..4 { voice_sum += b_masks[comp][f][b]; }
                
                let target_non_voice_sum = 1.0 - voice_sum * g; // FIXED
                let multiplier = if non_voice_sum > 1e-12 {
                    target_non_voice_sum / non_voice_sum
                } else { 1.0 };
                
                for comp in 0..4 { b_masks[comp][f][b] *= g; }
                for comp in 4..k_b { b_masks[comp][f][b] *= multiplier; }
            }
        }
    }
    
    // DIAGNOSE PARTITION
    let mut max_abs_nospeech = 0.0_f32;
    let mut max_abs_speech = 0.0_f32;
    let mut max_abs_ramp = 0.0_f32;
    let mut max_abs = 0.0_f32;
    
    for f in 0..n_frames {
        let vad_frame = std::cmp::min(f / 3, mix_is_speech.len().saturating_sub(1));
        let speech = mix_is_speech[vad_frame];
        let ramp = is_ramp[f];
        
        for b in 0..N_BINS {
            let mut sum = 0.0_f32;
            for comp in 0..k_b { sum += b_masks[comp][f][b]; }
            if b_h_sum[0] > 0.0 {
                let diff = (sum - 1.0).abs();
                if diff > max_abs { max_abs = diff; }
                if ramp {
                    if diff > max_abs_ramp { max_abs_ramp = diff; }
                } else if speech {
                    if diff > max_abs_speech { max_abs_speech = diff; }
                } else {
                    if diff > max_abs_nospeech { max_abs_nospeech = diff; }
                }
            }
        }
    }
    println!("PARTLEAK|category=no-speech|max_dev={:.6}", max_abs_nospeech);
    println!("PARTLEAK|category=speech|max_dev={:.6}", max_abs_speech);
    println!("PARTLEAK|category=ramp|max_dev={:.6}", max_abs_ramp);
    
    // GATED STEMS
    let mut b_stems_gated = Vec::new();
    for comp in 0..k_b { b_stems_gated.push(apply_spectral_mask_to_chunk(&full_cplx, &b_masks[comp], signal.len())); }
    
    let mut b_stems = vec![vec![0.0_f32; signal.len()]; 5];
    for i in 0..signal.len() {
        b_stems[scout.voice_idx][i] = b_stems_gated[0][i] + b_stems_gated[1][i] + b_stems_gated[2][i] + b_stems_gated[3][i];
        b_stems[scout.bass_idx][i] = b_stems_gated[4 + remaining_free[0]][i];
        b_stems[drums_idx][i] = b_stems_gated[4 + remaining_free[1]][i];
        b_stems[scout.harmonics_idx][i] = b_stems_gated[4 + remaining_free[2]][i];
        b_stems[scout.ambience_idx][i] = b_stems_gated[4 + ambience_idx_free][i];
    }
    
    // ==========================================
    // FULL JURY TABLE
    // ==========================================
    
    // 1. SEAMS
    for comp in 0..k_a {
        let stem = if comp == scout.voice_idx { "voice" } else if comp == scout.ambience_idx { "ambience" } else if comp == scout.bass_idx { "bass" } else if comp == scout.harmonics_idx { "harmonics" } else { "drums" };
        let audio = &b_stems[comp];
        
        let mut envelope = Vec::new();
        let mut superflux = sp314_dsp::analysis::onset_flux::SuperFluxOnset::new(10.0);
        let mut encoder = StreamingStftEncoder::new();
        let mut cplx = encoder.feed_chunk(audio);
        cplx.extend(encoder.finish());
        
        for f in &cplx {
            let mut mag = [0.0_f32; 1025];
            for b in 0..1025 { mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im); }
            envelope.push(superflux.process(&mag));
        }
        
        let mut peaks = vec![false; envelope.len()];
        for i in 1..envelope.len() - 1 {
            if envelope[i] > envelope[i - 1] && envelope[i] > envelope[i + 1] && envelope[i] > 0.05 { peaks[i] = true; }
        }
        
        let mut b_peaks = 0;
        for &s in &b_boundaries {
            if s + 2048 <= signal.len() {
                let f = s / 512;
                if f > 0 && f + 1 < peaks.len() {
                    if peaks[f - 1] || peaks[f] || peaks[f + 1] { b_peaks += 1; }
                }
            }
        }
        let mut c_peaks = 0;
        for &s in &b_controls {
            let f = s / 512;
            if f > 0 && f + 1 < peaks.len() {
                if peaks[f - 1] || peaks[f] || peaks[f + 1] { c_peaks += 1; }
            }
        }
        println!("JURY|seams|stem={}|boundary_peaks={}|control_peaks={}", stem, b_peaks, c_peaks);
    }

    // 2. PARTITION
    let mut b_sum = vec![0.0_f32; signal.len()];
    for i in 0..signal.len() {
        for comp in 0..k_a { b_sum[i] += b_stems[comp][i]; }
    }
    let mut num = 0.0_f32;
    let mut den = 0.0_f32;
    for i in 0..signal.len() {
        let diff = signal[i] - b_sum[i];
        num += signal[i] * signal[i];
        den += diff * diff;
    }
    let snr_db = 10.0 * (num / den.max(1e-12)).log10();
    println!("JURY|partition|max_abs={:.6}|snr_db={:.2}", max_abs, snr_db);

    // 3. CONTINUITY
    for comp in 0..k_a {
        let stem_name = if comp == scout.voice_idx { "voice" } else if comp == scout.ambience_idx { "ambience" } else if comp == scout.bass_idx { "bass" } else if comp == scout.harmonics_idx { "harmonics" } else { "drums" };
        let audio = &b_stems[comp];
        let mut worst_jump = 0.0_f32;
        for &b in &b_boundaries {
            if b > 0 && b < audio.len() {
                let jump = (audio[b] - audio[b - 1]).abs();
                if jump > worst_jump { worst_jump = jump; }
            }
        }
        let mut control_jump = 0.0_f32;
        for &c in &b_controls {
            if c > 0 && c < audio.len() {
                let jump = (audio[c] - audio[c - 1]).abs();
                if jump > control_jump { control_jump = jump; }
            }
        }
        println!("JURY|continuity|stem={}|boundary_jump={:.6}|control_jump={:.6}", stem_name, worst_jump, control_jump);
    }

    // 4. THE RESIDUAL TEST
    fn compute_vad_speech_pct(audio: &[f32]) -> f32 {
        let mut ext = VadFeatureExtractor::new();
        let feats = ext.process_chunk(audio, audio, audio);
        let mut vad = VadClassifier::new(FixedPriors);
        let mut speech_frames = 0;
        for f in &feats {
            if vad.process(&f, -144.0).is_speech { speech_frames += 1; }
        }
        100.0 * speech_frames as f32 / feats.len().max(1) as f32
    }
    for (variant, stems) in [("A", &a_stems), ("S2", &b_stems)] {
        let voice_audio = &stems[scout.voice_idx];
        let mut residual = vec![0.0_f32; signal.len()];
        for i in 0..signal.len() {
            residual[i] = signal[i] - voice_audio[i];
        }
        let speech_pct = compute_vad_speech_pct(&residual);
        println!("JURY3|residual|variant={}|speech_pct={:.2}", variant, speech_pct);
    }

    // 5. Energy check
    fn rms_db_local(signal: &[f32]) -> f32 {
        if signal.is_empty() { return -144.0; }
        let mut sum = 0.0;
        for &x in signal { sum += x * x; }
        10.0 * (sum / signal.len() as f32).max(1e-12).log10()
    }
    for (variant, stems) in [("A", &a_stems), ("S2", &b_stems)] {
        let stem_db = rms_db_local(&stems[scout.voice_idx]);
        println!("JURY3|energy|variant={}|stem_db={:.2}", variant, stem_db);
    }
    
    // 6. BASS BLEED
    fn compute_low_band_pct(audio: &[f32]) -> f32 {
        let mut encoder = StreamingStftEncoder::new();
        let mut cplx = encoder.feed_chunk(audio);
        cplx.extend(encoder.finish());
        
        let mut low_band_e = 0.0_f32;
        let mut total_e = 0.0_f32;
        for f in &cplx {
            let mut mag = [0.0_f32; 1025];
            for b in 0..1025 {
                mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
            }
            let mel = sp314_dsp::analysis::mel_128::fold_to_mel(&mag);
            for b in 0..16 { low_band_e += mel[b]; }
            for b in 0..128 { total_e += mel[b]; }
        }
        100.0 * low_band_e / total_e.max(1e-12)
    }
    
    for (variant, stems) in [("A", &a_stems), ("S2", &b_stems)] {
        let low_band_pct = compute_low_band_pct(&stems[scout.voice_idx]);
        println!("JURY3|bassbleed|variant={}|low_band_pct={:.2}", variant, low_band_pct);
    }
    
    // 7. JURY4: ENERGY JUROR
    let mut e_speech_gated = 0.0_f64;
    let mut e_speech_raw = 0.0_f64;
    let mut e_nospeech_gated = 0.0_f64;
    let mut e_nospeech_raw = 0.0_f64;
    
    for i in 0..signal.len() {
        let vad_frame = std::cmp::min(i / 1536, mix_is_speech.len().saturating_sub(1));
        let speech = mix_is_speech[vad_frame];
        let g_val = b_stems[scout.voice_idx][i] as f64;
        let r_val = b_voice_raw[i] as f64;
        if speech {
            e_speech_gated += g_val * g_val;
            e_speech_raw += r_val * r_val;
        } else {
            e_nospeech_gated += g_val * g_val;
            e_nospeech_raw += r_val * r_val;
        }
    }
    
    let db_speech_gated = 10.0 * (e_speech_gated.max(1e-12)).log10();
    let db_speech_raw = 10.0 * (e_speech_raw.max(1e-12)).log10();
    let db_nospeech_gated = 10.0 * (e_nospeech_gated.max(1e-12)).log10();
    let db_nospeech_raw = 10.0 * (e_nospeech_raw.max(1e-12)).log10();
    
    println!("JURY4|suppress|variant=S2|nospeech_energy_db_delta={:.2}", db_nospeech_gated - db_nospeech_raw);
    println!("JURY4|preserve|variant=S2|speech_energy_db_delta={:.2}", db_speech_gated - db_speech_raw);
}
