use sp314_dsp::stft::nmfd::{nmfd_f32, nmfd_f32_seq};
use sp314_dsp::stft::{StreamingStftEncoder, N_BINS};
use std::time::Instant;

const SR: u32 = 48_000;

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0)
}

fn create_mel_filterbank(sr: f32, n_fft: usize, n_mels: usize) -> Vec<Vec<f32>> {
    // Standard triangle mel filterbank (O'Shaughnessy: m = 2595 * log10(1 + f/700))
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

fn run_variant(
    name: &str,
    v: &[f32],
    num_bins: usize,
    num_frames: usize,
    k: usize,
    tau: usize,
    num_iter: usize,
    do_assert: bool,
    run_seq: bool,
) {
    let mut init_w = vec![0.0f32; num_bins * k * tau];
    let mut init_h = vec![0.0f32; k * num_frames];

    let mut lcg_state: u32 = 314159;
    let mut next_rand = || -> f32 {
        lcg_state = lcg_state.wrapping_mul(1664525).wrapping_add(1013904223);
        (lcg_state as f32) / (u32::MAX as f32)
    };

    for x in init_w.iter_mut() {
        *x = next_rand();
    }
    for x in init_h.iter_mut() {
        *x = next_rand();
    }

    let seq_ms = if run_seq {
        let t_seq = Instant::now();
        let _ = nmfd_f32_seq(v, &init_w, &init_h, num_bins, k, num_frames, tau, num_iter);
        t_seq.elapsed().as_millis()
    } else {
        0
    };

    let t_par = Instant::now();
    let (par_w, _par_h, par_cost) =
        nmfd_f32(v, &init_w, &init_h, num_bins, k, num_frames, tau, num_iter);
    let par_ms = t_par.elapsed().as_millis();

    if do_assert {
        let (par2_w, _, _) = nmfd_f32(v, &init_w, &init_h, num_bins, k, num_frames, tau, num_iter);
        assert_eq!(par_w, par2_w, "Determinism failed for tensor_W on {}", name);
    }

    if run_seq {
        println!(
            "WFIT|name={}|frames={}|bins={}|K={}|tau={}|iters={}|seq_ms={}|par_ms={}|cost={}",
            name, num_frames, num_bins, k, tau, num_iter, seq_ms, par_ms, par_cost
        );
    } else {
        println!(
            "WFIT|name={}|frames={}|bins={}|K={}|tau={}|iters={}|par_ms={}|cost={}",
            name, num_frames, num_bins, k, tau, num_iter, par_ms, par_cost
        );
    }
}

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bodleasons_mid.wav")
}

fn decode_wav(path: &std::path::Path) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    assert_eq!(spec.channels, 2, "Expected stereo WAV");
    assert_eq!(spec.sample_rate, SR, "Expected 48kHz");

    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    } else {
        reader
            .samples::<i32>()
            .map(|s| {
                let v = s.unwrap();
                v as f32 / (1 << (spec.bits_per_sample - 1)) as f32
            })
            .collect()
    };

    let mut mono = Vec::with_capacity(samples.len() / 2);
    for chunk in samples.chunks_exact(2) {
        mono.push((chunk[0] + chunk[1]) * 0.5);
    }
    mono
}

#[test]
#[ignore = "ΑΓΝΩΣΤΟΣ ΛΟΓΟΣ — δεν υπάρχει doc/σχόλιο πουθενά. ΤΙ ΠΑΡΑΤΗΡΕΙΤΑΙ: σάρωση κόστους/χρόνου (std::time::Instant) σε 20s του in-repo fixture, ΑΡΓΟ ~42s. Όποιος ξέρει την πρόθεση: γράψ' την εδώ. Το ξυπνά: scripts/run-ignored.sh"]
fn test_nmfd_fit_cost() {
    let path = fixture_path();
    let mono = decode_wav(&path);

    // Take first 20s
    let target_samples = (20.0 * SR as f32) as usize;
    let mono = if mono.len() > target_samples {
        &mono[..target_samples]
    } else {
        &mono[..]
    };

    let mut ctx = StreamingStftEncoder::new();
    let mut frames_cplx = ctx.feed_chunk(mono);
    frames_cplx.extend(ctx.finish());

    let num_frames = frames_cplx.len();

    // Construct V (N_BINS x num_frames)
    let mut magnitude_frames = vec![0.0f32; N_BINS * num_frames];
    for (f, frame) in frames_cplx.iter().enumerate() {
        for (b, c) in frame.iter().enumerate() {
            magnitude_frames[b * num_frames + f] = libm::sqrtf(c.re * c.re + c.im * c.im);
        }
    }

    // Construct 128-band mel filterbank
    let n_mels = 128;
    let fbank = create_mel_filterbank(SR as f32, (N_BINS - 1) * 2, n_mels);

    // Construct B: mel V (n_mels x num_frames)
    let mut mel_v = vec![0.0f32; n_mels * num_frames];
    for m in 0..n_mels {
        for f in 0..num_frames {
            let mut sum = 0.0;
            for b in 0..N_BINS {
                sum += fbank[m][b] * magnitude_frames[b * num_frames + f];
            }
            mel_v[m * num_frames + f] = sum;
        }
    }

    // Construct decimation x2 frames
    let dec_frames = num_frames / 2;

    // Construct C: dec_v (N_BINS x dec_frames) for Control
    let mut dec_v = vec![0.0f32; N_BINS * dec_frames];
    for b in 0..N_BINS {
        for f in 0..dec_frames {
            dec_v[b * dec_frames + f] = magnitude_frames[b * num_frames + f * 2];
        }
    }

    // Construct dec_mel_v (n_mels x dec_frames)
    let mut dec_mel_v = vec![0.0f32; n_mels * dec_frames];
    for m in 0..n_mels {
        for f in 0..dec_frames {
            dec_mel_v[m * dec_frames + f] = mel_v[m * num_frames + f * 2];
        }
    }

    let k = 4;
    let tau = 8;

    // A: BASELINE
    run_variant(
        "A_BASELINE",
        &magnitude_frames,
        N_BINS,
        num_frames,
        k,
        tau,
        20,
        false,
        true,
    );

    // B: BAND-COMPRESSED
    run_variant(
        "B_BAND_COMPRESSED",
        &mel_v,
        n_mels,
        num_frames,
        k,
        tau,
        20,
        false,
        false,
    );

    // C: B + iters=12
    run_variant(
        "C_BAND_COMPRESSED_ITERS12",
        &mel_v,
        n_mels,
        num_frames,
        k,
        tau,
        12,
        false,
        false,
    );

    // D: B + decimation x2, 20 iters
    run_variant(
        "D_BAND_COMPRESSED_DEC2_ITERS20",
        &dec_mel_v,
        n_mels,
        dec_frames,
        k,
        tau,
        20,
        false,
        false,
    );

    // E: B + decimation x2, 12 iters (determinism assert on this one)
    run_variant(
        "E_BAND_COMPRESSED_DEC2_ITERS12",
        &dec_mel_v,
        n_mels,
        dec_frames,
        k,
        tau,
        12,
        true,
        false,
    );

    // F: CONTROL (1025 bins, decimation x2, 12 iters)
    run_variant(
        "F_CONTROL",
        &dec_v,
        N_BINS,
        dec_frames,
        k,
        tau,
        12,
        false,
        false,
    );

    // G: K=14 (Music config golden)
    run_variant(
        "G_K11_MUSIC",
        &dec_mel_v,
        n_mels,
        dec_frames,
        14,
        tau,
        12,
        true,
        false,
    );
}
