use hound;
use sp314_dsp::stft::{StftEngine, hpss::HpssProcessor};

fn load_wav(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap();
    reader.samples::<f32>().map(|s| s.unwrap()).collect()
}

fn write_wav(path: &str, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for &s in samples {
        writer.write_sample(s).unwrap();
    }
}

fn main() {
    let base = "research/musdb-lab/excerpts/Forkupines_-_Semantics/";
    let bass = load_wav(&format!("{}bass.wav", base));
    let drums = load_wav(&format!("{}drums.wav", base));
    let other = load_wav(&format!("{}other.wav", base));

    let n = bass.len();
    let mut mat_a = vec![0.0_f32; n];
    for i in 0..n {
        mat_a[i] = bass[i] + drums[i] + other[i];
    }
    write_wav("research/musdb-lab/material_A.wav", &mat_a);

    let mut stft = StftEngine::new();
    let (mut cplx, n_frames) = stft.forward(&mat_a);

    let mut magnitudes = vec![vec![0.0_f32; 1025]; n_frames];
    for f in 0..n_frames {
        for b in 0..1025 {
            let c = cplx[f][b];
            magnitudes[f][b] = libm::sqrtf(c.re * c.re + c.im * c.im);
        }
    }

    let hpss = HpssProcessor::new();
    let (_mask_h, mask_p) = hpss.process(&magnitudes);

    for f in 0..n_frames {
        for b in 0..1025 {
            let harmonic_mask = 1.0 - mask_p[f][b];
            cplx[f][b].re *= harmonic_mask;
            cplx[f][b].im *= harmonic_mask;
        }
    }

    let mat_b = stft.inverse(&cplx, n);
    
    let mut stft2 = StftEngine::new();
    let (cplx_a, _) = stft2.forward(&mat_a);
    let mat_a_delayed = stft2.inverse(&cplx_a, n);
    
    write_wav("research/musdb-lab/material_A.wav", &mat_a_delayed[..n]);
    write_wav("research/musdb-lab/material_B.wav", &mat_b[..n]);
}
