//! ORACLE-SDR: separation quality of FiveStemRenderer against
//! REAL isolated stems (MUSDB18HQ) — not the synthetic signals
//! of four_stem_contract.rs.
//!
//! sdr_db() and StemSdr (analysis/sdr.rs:8,24) have had zero
//! callers since they were written. This test is the caller
//! they were built for.
//!
//! ΟΡΓΑΝΟ ΜΕΤΡΗΣΗΣ, ΟΧΙ GATE: κανένα assert σε τιμή SDR. Τα
//! κατώφλια (SDR > 6dB, sdr.rs:3,7) ζουν στους φρουρούς — αυτό
//! εδώ μόνο τυπώνει, μηχανικά parseable.
//!
//! ΣΥΜΒΑΣΗ MAPPING 5→4 — ΤΟΥ ΟΡΓΑΝΟΥ, όχι του separator:
//!   voice                → vocals
//!   bass                 → bass
//!   drums                → drums
//!   harmonics + ambience → other   (MUSDB "other" είναι catch-all
//!                                   guitars/synths/pads/FX — το
//!                                   μόνο ground-truth stem που
//!                                   ταιριάζει και στα δύο δικά
//!                                   μας catch-all buckets)
//!
//! RECON (πριν γραφτεί αυτό το αρχείο) — πώς το
//! four_stem_contract.rs καλεί τον separation δρόμο
//! (four_stem_contract.rs:1-14):
//!   use sp314_dsp::stft::stem_renderer::FiveStemRenderer;
//!   let mut renderer = FiveStemRenderer::new();
//!   let stems = renderer.render(&signal);   // signal: &[f32], MONO
//! Ίδια κλήση, byte-for-byte, εδώ.
//!
//! SAMPLE RATE: FiveStemRenderer::render() καλεί εσωτερικά
//! find_most_diverse_window(signal, 48000, 10.0) — σκληρό-
//! κωδικοποιημένο 48000 (stem_renderer.rs:143). Το MUSDB18HQ
//! είναι 44100Hz ⇒ resample ΥΠΟΧΡΕΩΤΙΚΟ, ΚΑΙ για το mixture ΚΑΙ
//! για τα 4 ground-truth stems, με ΤΟΝ ΙΔΙΟ resampler (rubato
//! SincFixedIn, ίδιες παράμετροι με fixture_factory.rs:108-113) —
//! ίδια μεταχείριση παντού, δίκαιη σύγκριση.

use hound::WavReader;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use sp314_dsp::analysis::sdr::sdr_db;
use sp314_dsp::stft::stem_renderer::FiveStemRenderer;
use std::path::PathBuf;

const MUSDB_SR: u32 = 44_100;
const RENDER_SR: u32 = 48_000;

/// Read a MUSDB18HQ wav (stereo, 44.1kHz) and fold to mono (L+R)/2.
fn read_mono(path: &std::path::Path) -> Vec<f32> {
    let mut reader = WavReader::open(path)
        .unwrap_or_else(|e| panic!("failed to open {}: {}", path.display(), e));
    let spec = reader.spec();
    assert_eq!(spec.channels, 2, "{} is not stereo", path.display());
    let samples: Vec<i16> = reader.samples().map(|s| s.unwrap()).collect();
    samples
        .chunks_exact(2)
        .map(|c| {
            let l = c[0] as f32 / 32768.0;
            let r = c[1] as f32 / 32768.0;
            (l + r) * 0.5
        })
        .collect()
}

/// 44.1kHz mono → 48kHz mono. Same resampler + params for the
/// mixture and every ground-truth stem (fairness — see module doc).
fn resample_to_48k(mono: &[f32]) -> Vec<f32> {
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedIn::<f32>::new(
        RENDER_SR as f64 / MUSDB_SR as f64,
        2.0,
        params,
        mono.len(),
        1,
    )
    .unwrap();
    let waves_out = resampler.process(&[mono.to_vec()], None).unwrap();
    waves_out[0].clone()
}

#[test]
#[ignore = "MUSDB SDR oracle — needs local musdb18hq (personal machine), ~Xs"]
fn oracle_sdr_musdb() {
    let track_dir = std::env::var("MUSDB_TRACK_DIR").unwrap_or_else(|_| {
        panic!(
            "MUSDB_TRACK_DIR not set — this is a measurement instrument, it does \
             not skip silently (κανόνας Γ). Set it to a MUSDB18HQ track folder \
             containing mixture/bass/drums/vocals/other.wav, e.g.:\n  \
             MUSDB_TRACK_DIR=\"$HOME/Downloads/DATASET/musdb18hq/test/<track>\" \
             cargo test -p sp314-dsp --test oracle_sdr_musdb -- --ignored --nocapture"
        )
    });
    let track_dir = PathBuf::from(track_dir);
    let track_name = track_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mixture_raw = read_mono(&track_dir.join("mixture.wav"));
    let bass_raw = read_mono(&track_dir.join("bass.wav"));
    let drums_raw = read_mono(&track_dir.join("drums.wav"));
    let vocals_raw = read_mono(&track_dir.join("vocals.wav"));
    let other_raw = read_mono(&track_dir.join("other.wav"));

    // 5 αρχεία φορτώθηκαν (όχι placeholder) + το βασικό MUSDB
    // invariant: mixture και κάθε stem έχουν ΙΔΙΟ πλήθος frames
    // στην πηγή, πριν από οποιοδήποτε resample.
    assert!(!mixture_raw.is_empty(), "mixture.wav loaded empty");
    assert_eq!(mixture_raw.len(), bass_raw.len(), "bass.wav length mismatch vs mixture");
    assert_eq!(mixture_raw.len(), drums_raw.len(), "drums.wav length mismatch vs mixture");
    assert_eq!(mixture_raw.len(), vocals_raw.len(), "vocals.wav length mismatch vs mixture");
    assert_eq!(mixture_raw.len(), other_raw.len(), "other.wav length mismatch vs mixture");

    let mixture = resample_to_48k(&mixture_raw);
    let bass_gt = resample_to_48k(&bass_raw);
    let drums_gt = resample_to_48k(&drums_raw);
    let vocals_gt = resample_to_48k(&vocals_raw);
    let other_gt = resample_to_48k(&other_raw);

    // ΙΔΙΑ κλήση με four_stem_contract.rs (βλ. RECON στο module doc).
    let mut renderer = FiveStemRenderer::new();
    let stems = renderer.render(&mixture);

    // ΣΥΜΒΑΣΗ mapping (module doc): harmonics + ambience → other.
    let other_est: Vec<f32> = stems
        .harmonics
        .iter()
        .zip(stems.ambience.iter())
        .map(|(h, a)| h + a)
        .collect();

    // Κοινό μήκος: ανεξάρτητα resamples στα ground-truth κανάλια
    // μπορεί να διαφέρουν κατά 1-2 δείγματα από το renderer output,
    // που έχει ΑΚΡΙΒΩΣ το μήκος του mixture (four_stem_contract.rs:10).
    let n = [
        mixture.len(),
        bass_gt.len(),
        drums_gt.len(),
        vocals_gt.len(),
        other_gt.len(),
        stems.bass.len(),
    ]
    .into_iter()
    .min()
    .unwrap();

    // Περιθώριο άκρων — ίδιο μοτίβο με four_stem_contract.rs
    // (margin = fft_size, γρ. 37-39): τα άκρα του STFT/iSTFT έχουν
    // artifacts άσχετα με την ποιότητα διαχωρισμού.
    const FFT_SIZE: usize = 2048;
    let margin = FFT_SIZE.min(n / 4);
    let lo = margin;
    let hi = n - margin;
    assert!(lo < hi, "track too short for margin trim: n={n}");

    let sdr_vocals = sdr_db(&vocals_gt[lo..hi], &stems.voice[lo..hi]);
    let sdr_bass = sdr_db(&bass_gt[lo..hi], &stems.bass[lo..hi]);
    let sdr_drums = sdr_db(&drums_gt[lo..hi], &stems.drums[lo..hi]);
    let sdr_other = sdr_db(&other_gt[lo..hi], &other_est[lo..hi]);

    // Sanity δίπλα στα per-stem: SDR του mixture έναντι του
    // αθροίσματος ΟΛΩΝ των ΕΚΤΙΜΩΜΕΝΩΝ (όχι ground-truth) stems.
    let recon_est: Vec<f32> = (lo..hi)
        .map(|i| {
            stems.bass[i] + stems.harmonics[i] + stems.voice[i] + stems.drums[i]
                + stems.ambience[i]
        })
        .collect();
    let sdr_recon = sdr_db(&mixture[lo..hi], &recon_est);

    println!(
        "[ORACLE-SDR] track={} vocals={:.2} bass={:.2} drums={:.2} other={:.2} recon={:.2}",
        track_name, sdr_vocals, sdr_bass, sdr_drums, sdr_other, sdr_recon
    );
}
