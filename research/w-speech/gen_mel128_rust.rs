use std::fs::File;
use std::io::Write;

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0f32.powf(mel / 2595.0) - 1.0)
}
fn create_mel_filterbank(sr: f32, n_fft: usize, n_mels: usize) -> Vec<Vec<f32>> {
    let n_bins = n_fft / 2 + 1;
    let min_mel = hz_to_mel(0.0);
    let max_mel = hz_to_mel(sr / 2.0);
    let mel_points: Vec<f32> = (0..(n_mels + 2))
        .map(|i| min_mel + i as f32 * (max_mel - min_mel) / (n_mels + 1) as f32)
        .collect();
    let hz_points: Vec<f32> = mel_points.into_iter().map(mel_to_hz).collect();

    let bin_freqs: Vec<f32> = (0..n_bins).map(|i| i as f32 * sr / n_fft as f32).collect();

    let mut fbank = vec![vec![0.0f32; n_bins]; n_mels];
    for i in 0..n_mels {
        let f_m_minus = hz_points[i];
        let f_m = hz_points[i + 1];
        let f_m_plus = hz_points[i + 2];
        for b in 0..n_bins {
            let freq = bin_freqs[b];
            if freq >= f_m_minus && freq <= f_m {
                fbank[i][b] = (freq - f_m_minus) / (f_m - f_m_minus);
            } else if freq >= f_m && freq <= f_m_plus {
                fbank[i][b] = (f_m_plus - freq) / (f_m_plus - f_m);
            }
        }
    }
    fbank
}

fn main() {
    let sr = 48000.0;
    let n_fft = 2048;
    let n_mels = 128;
    let n_bins = n_fft / 2 + 1;
    
    let fbank = create_mel_filterbank(sr, n_fft, n_mels);
    
    // Inverse matrix
    let mut inv_fbank = vec![vec![0.0f32; n_mels]; n_bins];
    let min_mel = hz_to_mel(0.0);
    let max_mel = hz_to_mel(sr / 2.0);
    let mel_points: Vec<f32> = (0..(n_mels + 2))
        .map(|i| min_mel + i as f32 * (max_mel - min_mel) / (n_mels + 1) as f32)
        .collect();
    let hz_points: Vec<f32> = mel_points.into_iter().map(mel_to_hz).collect();
    let bin_freqs: Vec<f32> = (0..n_bins).map(|i| i as f32 * sr / n_fft as f32).collect();

    for b in 0..n_bins {
        let mut col_sum = 0.0f32;
        for m in 0..n_mels {
            col_sum += fbank[m][b];
        }
        if col_sum > 0.0 {
            for m in 0..n_mels {
                inv_fbank[b][m] = fbank[m][b] / col_sum;
            }
        } else {
            let freq = bin_freqs[b];
            let mut nearest_m = 0;
            let mut min_diff = f32::MAX;
            for m in 0..n_mels {
                let diff = (freq - hz_points[m + 1]).abs();
                if diff < min_diff {
                    min_diff = diff;
                    nearest_m = m;
                }
            }
            inv_fbank[b][nearest_m] = 1.0;
        }
    }

    let mut out = String::new();
    out.push_str(&format!("pub const MEL_BANDS: usize = {};\n", n_mels));
    out.push_str(&format!("pub const N_BINS: usize = {};\n\n", n_bins));
    
    out.push_str("pub const MEL_128_MATRIX: [[f32; N_BINS]; MEL_BANDS] = [\n");
    for row in &fbank {
        out.push_str("    [\n");
        for chunk in row.chunks(8) {
            out.push_str("        ");
            for val in chunk {
                out.push_str(&format!("f32::from_bits(0x{:08x}), ", val.to_bits()));
            }
            out.push_str("\n");
        }
        out.push_str("    ],\n");
    }
    out.push_str("];\n\n");
    
    out.push_str("pub const MEL_128_INVERSE: [[f32; MEL_BANDS]; N_BINS] = [\n");
    for row in &inv_fbank {
        out.push_str("    [\n");
        for chunk in row.chunks(8) {
            out.push_str("        ");
            for val in chunk {
                out.push_str(&format!("f32::from_bits(0x{:08x}), ", val.to_bits()));
            }
            out.push_str("\n");
        }
        out.push_str("    ],\n");
    }
    out.push_str("];\n\n");
    
    out.push_str(r#"
#[inline(always)]
pub fn fold_to_mel(frame: &[f32; N_BINS]) -> [f32; MEL_BANDS] {
    let mut out = [0.0; MEL_BANDS];
    for m in 0..MEL_BANDS {
        let mut sum = 0.0;
        for b in 0..N_BINS {
            sum += MEL_128_MATRIX[m][b] * frame[b];
        }
        out[m] = sum;
    }
    out
}

#[inline(always)]
pub fn expand_mask_to_linear(mask: &[f32; MEL_BANDS]) -> [f32; N_BINS] {
    let mut out = [0.0; N_BINS];
    for b in 0..N_BINS {
        let mut sum = 0.0;
        for m in 0..MEL_BANDS {
            sum += MEL_128_INVERSE[b][m] * mask[m];
        }
        out[b] = sum;
    }
    out
}
"#);

    let mut file = File::create("../../lineos/m1/sp314-dsp/src/analysis/mel_128.rs").unwrap();
    file.write_all(out.as_bytes()).unwrap();
}
