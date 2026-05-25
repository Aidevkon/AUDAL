pub const ISO226_FREQS_HZ: [f32; 29] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0,
    200.0, 250.0, 315.0, 400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0,
    2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0, 8000.0, 10000.0, 12500.0
];

pub const ISO226_80PHON_CORRECTION_DB: [f32; 29] = [
    25.8, 19.8, 13.9, 8.2, 2.5, -3.5, -9.3, -14.1, -18.1, -22.3,
    -25.8, -29.1, -31.6, -33.2, -33.8, -33.5, -32.2, -30.0, -27.5, -25.7,
    -27.0, -29.5, -32.3, -34.6, -35.1, -33.5, -30.4, -25.9, -20.5
];

pub const ISO226_90PHON_CORRECTION_DB: [f32; 29] = [
    16.0, 10.0, 4.3, -1.3, -6.8, -12.6, -18.3, -23.1, -27.0, -31.1,
    -34.5, -37.7, -40.1, -41.7, -42.2, -41.9, -40.6, -38.4, -35.9, -34.1,
    -35.4, -37.8, -40.6, -42.9, -43.4, -41.8, -38.7, -34.2, -28.8
];

fn get_tangent(table: &[f32; 29], idx: usize) -> f32 {
    let x = |i: usize| ISO226_FREQS_HZ[i];
    if idx == 0 {
        (table[1] - table[0]) / (x(1) - x(0))
    } else if idx == 28 {
        (table[28] - table[27]) / (x(28) - x(27))
    } else {
        (table[idx + 1] - table[idx - 1]) / (x(idx + 1) - x(idx - 1))
    }
}

fn interpolate_table(freq: f32, table: &[f32; 29]) -> f32 {
    let mut idx = 0;
    while idx < 28 && ISO226_FREQS_HZ[idx + 1] <= freq {
        idx += 1;
    }
    
    if idx == 28 {
        return table[28];
    }
    if freq == ISO226_FREQS_HZ[idx] {
        return table[idx];
    }
    
    let x1 = ISO226_FREQS_HZ[idx];
    let x2 = ISO226_FREQS_HZ[idx + 1];
    let y1 = table[idx];
    let y2 = table[idx + 1];
    let m1 = get_tangent(table, idx);
    let m2 = get_tangent(table, idx + 1);
    
    let h = x2 - x1;
    let t = (freq - x1) / h;
    let t2 = t * t;
    let t3 = t2 * t;
    
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    
    h00 * y1 + h10 * h * m1 + h01 * y2 + h11 * h * m2
}

pub fn interpolate_iso226_correction(freq_hz: f32, phon_level: f32) -> f32 {
    let freq_hz = libm::fmaxf(ISO226_FREQS_HZ[0], libm::fminf(ISO226_FREQS_HZ[28], freq_hz));
    let phon = libm::fmaxf(80.0, libm::fminf(90.0, phon_level));
    
    let corr_80 = interpolate_table(freq_hz, &ISO226_80PHON_CORRECTION_DB);
    let corr_90 = interpolate_table(freq_hz, &ISO226_90PHON_CORRECTION_DB);
    
    let t = (phon - 80.0) / 10.0;
    corr_80 + t * (corr_90 - corr_80)
}

pub fn apply_equal_loudness_weighting(spectrum_db: &mut [f32], sample_rate: u32, fft_size: usize, target_phon: f32) {
    let num_bins = (fft_size / 2) + 1;
    debug_assert_eq!(spectrum_db.len(), num_bins, "spectrum_db length must be (fft_size/2)+1");
    
    let phon = libm::fmaxf(80.0, libm::fminf(90.0, target_phon));
    
    for (bin, item) in spectrum_db.iter_mut().enumerate().take(num_bins) {
        let freq_hz = bin as f32 * (sample_rate as f32 / fft_size as f32);
        let corr = interpolate_iso226_correction(freq_hz, phon);
        *item += corr;
    }
}
