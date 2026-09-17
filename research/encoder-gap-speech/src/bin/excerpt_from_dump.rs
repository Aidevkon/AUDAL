//! Βοηθητικό: κόβει ένα απόσπασμα [start_sec, end_sec) από ένα raw PCM
//! dump (f32 LE interleaved stereo, 48kHz — η ΙΔΙΑ μορφή production)
//! και το γράφει ως wav (f32, 48kHz, stereo) — για full-chain render
//! ενός μικρού αποσπάσματος αντί ολόκληρου του βιβλίου. ΔΕΝ αγγίζει
//! παραγωγή, ΔΕΝ ξαναγράφει decode λογική — μόνο slice+write.
//! ΧΡΗΣΗ: cargo run --release --bin excerpt_from_dump -- <dump> <start_sec> <end_sec> <out.wav>
const DUMP_FRAME_BYTES: usize = 8;
const SR: u32 = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dump_path = &args[1];
    let start_sec: f32 = args[2].parse().unwrap();
    let end_sec: f32 = args[3].parse().unwrap();
    let out_path = &args[4];

    let buf = std::fs::read(dump_path).unwrap_or_else(|e| panic!("read {dump_path}: {e}"));
    let full_frames = buf.len() / DUMP_FRAME_BYTES;
    let start_frame = (start_sec * SR as f32) as usize;
    let end_frame = ((end_sec * SR as f32) as usize).min(full_frames);

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(out_path, spec).unwrap_or_else(|e| panic!("create {out_path}: {e}"));
    for i in start_frame..end_frame {
        let base = i * DUMP_FRAME_BYTES;
        let l = f32::from_le_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]);
        let r = f32::from_le_bytes([buf[base + 4], buf[base + 5], buf[base + 6], buf[base + 7]]);
        writer.write_sample(l).unwrap();
        writer.write_sample(r).unwrap();
    }
    writer.finalize().unwrap();
    println!("wrote {out_path}: [{start_sec}s, {end_sec}s) = {} frames", end_frame - start_frame);
}
