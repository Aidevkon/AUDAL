//! Παράγει το τέταρτο ζεύγος ακρόασης OFF/ON για tale_of_two_cities, στο
//! ΙΔΙΟ παράθυρο ομιλίας που ήδη βρήκε το Σημείο Β του
//! lowcut_rumble_audition.rs (263.4s, 3.00s — docs/lab-logs/
//! lowcut-rumble-20260915.txt). ΜΗΔΕΝ νέα μέτρηση: η θέση, η θεμελιώδης
//! (111.33Hz) και οι στάθμες OFF/ON (-0.79/-1.81dB) έρχονται αυτούσιες
//! από το ήδη τρεγμένο log. Το μόνο νέο εδώ είναι η εξαγωγή 10s WAV.
//!
//! decode_dump/read_dump_stereo/write_wav: ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ από
//! lowcut_rumble_audition.rs (δεν το αγγίζει, δεν το εισάγει — bin
//! crate, δεν έχει δημόσιο lib API να επαναχρησιμοποιηθεί).
//!
//! ΜΗΔΕΝ κώδικας παραγωγής. ΜΗΔΕΝ κατώφλι.

use sp314_dsp::restoration::{RestorationChain, RestorationConfig};

const SR: f32 = 48_000.0;

fn decode_dump(path: &str, tag: &str) -> String {
    let dump = format!("/tmp/lowcut_rumble_{tag}.raw");
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .expect("pass0 decode");
    dump
}

fn read_dump_stereo(dump: &str) -> (Vec<f32>, Vec<f32>) {
    let bytes = std::fs::read(dump).expect("read dump");
    let n = bytes.len() / 4;
    let mut inter = Vec::with_capacity(n);
    for i in 0..n {
        inter.push(f32::from_le_bytes([
            bytes[i * 4],
            bytes[i * 4 + 1],
            bytes[i * 4 + 2],
            bytes[i * 4 + 3],
        ]));
    }
    let l: Vec<f32> = inter.chunks_exact(2).map(|f| f[0]).collect();
    let r: Vec<f32> = inter.chunks_exact(2).map(|f| f[1]).collect();
    (l, r)
}

fn write_wav(path: &str, l: &[f32], r: &[f32]) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("create wav");
    for i in 0..l.len() {
        w.write_sample(l[i]).unwrap();
        w.write_sample(r[i]).unwrap();
    }
    w.finalize().unwrap();
}

fn main() {
    let name = "tale_of_two_cities_01_dickens";
    let prominence_ref = 3.49_f32;
    // ΘΕΣΗ ΑΠΟ ΤΟ lab-log (Σημείο Β), ΟΧΙ ξανα-υπολογισμένη:
    let speech_start_s = 263.4_f32;
    let path = format!(
        "/home/aidevcon/Downloads/DATASET/librivox-hq/{name}.mp3"
    );

    let off_dump = decode_dump(&path, &format!("{name}_clip4"));
    let (l0, r0) = read_dump_stereo(&off_dump);

    let mut chain_on = RestorationChain::new(
        SR,
        RestorationConfig {
            lowcut_enabled: true,
            hum_enabled: false,
            deess_enabled: false,
            gate_enabled: false,
        },
        0.0,
        f32::NEG_INFINITY,
    );
    let mut l1 = l0.clone();
    let mut r1 = r0.clone();
    chain_on.process(&mut l1, &mut r1);

    let start_frame = (speech_start_s * SR) as usize;
    let len_frame = (10.0 * SR) as usize;
    let end_frame = (start_frame + len_frame).min(l0.len());

    let out_dir = "/tmp/lowcut-rumble-audition";
    let _ = std::fs::create_dir_all(out_dir);
    let off_path = format!(
        "{out_dir}/{name}_OFF_prom{prominence_ref:.2}dB_speech@{speech_start_s:.1}s.wav"
    );
    let on_path = format!(
        "{out_dir}/{name}_ON_prom{prominence_ref:.2}dB_speech@{speech_start_s:.1}s.wav"
    );
    write_wav(&off_path, &l0[start_frame..end_frame], &r0[start_frame..end_frame]);
    write_wav(&on_path, &l1[start_frame..end_frame], &r1[start_frame..end_frame]);

    println!("θεμελιώδης (από το lab-log): 111.33 Hz · OFF=-0.79dB ON=-1.81dB (Δ=-1.02dB, αναμενόμενη -1.03dB)");
    println!("παράθυρο: @{speech_start_s:.1}s, 10.0s (Σημείο Β βρήκε 3.00s στο ίδιο ξεκίνημα — εδώ επεκτάθηκε σε 10s ίδιο μήκος με τα άλλα τρία)");
    println!("{off_path}");
    println!("{on_path}");

    let _ = std::fs::remove_file(&off_dump);
}
