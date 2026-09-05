use sp314_dsp::stft::nmf::NmfEngine;

/// W16 diagnostic: proves that nmfd_component_mask_chunk uses
/// k = self.n_components = 5 (NMF5 default) while the tensor_w
/// and h_chunk are structured for k=8. This causes wrong indexing
/// into tensor_w and an incomplete denominator sum, inflating masks.
#[test]
#[ignore = "W16 ΔΙΑΓΝΩΣΤΙΚΟ (δες doc από πάνω): αποδεικνύει το k=5 vs k=8 mismatch τυπώνοντας τιμές. Το assert φρουρεί την ΠΡΟΫΠΟΘΕΣΗ (n_components==5), ΟΧΙ το μετρούμενο ⇒ δεν είναι πύλη. Γρήγορο (~0.01s). Το ξυπνά: scripts/run-ignored.sh"]
fn w16_nmfd_k_mismatch() {
    let k_real = 8usize;
    let k_nmf = 5usize;   // NmfEngine::default().n_components
    let n_mels = 128;
    let tau = 3;
    let n_frames = 4;
    let n_bins = 1025;

    // Verify NmfEngine::default() has n_components = 5
    let nmf = NmfEngine::default();
    println!("nmf.n_components = {}", nmf.n_components);
    assert_eq!(nmf.n_components, 5, "NmfEngine::default is not 5 — re-check");

    // Create tensor_w sized for k=8 (as scout produces)
    let tensor_w: Vec<f32> = (0..(n_mels * k_real * tau))
        .map(|i| ((i as f32) * 0.1).sin().abs() + 0.01)
        .collect();
    // Create h_chunk sized for k=8
    let h_chunk: Vec<f32> = (0..(k_real * n_frames))
        .map(|i| ((i as f32) * 0.2).cos().abs() + 0.01)
        .collect();

    println!("tensor_w.len()={} (expected {})", tensor_w.len(), n_mels * k_real * tau);
    println!("h_chunk.len()={} (expected {})", h_chunk.len(), k_real * n_frames);

    // --- TEST A: k=5 (wrong, as default NmfEngine does) ---
    let mask_k5 = nmf.nmfd_component_mask_chunk(
        6,  // component 6 (would be e.g. harmonics)
        &h_chunk,
        &tensor_w,
        n_frames,
        n_bins,
        tau,
    );

    // --- TEST B: k=8 (correct) ---
    let mut nmf8 = NmfEngine::default();
    nmf8.n_components = 8;
    let mask_k8 = nmf8.nmfd_component_mask_chunk(
        6,
        &h_chunk,
        &tensor_w,
        n_frames,
        n_bins,
        tau,
    );

    // Print some mask values for comparison
    println!("\n=== MASK VALUES (frame 0, first 10 bins) ===");
    println!("{:>6} {:>10} {:>10}", "bin", "k=5", "k=8");
    for b in 0..10 {
        println!("{:>6} {:>10.6} {:>10.6}", b, mask_k5[0][b], mask_k8[0][b]);
    }

    // --- TEST C: sum of ALL masks with k=5 vs k=8 ---
    // With k=5: voice group [0,1,2,3] + bass(5) + harm(6) + amb(7)
    let voice_k5 = nmf.nmfd_group_mask_chunk(&[0,1,2,3], &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let bass_k5 = nmf.nmfd_component_mask_chunk(5, &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let harm_k5 = nmf.nmfd_component_mask_chunk(6, &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let amb_k5 = nmf.nmfd_component_mask_chunk(7, &h_chunk, &tensor_w, n_frames, n_bins, tau);

    let voice_k8 = nmf8.nmfd_group_mask_chunk(&[0,1,2,3], &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let bass_k8 = nmf8.nmfd_component_mask_chunk(5, &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let harm_k8 = nmf8.nmfd_component_mask_chunk(6, &h_chunk, &tensor_w, n_frames, n_bins, tau);
    let amb_k8 = nmf8.nmfd_component_mask_chunk(7, &h_chunk, &tensor_w, n_frames, n_bins, tau);

    // Check mask sums per (frame, bin)
    let mut k5_sum_min = f32::MAX;
    let mut k5_sum_max = f32::MIN;
    let mut k5_sum_total = 0.0f64;
    let mut k8_sum_min = f32::MAX;
    let mut k8_sum_max = f32::MIN;
    let mut k8_sum_total = 0.0f64;
    let mut count = 0u64;

    for f in 0..n_frames {
        for b in 0..n_bins {
            let s5 = voice_k5[f][b] + bass_k5[f][b] + harm_k5[f][b] + amb_k5[f][b];
            let s8 = voice_k8[f][b] + bass_k8[f][b] + harm_k8[f][b] + amb_k8[f][b];
            if s5 < k5_sum_min { k5_sum_min = s5; }
            if s5 > k5_sum_max { k5_sum_max = s5; }
            k5_sum_total += s5 as f64;
            if s8 < k8_sum_min { k8_sum_min = s8; }
            if s8 > k8_sum_max { k8_sum_max = s8; }
            k8_sum_total += s8 as f64;
            count += 1;
        }
    }

    println!("\n=== MASK SUM PER (frame, bin): voice+bass+harm+amb ===");
    println!("k=5: min={:.4} max={:.4} mean={:.4}", k5_sum_min, k5_sum_max, k5_sum_total / count as f64);
    println!("k=8: min={:.4} max={:.4} mean={:.4}", k8_sum_min, k8_sum_max, k8_sum_total / count as f64);

    // --- TEST D: indexing proof ---
    // Show what tensor_w address the code reads with k=5 vs k=8
    // for mel band m=50, component=6, tau=1
    let m = 50;
    let comp = 6;
    let t = 1;
    let addr_k5 = m * (k_nmf * tau) + comp * tau + t;
    let addr_k8 = m * (k_real * tau) + comp * tau + t;
    println!("\n=== INDEXING PROOF ===");
    println!("tensor_w address for (m={}, comp={}, tau={}):", m, comp, t);
    println!("  k=5: tensor_w[{}] = {:.6}", addr_k5, if addr_k5 < tensor_w.len() { tensor_w[addr_k5] } else { f32::NAN });
    println!("  k=8: tensor_w[{}] = {:.6}", addr_k8, if addr_k8 < tensor_w.len() { tensor_w[addr_k8] } else { f32::NAN });
    println!("  DIFFERENT VALUE = {}", (tensor_w.get(addr_k5).unwrap_or(&0.0) - tensor_w.get(addr_k8).unwrap_or(&0.0)).abs() > 1e-10);
    
    // The k=5 address overflows the intended stride — it reads
    // data from DIFFERENT mel bands, creating garbage masks.
    // At m=50 the offset is m*(5*3 - 8*3) = 50 * (15-24) = -450,
    // meaning it's 450 entries BEHIND where it should be.
}
