use sp314_dsp::analysis::mel_128::expand_mask_to_linear;

#[test]
#[ignore = "diagnostic, prints only, no assertions"]
fn w16_mel_inverse_probe() {
    let n_bins = 1025;
    let mel_bands = 128;

    // TEST 1
    let ones_mask = [1.0f32; 128];
    let row_sums = expand_mask_to_linear(&ones_mask);
    
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    let mut sum = 0.0;
    let mut over_1_1 = 0;
    
    let mut sorted_rows: Vec<(usize, f32)> = row_sums.iter().enumerate().map(|(i, &v)| (i, v)).collect();
    
    for (b, &v) in row_sums.iter().enumerate() {
        if v < min { min = v; }
        if v > max { max = v; }
        sum += v;
        if v > 1.1 { over_1_1 += 1; }
    }
    
    sorted_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    println!("=== TEST 1: ROW SUMS ===");
    println!("min={:.4} max={:.4} mean={:.4}", min, max, sum / n_bins as f32);
    println!("Bins > 1.1: {}", over_1_1);
    println!("Top 10:");
    for i in 0..10.min(sorted_rows.len()) {
        let b = sorted_rows[i].0;
        let v = sorted_rows[i].1;
        let freq = b as f32 * 48000.0 / 2048.0;
        println!("  bin {:>4}: val={:.4} freq={:.1}Hz", b, v, freq);
    }
    
    // TEST 2
    let part_mask = [0.25f32; 128];
    let expanded_1 = expand_mask_to_linear(&part_mask);
    let expanded_2 = expand_mask_to_linear(&part_mask);
    let expanded_3 = expand_mask_to_linear(&part_mask);
    let expanded_4 = expand_mask_to_linear(&part_mask);
    
    let mut sum_min = f32::MAX;
    let mut sum_max = f32::MIN;
    let mut sum_sum = 0.0;
    
    for b in 0..n_bins {
        let v = expanded_1[b] + expanded_2[b] + expanded_3[b] + expanded_4[b];
        if v < sum_min { sum_min = v; }
        if v > sum_max { sum_max = v; }
        sum_sum += v;
    }
    
    println!("\n=== TEST 2: EXPANDED SUM ===");
    println!("min={:.4} max={:.4} mean={:.4}", sum_min, sum_max, sum_sum / n_bins as f32);
}
