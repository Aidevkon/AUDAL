//! Ζεύγη ακρόασης 80Hz έναντι Σχεδίου Β (ΟΧΙ OFF/ON) για δύο αρχεία,
//! στα ΙΔΙΑ παράθυρα ομιλίας που ήδη χρησιμοποιήθηκαν:
//!   secretgarden @352.0s (το ρίσκο — γυναικεία, γωνία Β ανεβαίνει σε 123.045Hz)
//!   tale         @263.4s (το κέρδος — ανδρική, γωνία Β κατεβαίνει σε 55.665Hz)
//!
//! decode_dump/read_dump_stereo/write_wav/HpBiquad: ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ
//! από lowcut_rumble_audition.rs / lowcut_plan_b_sweep.rs.
//!
//! ΜΗΔΕΝ αλλαγή παραγωγής. ΜΗΔΕΝ κατώφλι. ΜΗΔΕΝ κρίση.

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
            bytes[i * 4], bytes[i * 4 + 1], bytes[i * 4 + 2], bytes[i * 4 + 3],
        ]));
    }
    let l: Vec<f32> = inter.chunks_exact(2).map(|f| f[0]).collect();
    let r: Vec<f32> = inter.chunks_exact(2).map(|f| f[1]).collect();
    (l, r)
}

fn write_wav(path: &str, l: &[f32], r: &[f32]) {
    let spec = hound::WavSpec { channels: 2, sample_rate: SR as u32, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(path, spec).expect("create wav");
    for i in 0..l.len() {
        w.write_sample(l[i]).unwrap();
        w.write_sample(r[i]).unwrap();
    }
    w.finalize().unwrap();
}

struct HpBiquad { b0: f32, b1: f32, b2: f32, a1: f32, a2: f32, z1_l: f32, z2_l: f32, z1_r: f32, z2_r: f32 }
impl HpBiquad {
    fn new(freq: f32, q: f32, sr: f32) -> Self {
        let omega = 2.0 * std::f32::consts::PI * freq / sr;
        let alpha = omega.sin() / (2.0 * q);
        let cos_w = omega.cos();
        let b0 = (1.0 + cos_w) / 2.0;
        let b1 = -(1.0 + cos_w);
        let b2 = (1.0 + cos_w) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w;
        let a2 = 1.0 - alpha;
        Self { b0: b0/a0, b1: b1/a0, b2: b2/a0, a1: a1/a0, a2: a2/a0, z1_l:0.0, z2_l:0.0, z1_r:0.0, z2_r:0.0 }
    }
    fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        for i in 0..l.len() {
            let out_l = self.b0 * l[i] + self.z1_l;
            self.z1_l = self.b1 * l[i] - self.a1 * out_l + self.z2_l;
            self.z2_l = self.b2 * l[i] - self.a2 * out_l;
            let out_r = self.b0 * r[i] + self.z1_r;
            self.z1_r = self.b1 * r[i] - self.a1 * out_r + self.z2_r;
            self.z2_r = self.b2 * r[i] - self.a2 * out_r;
            l[i] = out_l;
            r[i] = out_r;
        }
    }
}

fn make_pair(base: &str, name: &str, speech_start_s: f32, corner_b: f32) {
    let path = format!("{base}/{name}.mp3");
    let off_dump = decode_dump(&path, &format!("{name}_ab"));
    let (l0, r0) = read_dump_stereo(&off_dump);

    // 80Hz — ίδιος κόμβος με το cleaner.rs:69 (RestorationChain, lowcut μόνο).
    let mut chain80 = RestorationChain::new(
        SR,
        RestorationConfig { lowcut_enabled: true, hum_enabled: false, deess_enabled: false, gate_enabled: false },
        0.0, f32::NEG_INFINITY,
    );
    let mut l80 = l0.clone();
    let mut r80 = r0.clone();
    chain80.process(&mut l80, &mut r80);

    // Σχέδιο Β — ισοδύναμο φίλτρο, γωνία = θεμ./2.
    let mut hpb = HpBiquad::new(corner_b, 0.707, SR);
    let mut lb = l0.clone();
    let mut rb = r0.clone();
    hpb.process(&mut lb, &mut rb);

    let start_frame = (speech_start_s * SR) as usize;
    let len_frame = (10.0 * SR) as usize;
    let end_frame = (start_frame + len_frame).min(l0.len());

    let out_dir = "/tmp/lowcut-rumble-audition";
    let _ = std::fs::create_dir_all(out_dir);
    let p80 = format!("{out_dir}/{name}_80Hz_speech@{speech_start_s:.1}s.wav");
    let pb = format!("{out_dir}/{name}_planB-{corner_b:.1}Hz_speech@{speech_start_s:.1}s.wav");
    write_wav(&p80, &l80[start_frame..end_frame], &r80[start_frame..end_frame]);
    write_wav(&pb, &lb[start_frame..end_frame], &rb[start_frame..end_frame]);
    println!("{p80}");
    println!("{pb}");

    let _ = std::fs::remove_file(&off_dump);
}

fn main() {
    let base = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    make_pair(base, "secretgarden_01_burnett", 352.0, 123.045);
    make_pair(base, "tale_of_two_cities_01_dickens", 263.4, 55.665);
}
