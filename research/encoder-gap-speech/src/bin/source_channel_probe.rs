//! RECON 2026-09-18: πόσα από τα εννιά πραγματικά αρχεία είναι
//! μονοφωνικά ΣΤΗΝ ΠΗΓΗ (container probe, πριν από οποιοδήποτε
//! mono→stereo). Read-only, ΜΗΔΕΝ αλλαγή στην παραγωγή.
//!
//! ΔΕΝ καλεί το `decode_raw_interleaved` της παραγωγής απευθείας:
//! εκείνο έχει `MAX_DURATION_SECS=720` (12 λεπτά, decode.rs:23) και
//! ΟΛΑ τα εννιά κεφάλαια εκτός τριών το ξεπερνούν (13-37 λεπτά) —
//! το όριο ελέγχεται ΜΕΤΑ την πλήρη αποκωδικοποίηση, άρα δεν υπάρχει
//! τρόπος να το προσπεράσεις καλώντας την ίδια συνάρτηση.
//!
//! Αντ' αυτού: ΑΚΡΙΒΕΣ ΑΝΤΙΓΡΑΦΟ μόνο του probe-σκέλους
//! (decode.rs:191-236, πριν την παραγωγή του duration check) — ίδιες
//! κλήσεις symphonia (get_probe/format/tracks/codec_params.channels),
//! ΧΩΡΙΣ decode πλήρους σήματος, ΧΩΡΙΣ έλεγχο διάρκειας. Διαβάζει τον
//! ίδιο αριθμό καναλιών που θα διάβαζε η παραγωγή, απλά χωρίς να
//! σταματήσει σε μακριά αρχεία.
//! ΧΡΗΣΗ: cargo run --release --bin source_channel_probe
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// ΑΝΤΙΓΡΑΦΟ decode.rs:191-236 — ΜΟΝΟ το probe, όχι ο βρόχος decode.
fn probe_channels_and_sr(path: &str) -> Result<(u16, u32), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("open: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path).extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions { enable_gapless: true, ..Default::default() };
    let metadata_opts = MetadataOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| format!("probe: {e}"))?;

    let format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| "no audio track".to_string())?;

    let original_sr = track.codec_params.sample_rate.ok_or("missing sample rate")?;
    let original_ch = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(2);

    Ok((original_ch, original_sr))
}

fn main() {
    let dir = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    let books = [
        ("secretgarden", "secretgarden_01_burnett.mp3"),
        ("dracula", "dracula_01_stoker.mp3"),
        ("count_of_monte_cristo", "count_of_monte_cristo_001_dumas.mp3"),
        ("peterpan", "peterpan_01_barrie.mp3"),
        ("anne_of_green_gables", "anne_of_green_gables_01_montgomery.mp3"),
        ("tale_of_two_cities", "tale_of_two_cities_01_dickens.mp3"),
        ("adventurespinocchio", "adventurespinocchio_01_collodi.mp3"),
        ("huckfinn", "huckfinn_01_twain_apc.mp3"),
        ("janeeyre", "janeeyre_01_bronte.mp3"),
    ];

    println!("=== ΚΑΝΑΛΙΑ ΠΗΓΗΣ (container probe, ΑΝΤΙΓΡΑΦΟ decode.rs:191-236) ===\n");
    let mut n_mono = 0usize;
    for (label, fname) in books {
        let path = format!("{dir}/{fname}");
        match probe_channels_and_sr(&path) {
            Ok((ch, sr)) => {
                println!("{label:<24} κανάλια_πηγής={ch}  sample_rate_πηγής={sr}Hz");
                if ch == 1 {
                    n_mono += 1;
                }
            }
            Err(e) => println!("{label:<24} ΣΦΑΛΜΑ probe: {e}"),
        }
    }
    println!("\nΣύνολο μονοφωνικών στην πηγή: {n_mono}/9");
}
