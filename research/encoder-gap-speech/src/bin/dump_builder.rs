//! Βοηθητικό: μετατρέπει ένα αρχείο ήχου σε raw PCM dump μέσω της
//! ΠΡΑΓΜΑΤΙΚΗΣ m0d::dsp::input_lufs::pass0_decode_to_dump (P0-a) — όχι
//! αναπαραγωγή decode. Χρησιμοποιείται για τιμολόγηση/σύγκριση ορίων
//! σε αυτό το task, ΔΕΝ αγγίζει παραγωγή.
//! ΧΡΗΣΗ: cargo run --release --bin dump_builder -- <input> <dump_out>
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = &args[1];
    let out = &args[2];
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(input), out)
        .unwrap_or_else(|e| panic!("decode {input}: {e}"));
    println!("wrote {out}");
}
