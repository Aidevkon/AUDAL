use hound;
use rustfft::num_complex::Complex;
use sp314_dsp::analysis::onset_flux::SuperFluxOnset;
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
    if signal.is_empty() {
        return -144.0;
    }
    let mut sum = 0.0;
    for &x in signal {
        sum += x * x;
    }
    let rms = (sum / signal.len() as f32).sqrt();
    10.0 * (sum / signal.len() as f32).max(1e-12).log10()
}

fn crest_factor(signal: &[f32]) -> f32 {
    if signal.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut max_sq = 0.0_f32;
    for &x in signal {
        let sq = x * x;
        sum += sq;
        if sq > max_sq {
            max_sq = sq;
        }
    }
    max_sq.sqrt() / (sum / signal.len() as f32).sqrt().max(1e-6)
}

fn measure_region(
    whole: &[f32],
    chunked: &[f32],
    start: usize,
    end: usize,
    name: &str,
    stem: &str,
) {
    let mut max_abs = 0.0_f32;
    let mut delta = Vec::with_capacity(end - start);
    for i in start..end {
        let d = whole[i] - chunked[i];
        if d.abs() > max_abs {
            max_abs = d.abs();
        }
        delta.push(d);
    }
    let rms_d = rms_db(&delta);
    println!(
        "NMFDAUDIO|{}|stem={}|max_abs={:.6}|rms_db_delta={:.2}",
        name, stem, max_abs, rms_d
    );
}

#[test]
fn test_jury_corrected() {
    let signal = read_audio("tests/fixtures/bodleasons_mid.wav");
    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&signal, 48000, None, None, true);

    let tensor_w = scout.tensor_w.clone();
    let proxy_w = scout.w.clone();
    let tau = scout.tau;
    let k = 5;

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

    let mut nmf = NmfEngine::new(k);
    nmf.w = proxy_w.clone();

    // ==========================================
    // (A) Plain NMF
    // ==========================================
    let mut a_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k];

    // ==========================================
    // (B) NMFD chunked cold
    // ==========================================
    let mut b_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k];

    let chunk_len = 96000;
    let history_len = 10240;
    let mut pos = 0;

    let mut written_b = vec![false; n_frames];
    let mut num_boundaries = 0;

    while pos < signal.len() {
        let start = if pos > history_len {
            pos - history_len
        } else {
            0
        };
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

        // Stitch math
        let pad_samples = if pos == 0 { 0 } else { history_len };
        let pad_frames = pad_samples / 512;
        let global_start_frame = (start + pad_samples) / 512;

        // A (Plain NMF)
        let h_chunk_a = nmf.transform(&proxy_w, &chunk_mag);
        for comp in 0..k {
            let c_mask = nmf.component_mask_chunk(comp, &h_chunk_a, c_frames, N_BINS);
            let core_mask = if c_mask.len() > pad_frames {
                &c_mask[pad_frames..]
            } else {
                &[]
            };

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
            for b in 0..128 {
                c_v[b * c_frames + f] = chunk_mel[f][b];
            }
        }
        let init_h_chunk = vec![0.1_f32; k * c_frames];
        let (chunk_h, _) =
            nmfd_f32_h_only(&c_v, &tensor_w, &init_h_chunk, 128, k, c_frames, tau, 12);

        for comp in 0..k {
            let c_mask =
                nmf.nmfd_component_mask_chunk(comp, &chunk_h, &tensor_w, c_frames, N_BINS, tau);
            let core_mask = if c_mask.len() > pad_frames {
                &c_mask[pad_frames..]
            } else {
                &[]
            };

            let mut i = 0;
            while i < core_mask.len() && global_start_frame + i < n_frames {
                b_masks[comp][global_start_frame + i] = core_mask[i].clone();
                if comp == 0 {
                    written_b[global_start_frame + i] = true;
                }
                i += 1;
            }
            if is_last_chunk && !core_mask.is_empty() {
                while global_start_frame + i < n_frames {
                    b_masks[comp][global_start_frame + i] = core_mask.last().unwrap().clone();
                    if comp == 0 {
                        written_b[global_start_frame + i] = true;
                    }
                    i += 1;
                }
            }
        }

        if pos > 0 {
            num_boundaries += 1;
        }
        pos += chunk_len;
    }

    let unwritten_count = written_b.iter().filter(|&&w| !w).count();
    println!("UNWRITTEN|count={}", unwritten_count);
    assert_eq!(
        unwritten_count, 0,
        "Harness stitch math left unwritten frames!"
    );

    let mut a_stems = Vec::new();
    let mut b_stems = Vec::new();
    for comp in 0..k {
        a_stems.push(apply_spectral_mask_to_chunk(
            &full_cplx,
            &a_masks[comp],
            signal.len(),
        ));
        b_stems.push(apply_spectral_mask_to_chunk(
            &full_cplx,
            &b_masks[comp],
            signal.len(),
        ));
    }

    // ==========================================
    // (C) NMFD whole
    // ==========================================
    let init_h_full = vec![0.1_f32; k * n_frames];
    let (whole_h, _) = nmfd_f32_h_only(&full_v, &tensor_w, &init_h_full, 128, k, n_frames, tau, 12);
    let mut c_stems = Vec::new();
    let mut c_masks = vec![vec![vec![0.0_f32; N_BINS]; n_frames]; k];
    for comp in 0..k {
        let whole_mask =
            nmf.nmfd_component_mask_chunk(comp, &whole_h, &tensor_w, n_frames, N_BINS, tau);
        c_masks[comp] = whole_mask.clone();
        c_stems.push(apply_spectral_mask_to_chunk(
            &full_cplx,
            &whole_mask,
            signal.len(),
        ));
    }

    let b_boundaries: Vec<usize> = (1..=num_boundaries).map(|i| i * chunk_len).collect();
    let b_controls: Vec<usize> = (1..=num_boundaries)
        .map(|i| i * chunk_len - chunk_len / 2)
        .collect();

    // ==========================================
    // P3d JURY TESTS
    // ==========================================
    println!("\n=== JURY ===");
    // 1. SEAM DETECTOR
    for comp in 0..k {
        let stem = if comp == scout.voice_idx {
            "voice"
        } else if comp == scout.ambience_idx {
            "ambience"
        } else if comp == scout.bass_idx {
            "bass"
        } else if comp == scout.harmonics_idx {
            "harmonics"
        } else {
            "comp5"
        };

        let mut encoder = StreamingStftEncoder::new();
        let mut cplx = encoder.feed_chunk(&b_stems[comp]);
        cplx.extend(encoder.finish());

        let mut superflux = SuperFluxOnset::new(10.0);
        let mut envelope = Vec::new();
        for f in &cplx {
            let mut mag = [0.0_f32; 1025];
            for b in 0..1025 {
                mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
            }
            envelope.push(superflux.process(&mag));
        }

        let mut peaks = vec![false; envelope.len()];
        for i in 1..envelope.len() - 1 {
            if envelope[i] > envelope[i - 1] && envelope[i] > envelope[i + 1] && envelope[i] > 0.05
            {
                peaks[i] = true;
            }
        }

        let mut b_peaks = 0;
        for &s in &b_boundaries {
            if s + 2048 <= signal.len() {
                let f = s / 512;
                if f > 0 && f + 1 < peaks.len() {
                    if peaks[f - 1] || peaks[f] || peaks[f + 1] {
                        b_peaks += 1;
                    }
                }
            }
        }
        let mut c_peaks = 0;
        for &s in &b_controls {
            let f = s / 512;
            if f > 0 && f + 1 < peaks.len() {
                if peaks[f - 1] || peaks[f] || peaks[f + 1] {
                    c_peaks += 1;
                }
            }
        }
        println!(
            "JURY|seams|stem={}|boundary_peaks={}|control_peaks={}",
            stem, b_peaks, c_peaks
        );
    }

    // 2. SPECTRAL TRUTH
    let a_voice_cplx = compute_cplx_spectrogram(&a_stems[scout.voice_idx]);
    let mut avg_mel = [0.0_f32; 128];
    for f in &a_voice_cplx {
        let mut mag = [0.0_f32; 1025];
        for b in 0..1025 {
            mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
        }
        let mel = sp314_dsp::analysis::mel_128::fold_to_mel(&mag);
        for b in 0..128 {
            avg_mel[b] += mel[b];
        }
    }
    let mut indices: Vec<usize> = (0..128).collect();
    indices.sort_by(|&i, &j| avg_mel[j].partial_cmp(&avg_mel[i]).unwrap());
    let mut is_top16 = [false; 128];
    for &idx in &indices[0..16] {
        is_top16[idx] = true;
    }

    for (variant, stems) in [("A", &a_stems), ("B", &b_stems)] {
        let voice_cplx = compute_cplx_spectrogram(&stems[scout.voice_idx]);
        let mut in_band_e = 0.0_f32;
        let mut out_band_e = 0.0_f32;
        for f in &voice_cplx {
            let mut mag = [0.0_f32; 1025];
            for b in 0..1025 {
                mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
            }
            let mel = sp314_dsp::analysis::mel_128::fold_to_mel(&mag);
            for b in 0..128 {
                if is_top16[b] {
                    in_band_e += mel[b];
                } else {
                    out_band_e += mel[b];
                }
            }
        }
        let in_band_pct = 100.0 * in_band_e / (in_band_e + out_band_e).max(1e-9);
        println!(
            "JURY|leakage|variant={}|stem=voice|in_band_pct={:.2}",
            variant, in_band_pct
        );
    }

    // 3. PARTITION
    let mut b_sum = vec![0.0_f32; signal.len()];
    for i in 0..signal.len() {
        for comp in 0..k {
            b_sum[i] += b_stems[comp][i];
        }
    }
    let mut max_abs = 0.0_f32;
    let mut num = 0.0_f32;
    let mut den = 0.0_f32;
    for i in 0..signal.len() {
        let diff = signal[i] - b_sum[i];
        if diff.abs() > max_abs {
            max_abs = diff.abs();
        }
        num += signal[i] * signal[i];
        den += diff * diff;
    }
    let snr_db = 10.0 * (num / den.max(1e-12)).log10();
    println!("JURY|partition|max_abs={:.6}|snr_db={:.2}", max_abs, snr_db);

    // 4. DYNAMICS/LOUDNESS
    for (variant, stems) in [("A", &a_stems), ("B", &b_stems), ("C", &c_stems)] {
        let mut s_sum = vec![0.0_f32; signal.len()];
        for i in 0..signal.len() {
            for comp in 0..k {
                s_sum[i] += stems[comp][i];
            }
        }
        let ms_db = rms_db(&s_sum);
        let crest = crest_factor(&s_sum);
        println!(
            "JURY|dynamics|variant={}|ms_db={:.2}|crest={:.2}",
            variant, ms_db, crest
        );
    }

    // 5. CONTINUITY
    for comp in 0..k {
        let stem_name = if comp == scout.voice_idx {
            "voice"
        } else if comp == scout.ambience_idx {
            "ambience"
        } else if comp == scout.bass_idx {
            "bass"
        } else if comp == scout.harmonics_idx {
            "harmonics"
        } else {
            "comp5"
        };

        let audio = &b_stems[comp];
        let mut worst_jump = 0.0_f32;
        for &b in &b_boundaries {
            if b > 0 && b < audio.len() {
                let jump = (audio[b] - audio[b - 1]).abs();
                if jump > worst_jump {
                    worst_jump = jump;
                }
            }
        }

        let mut jumps = Vec::with_capacity(20000);
        for &c in &b_controls {
            for i in c..(c + 2048) {
                if i > 0 && i < audio.len() {
                    jumps.push((audio[i] - audio[i - 1]).abs());
                }
            }
        }
        jumps.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p99 = if !jumps.is_empty() {
            jumps[(jumps.len() as f32 * 0.99) as usize]
        } else {
            0.0
        };
        println!(
            "JURY|continuity|stem={}|worst_jump={:.6}|natural_p99={:.6}",
            stem_name, worst_jump, p99
        );
    }

    // ==========================================
    // P3c AUDIO DRAGON (CORRECTED)
    // ==========================================
    println!("\n=== P3c AUDIO DRAGON (CORRECTED) ===");
    println!("--- COLD INIT (B vs C) ---");
    let c0 = 48000;
    measure_region(
        &c_stems[scout.voice_idx],
        &b_stems[scout.voice_idx],
        c0 - 2048,
        c0 + 2048,
        "control=0",
        "voice",
    );
    measure_region(
        &c_stems[scout.ambience_idx],
        &b_stems[scout.ambience_idx],
        c0 - 2048,
        c0 + 2048,
        "control=0",
        "ambience",
    );

    for b_idx in 0..num_boundaries {
        let b_samp = b_boundaries[b_idx];
        if b_samp + 2048 <= signal.len() {
            let start = b_samp.saturating_sub(2048);
            let end = b_samp + 2048;
            let name = format!("boundary={}", b_idx);
            measure_region(
                &c_stems[scout.voice_idx],
                &b_stems[scout.voice_idx],
                start,
                end,
                &name,
                "voice",
            );
            measure_region(
                &c_stems[scout.ambience_idx],
                &b_stems[scout.ambience_idx],
                start,
                end,
                &name,
                "ambience",
            );
        }
    }

    measure_region(
        &c_stems[scout.voice_idx],
        &b_stems[scout.voice_idx],
        0,
        signal.len(),
        "global_worst",
        "voice",
    );
    measure_region(
        &c_stems[scout.ambience_idx],
        &b_stems[scout.ambience_idx],
        0,
        signal.len(),
        "global_worst",
        "ambience",
    );

    println!("\n=== JURY2 ARBITRATION ===");
    // 1. SYMMETRIC RESPECTIVE BANDS
    for (variant, stems) in [("A", &a_stems), ("B", &b_stems)] {
        let voice_cplx = compute_cplx_spectrogram(&stems[scout.voice_idx]);
        let mut avg_mel = [0.0_f32; 128];
        let mut all_mels = Vec::new();
        for f in &voice_cplx {
            let mut mag = [0.0_f32; 1025];
            for b in 0..1025 {
                mag[b] = libm::sqrtf(f[b].re * f[b].re + f[b].im * f[b].im);
            }
            let mel = sp314_dsp::analysis::mel_128::fold_to_mel(&mag);
            for b in 0..128 {
                avg_mel[b] += mel[b];
            }
            all_mels.push(mel);
        }
        let mut indices: Vec<usize> = (0..128).collect();
        indices.sort_by(|&i, &j| avg_mel[j].partial_cmp(&avg_mel[i]).unwrap());
        let mut is_top16 = [false; 128];
        for &idx in &indices[0..16] {
            is_top16[idx] = true;
        }

        let mut in_band_e = 0.0_f32;
        let mut out_band_e = 0.0_f32;
        for mel in all_mels {
            for b in 0..128 {
                if is_top16[b] {
                    in_band_e += mel[b];
                } else {
                    out_band_e += mel[b];
                }
            }
        }
        let in_band_pct = 100.0 * in_band_e / (in_band_e + out_band_e).max(1e-9);
        println!(
            "JURY2|self_band|variant={}|in_band_pct={:.2}",
            variant, in_band_pct
        );
    }

    // 2. THE VAD JUDGE
    use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
    use sp314_dsp::analysis::vad_model::{FixedPriors, VadClassifier};

    let sources = [
        ("A", &a_stems[scout.voice_idx] as &[f32]),
        ("B", &b_stems[scout.voice_idx] as &[f32]),
        ("mix", &signal as &[f32]),
    ];
    for (source_name, src_audio) in sources {
        let mut ext = VadFeatureExtractor::new();
        let features = ext.process_chunk(src_audio, src_audio, src_audio);
        let mut vad = VadClassifier::new(FixedPriors);
        let mut speech_frames = 0;
        for f in &features {
            let dec = vad.process(f, -144.0);
            if dec.is_speech {
                speech_frames += 1;
            }
        }
        let speech_pct = 100.0 * speech_frames as f32 / features.len().max(1) as f32;
        println!(
            "JURY2|vad|source={}|speech_pct={:.2}",
            source_name, speech_pct
        );
    }

    // 3. ENERGY SPLIT
    for (variant, stems) in [("A", &a_stems), ("B", &b_stems)] {
        let stem_db_val = rms_db(&stems[scout.voice_idx]);
        println!(
            "JURY2|energy|variant={}|stem_db={:.2}",
            variant, stem_db_val
        );
    }
}
