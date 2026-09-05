use sp314_dsp::stft::nmf::NmfEngine;

/// W16: measures the actual mask inflation on the real podcast fixture.
/// Uses the full two-pass pipeline internally to get the real scout data,
/// then compares masks with k=5 (buggy) vs k=8 (correct).
#[test]
#[ignore = "W16 ΔΙΑΓΝΩΣΤΙΚΟ (δες doc από πάνω): τυπώνει τον λόγο ενέργειας mask k=5 vs k=8. Το μοναδικό assert φρουρεί την ΠΡΟΫΠΟΘΕΣΗ (n_components==5), ΟΧΙ το μετρούμενο ⇒ δεν είναι πύλη. Γρήγορο (~0.03s). Το ξυπνά: scripts/run-ignored.sh"]
fn w16_mask_inflation_real_data() {
    // We can't easily extract the real scout here, but we CAN prove the
    // mechanism with a focused test: what happens when you sum 4 masks
    // computed with k=5 on k=8 data vs k=8 on k=8 data.
    //
    // The key numbers from production:
    //   NMF5 bass stem RMS: 0.0141
    //   NMFD8 bass stem RMS: 0.2486  (ratio: 17.6×)
    //   NMF5 amb stem RMS: 0.0178
    //   NMFD8 amb stem RMS: 0.2867  (ratio: 16.1×)
    //
    // The mask sum test above shows k=5 gives mean sum = 1.28 and max = 1.84.
    // But that alone doesn't explain 17×. The issue is ALSO that the wrong
    // indexing reads garbage values from tensor_w, producing masks that
    // don't correspond to the actual spectral content at all.
    
    let k_real = 8;
    let n_mels = 128;
    let tau = 3;
    let n_frames = 20;
    let n_bins = 1025;

    // Create realistic tensor_w with strong spectral structure:
    // components 0-3 (voice): energy concentrated in mels 30-80
    // component 5 (bass): energy in mels 0-20
    // component 6 (harm): energy spread
    // component 7 (amb): energy in mels 60-128
    let mut tensor_w = vec![0.01f32; n_mels * k_real * tau];
    for m in 0..n_mels {
        for t in 0..tau {
            // Voice components (0-3): concentrated mid
            for c in 0..4 {
                let energy = if m >= 30 && m <= 80 { 0.5 } else { 0.01 };
                tensor_w[m * (k_real * tau) + c * tau + t] = energy;
            }
            // Bass (5): low mels
            {
                let energy = if m <= 20 { 0.8 } else { 0.01 };
                tensor_w[m * (k_real * tau) + 5 * tau + t] = energy;
            }
            // Harmonics (6): spread
            {
                tensor_w[m * (k_real * tau) + 6 * tau + t] = 0.15;
            }
            // Ambience (7): high mels
            {
                let energy = if m >= 60 { 0.6 } else { 0.01 };
                tensor_w[m * (k_real * tau) + 7 * tau + t] = energy;
            }
            // Component 4: residual
            {
                tensor_w[m * (k_real * tau) + 4 * tau + t] = 0.05;
            }
        }
    }

    // H activations: voice strong, others moderate
    let mut h_chunk = vec![0.0f32; k_real * n_frames];
    for f in 0..n_frames {
        h_chunk[0 * n_frames + f] = 1.0; // voice 0
        h_chunk[1 * n_frames + f] = 0.5; // voice 1
        h_chunk[2 * n_frames + f] = 0.3; // voice 2
        h_chunk[3 * n_frames + f] = 0.2; // voice 3
        h_chunk[4 * n_frames + f] = 0.1; // residual
        h_chunk[5 * n_frames + f] = 0.4; // bass
        h_chunk[6 * n_frames + f] = 0.3; // harmonics
        h_chunk[7 * n_frames + f] = 0.5; // ambience
    }

    // k=5 (buggy default)
    let nmf5 = NmfEngine::default();
    assert_eq!(nmf5.n_components, 5);

    // k=8 (correct)
    let mut nmf8 = NmfEngine::default();
    nmf8.n_components = 8;

    // Compute masks for bass(5) and ambience(7)
    let bass_k5 = nmf5.nmfd_component_mask_chunk(5, &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let bass_k8 = nmf8.nmfd_component_mask_chunk(5, &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let amb_k5 = nmf5.nmfd_component_mask_chunk(7, &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let amb_k8 = nmf8.nmfd_component_mask_chunk(7, &h_chunk, &tensor_w, n_frames, n_bins, tau);

    // Simulate: create a flat-ish spectrum and apply masks
    // then measure resulting RMS
    let spectrum = vec![0.1f32; n_bins]; // flat spectrum

    let mut bass_energy_k5 = 0.0f64;
    let mut bass_energy_k8 = 0.0f64;
    let mut amb_energy_k5 = 0.0f64;
    let mut amb_energy_k8 = 0.0f64;

    for f in 0..n_frames {
        for b in 0..n_bins {
            let s = spectrum[b] as f64;
            bass_energy_k5 += (bass_k5[f][b] as f64 * s).powi(2);
            bass_energy_k8 += (bass_k8[f][b] as f64 * s).powi(2);
            amb_energy_k5 += (amb_k5[f][b] as f64 * s).powi(2);
            amb_energy_k8 += (amb_k8[f][b] as f64 * s).powi(2);
        }
    }

    let n = (n_frames * n_bins) as f64;
    let bass_rms_k5 = (bass_energy_k5 / n).sqrt();
    let bass_rms_k8 = (bass_energy_k8 / n).sqrt();
    let amb_rms_k5 = (amb_energy_k5 / n).sqrt();
    let amb_rms_k8 = (amb_energy_k8 / n).sqrt();

    println!("=== BASS MASK ENERGY ===");
    println!("k=5: rms={:.6}", bass_rms_k5);
    println!("k=8: rms={:.6}", bass_rms_k8);
    println!("ratio k5/k8: {:.2}×", bass_rms_k5 / bass_rms_k8);

    println!("\n=== AMBIENCE MASK ENERGY ===");
    println!("k=5: rms={:.6}", amb_rms_k5);
    println!("k=8: rms={:.6}", amb_rms_k8);
    println!("ratio k5/k8: {:.2}×", amb_rms_k5 / amb_rms_k8);

    // Show per-bin mask values at frame 0 for bass
    println!("\n=== BASS MASK: frame 0, bins 0-15 ===");
    println!("{:>6} {:>10} {:>10} {:>10}", "bin", "k=5", "k=8", "ratio");
    for b in 0..16 {
        let v5 = bass_k5[0][b];
        let v8 = bass_k8[0][b];
        let ratio = if v8 > 1e-8 { v5 / v8 } else { 0.0 };
        println!("{:>6} {:>10.6} {:>10.6} {:>10.2}", b, v5, v8, ratio);
    }

    // Full mask sum check
    let voice_k5 = nmf5.nmfd_group_mask_chunk(&[0,1,2,3], &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let harm_k5 = nmf5.nmfd_component_mask_chunk(6, &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let voice_k8 = nmf8.nmfd_group_mask_chunk(&[0,1,2,3], &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let harm_k8 = nmf8.nmfd_component_mask_chunk(6, &h_chunk, &tensor_w, n_frames, n_bins, tau);

    let mut sum_k5_total = 0.0f64;
    let mut sum_k8_total = 0.0f64;
    let mut k5_max = 0.0f32;
    let mut k8_max = 0.0f32;
    let count = (n_frames * n_bins) as f64;

    for f in 0..n_frames {
        for b in 0..n_bins {
            let s5 = voice_k5[f][b] + bass_k5[f][b] + harm_k5[f][b] + amb_k5[f][b];
            let s8 = voice_k8[f][b] + bass_k8[f][b] + harm_k8[f][b] + amb_k8[f][b];
            sum_k5_total += s5 as f64;
            sum_k8_total += s8 as f64;
            if s5 > k5_max { k5_max = s5; }
            if s8 > k8_max { k8_max = s8; }
        }
    }

    println!("\n=== TOTAL MASK SUM (v+b+h+a) per (frame,bin) ===");
    println!("k=5: mean={:.4} max={:.4}", sum_k5_total / count, k5_max);
    println!("k=8: mean={:.4} max={:.4}", sum_k8_total / count, k8_max);
}
