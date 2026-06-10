pub const MPEG1_BARK_BANDS: usize = 25;

pub const BARK_BOUNDARIES_HZ: [f32; 26] = [
    20.0, 100.0, 200.0, 300.0, 400.0, 510.0, 630.0, 770.0, 920.0, 1080.0, 1270.0, 1480.0, 1720.0,
    2000.0, 2320.0, 2700.0, 3150.0, 3700.0, 4400.0, 5300.0, 6400.0, 7700.0, 9500.0, 12000.0,
    15500.0, 20000.0,
];

pub const SYSTEM_NOISE_FLOOR_DB: f32 = 144.0;

pub fn bin_to_bark_band(bin: usize, fft_size: usize, sample_rate: u32) -> usize {
    let freq = bin as f32 * (sample_rate as f32 / fft_size as f32);
    let mut band = 0;
    for i in 0..MPEG1_BARK_BANDS {
        if freq >= BARK_BOUNDARIES_HZ[i] && freq < BARK_BOUNDARIES_HZ[i + 1] {
            band = i;
            break;
        }
    }
    if freq >= BARK_BOUNDARIES_HZ[MPEG1_BARK_BANDS] {
        band = MPEG1_BARK_BANDS - 1;
    }
    band.min(MPEG1_BARK_BANDS - 1)
}

pub fn spreading_attenuation_db(dz: f32) -> f32 {
    let dz = libm::fabsf(dz);
    if dz <= 1.0 {
        17.0 * dz
    } else if dz <= 8.0 {
        17.0 + 10.0 * (dz - 1.0)
    } else {
        87.0
    }
}

pub fn calculate_mask_per_band(
    spectrum_db: &[f32],
    fft_size: usize,
    sample_rate: u32,
    output: &mut [f32; MPEG1_BARK_BANDS],
) {
    let num_bins = (fft_size / 2) + 1;
    debug_assert_eq!(
        spectrum_db.len(),
        num_bins,
        "spectrum_db length must be (fft_size/2)+1, got {}",
        spectrum_db.len()
    );

    let mut band_energies = [0.0; MPEG1_BARK_BANDS];
    for (bin, &val) in spectrum_db.iter().enumerate().take(num_bins) {
        let band = bin_to_bark_band(bin, fft_size, sample_rate);
        let e = libm::powf(10.0, val / 10.0);
        band_energies[band] += e;
    }

    for (i, out) in output.iter_mut().enumerate() {
        let mut mask_e = 0.0;
        for (j, &energy) in band_energies.iter().enumerate() {
            let dz = i as f32 - j as f32;
            let atten_db = spreading_attenuation_db(dz);
            let atten_lin = libm::powf(10.0, atten_db / 10.0);
            mask_e += energy / atten_lin;
        }
        *out = 10.0 * libm::log10f(libm::fmaxf(mask_e, 1e-10));
    }
}

pub fn calculate_mask_per_bin(
    spectrum_db: &[f32],
    fft_size: usize,
    sample_rate: u32,
    output: &mut [f32],
) {
    let num_bins = (fft_size / 2) + 1;
    debug_assert_eq!(
        spectrum_db.len(),
        num_bins,
        "spectrum_db length must be (fft_size/2)+1, got {}",
        spectrum_db.len()
    );
    debug_assert_eq!(
        output.len(),
        num_bins,
        "output length must match spectrum_db"
    );

    let mut band_masks = [0.0; MPEG1_BARK_BANDS];
    calculate_mask_per_band(spectrum_db, fft_size, sample_rate, &mut band_masks);

    for (bin, out) in output.iter_mut().enumerate().take(num_bins) {
        let band = bin_to_bark_band(bin, fft_size, sample_rate);
        *out = band_masks[band];
    }
}
