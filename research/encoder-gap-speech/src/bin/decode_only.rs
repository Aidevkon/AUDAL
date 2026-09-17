//! RECON 2026-09-18 βοηθητικό: decode ενός mp3 σε raw dump μέσω του
//! ΠΡΑΓΜΑΤΙΚΟΥ pass0_decode_to_dump, για να τροφοδοτηθεί ξεχωριστά σε
//! run_trunk_once (μέτρηση κόστους ανά τρέξιμο, /usr/bin/time -v).
//! ΧΡΗΣΗ: cargo run --release --bin decode_only -- <input.mp3> <dump_out.raw>
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = &args[1];
    let dump = &args[2];
    let (_metrics, _dec) = m0d::dsp::input_lufs::pass0_decode_to_dump(
        std::path::Path::new(input),
        dump,
    )
    .unwrap_or_else(|e| panic!("decode {input}: {e}"));
    let n = std::fs::metadata(dump).unwrap().len();
    println!("dump={dump} bytes={n} frames={}", n / 8);
}
