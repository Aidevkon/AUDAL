use sp314_dsp::stft::nmf::NmfEngine;

#[test]
fn test_nmf_semantic_isolation() {
    let n_bins = 32_usize;
    let n_frames = 16_usize;
    let mut frames = vec![vec![0.0_f32; n_bins]; n_frames];

    // Deterministic noise — xorshift32 seed=42
    let mut seed: u32 = 42;
    let mut next_rand = || -> f32 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };

    // Build synthetic V matrix (matches Python fixture structure)
    for f in 0..n_frames {
        for b in 0..n_bins {
            let mut val = 0.0_f32;
            if b < 8 {
                val += 0.8_f32;
            }
            if (8..20).contains(&b) && (4..12).contains(&f) {
                val += 0.7_f32;
            }
            if !(4..12).contains(&f) {
                val += 0.3_f32;
            }
            val += next_rand() * 0.05_f32;
            frames[f][b] = val;
        }
    }

    let mut engine = NmfEngine::default();
    engine.fit(&frames);

    // Reconstruction MSE
    let mut mse = 0.0_f32;
    for f in 0..n_frames {
        for b in 0..n_bins {
            let mut approx = 0.0_f32;
            for c in 0..sp314_dsp::stft::nmf::N_COMPONENTS {
                approx += engine.w[b * sp314_dsp::stft::nmf::N_COMPONENTS + c]
                    * engine.h[c * n_frames + f];
            }
            let diff = frames[f][b] - approx;
            mse += diff * diff;
        }
    }
    mse /= (n_bins * n_frames) as f32;
    println!("Reconstruction MSE: {:.6}", mse);
    assert!(mse < 0.1_f32, "NMF failed to converge. MSE: {:.6}", mse);

    // Spectral centroids
    let centroids = engine.centroids(n_bins);
    for c in 0..sp314_dsp::stft::nmf::N_COMPONENTS {
        println!("Component {} centroid: {:.2}", c, centroids[c]);
    }

    // Sort by centroid — deterministic float sort
    let mut sorted: Vec<usize> = (0..sp314_dsp::stft::nmf::N_COMPONENTS).collect();
    sorted.sort_by(|&a, &b| centroids[a].total_cmp(&centroids[b]));

    println!(
        "Bass     component: {} (centroid {:.2})",
        sorted[0], centroids[sorted[0]]
    );
    println!(
        "Drums    component: {} (centroid {:.2})",
        sorted[1], centroids[sorted[1]]
    );
    println!(
        "Mid      component: {} (centroid {:.2})",
        sorted[2], centroids[sorted[2]]
    );
    println!(
        "Harmonic component: {} (centroid {:.2})",
        sorted[3], centroids[sorted[3]]
    );
    println!(
        "Other    component: {} (centroid {:.2})",
        sorted[4], centroids[sorted[4]]
    );

    // Bass must be in low frequency range
    assert!(
        centroids[sorted[0]] < 10.0_f32,
        "Bass centroid too high: {:.2}",
        centroids[sorted[0]]
    );

    // Highest component must be broadband/high freq
    assert!(
        centroids[sorted[4]] > 10.0_f32,
        "Highest component centroid too low: {:.2}",
        centroids[sorted[4]]
    );
}

#[test]
fn test_nmf_masks_sum_to_unity() {
    use sp314_dsp::stft::nmf::NmfEngine;

    let n_bins = 8_usize;
    let n_frames = 4_usize;
    let frames = vec![vec![0.5_f32; n_bins]; n_frames];
    let mut engine = NmfEngine::default();
    engine.fit(&frames);

    let mut max_err = 0.0_f32;
    for f in 0..n_frames {
        for b in 0..n_bins {
            let sum: f32 = (0..sp314_dsp::stft::nmf::N_COMPONENTS)
                .map(|c| engine.component_mask(c, n_bins, n_frames)[f][b])
                .sum();
            let err = (sum - 1.0_f32).abs();
            if err > max_err {
                max_err = err;
            }
        }
    }
    println!("Max mask sum error: {:.2e}", max_err);
    assert!(max_err < 1e-4_f32, "Masks don't sum to 1: {:.2e}", max_err);
}
