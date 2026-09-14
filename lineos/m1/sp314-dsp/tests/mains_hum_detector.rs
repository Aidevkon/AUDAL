//! Επαλήθευση του ανιχνευτή (analysis::mains_hum::detect_mains_line) πάνω
//! στα εφτά δοκίμια γνωστής απάντησης. Διαβάζει το manifest.json — δεν
//! γράφει νούμερα μέσα του. Οι τιμές αναφοράς είναι οι `_decim1k`
//! (commit 01259fa), το ίδιο όργανο με τον ανιχνευτή.
//!
//! ⚠ ΑΝΟΧΗ, ΔΗΛΩΜΕΝΗ ΠΡΙΝ ΤΡΕΞΕΙ: προεξοχή ±0.5 dB, συχνότητα ±0.5 Hz.
//! Αιτιολογία: το detect_mains_line βρίσκει δυναμικά την κορυφή μέσα στη
//! ζώνη 40-75Hz (peak_in_band) αντί να στοχεύει απευθείας το γνωστό
//! hum_hz όπως έκανε το μετρητικό scratch (hum_detector_methods.rs) — για
//! καθαρό τόνο αναμένεται να πέσουν στο ΙΔΙΟ κάδο, η ανοχή καλύπτει
//! μικροδιαφορές υλοποίησης, όχι αβεβαιότητα για το αν δουλεύει.

use serde_json::Value;
use sp314_dsp::analysis::mains_hum::{detect_mains_line, longest_run_below, otsu_pause_threshold_db};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const PROMINENCE_TOL_DB: f32 = 0.5;
const FREQ_TOL_HZ: f32 = 0.5;

fn decode_mono(path: &str) -> (Vec<f32>, u32) {
    let file = Box::new(std::fs::File::open(path).unwrap_or_else(|e| panic!("{path}: {e}")));
    let mss = MediaSourceStream::new(file, Default::default());
    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .expect("probe");
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .expect("track")
        .clone();
    let sr = track.codec_params.sample_rate.expect("sample_rate");
    let channels = track
        .codec_params
        .channels
        .map(|c| c.count())
        .unwrap_or(1);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .expect("decoder");

    let mut mono = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != track.id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let spec = *decoded.spec();
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();
        if channels >= 2 {
            for frame in samples.chunks_exact(channels) {
                mono.push((frame[0] + frame[1]) * 0.5);
            }
        } else {
            mono.extend_from_slice(samples);
        }
    }
    (mono, sr)
}

fn pause_for(mono: &[f32], sr: u32) -> Vec<f32> {
    let thr = otsu_pause_threshold_db(mono, sr).expect("το δοκίμιο δεν έδωσε τομή Otsu");
    let (a, n) = longest_run_below(mono, sr, thr, None).expect("το δοκίμιο δεν έχει παύση");
    mono[a..a + n].to_vec()
}

#[test]
fn detects_seven_known_fixtures() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hum_detector");
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{dir}/manifest.json")).expect("manifest.json"),
    )
    .expect("valid JSON");

    let mut checked = 0usize;
    let mut failures = Vec::new();

    for f in manifest["fixtures"].as_array().expect("fixtures array") {
        let file = f["file"].as_str().expect("file");
        let path = format!("{dir}/{file}");
        let (mono, sr) = decode_mono(&path);
        let pause = pause_for(&mono, sr);
        let line = detect_mains_line(&pause, sr).expect("η παύση δεν είναι πολύ κοντή για FFT");

        let (expect_hz, expect_prom) = if file == "hum_none.flac" {
            // Το αρνητικό: καμία εγχυμένη γραμμή, αλλά ο φορέας έχει δικό
            // του υπόλειμμα δικτύου στα 60Hz (manifest carrier_residual_db).
            (
                60.0_f32,
                f["measured_prominence_at_60hz_db_decim1k"]
                    .as_f64()
                    .expect("measured_prominence_at_60hz_db_decim1k") as f32,
            )
        } else {
            (
                f["hum_hz"].as_f64().expect("hum_hz") as f32,
                f["measured_prominence_db_decim1k"]
                    .as_f64()
                    .expect("measured_prominence_db_decim1k") as f32,
            )
        };

        checked += 1;
        let hz_err = (line.hz - expect_hz).abs();
        let prom_err = (line.prominence_db - expect_prom).abs();
        if hz_err > FREQ_TOL_HZ || prom_err > PROMINENCE_TOL_DB {
            failures.push(format!(
                "{file}: βρέθηκε hz={:.3} prom={:.3}dB, αναμενόταν hz={:.2}±{FREQ_TOL_HZ} prom={:.2}±{PROMINENCE_TOL_DB}dB (Δhz={:.3} Δprom={:.3})",
                line.hz, line.prominence_db, expect_hz, expect_prom, hz_err, prom_err
            ));
        }
    }

    assert_eq!(checked, 7, "το manifest δεν έχει πια εφτά δοκίμια");
    assert!(
        failures.is_empty(),
        "{}/{} δοκίμια εκτός ανοχής:\n{}",
        failures.len(),
        checked,
        failures.join("\n")
    );
}
