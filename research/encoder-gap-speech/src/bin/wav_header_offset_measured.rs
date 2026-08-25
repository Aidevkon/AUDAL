//! F-083 blocking question, step 1: does the REAL StreamingWavWriter (the
//! same writer executor.rs uses for the live streaming path's output_path)
//! produce a file whose header survives unstripped when read back by
//! export.rs's pcm_bytes_to_f32 (chunks_exact(4).from_le_bytes, zero header
//! skip)? ΜΕΤΡΗΜΕΝΟ, not theory: write with the real production writer,
//! read the real bytes, count.
//!
//! pcm_bytes_to_f32 itself is a private fn in m0d::handlers::export (3-line
//! chunks_exact(4)+from_le_bytes) — not reachable from this external crate.
//! Its logic is reproduced verbatim below (ΔΙΑΒΑΣΤΗΚΕ: export.rs:992-997)
//! rather than re-implemented differently, so this measures the same
//! behavior the production code would apply to this same file.

use sp314_dsp::io::wav_writer::StreamingWavWriter;

fn pcm_bytes_to_f32_verbatim(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn main() {
    let path = "/tmp/f083_header_probe.wav";
    let sample_rate = 48_000u32;

    // Known synthetic stereo signal: distinct, recognizable per-frame values
    // so a header-induced sample shift is visible in the recovered channel
    // alignment, not just in byte count.
    let n_frames = 10usize;
    let left: Vec<f32> = (0..n_frames).map(|i| 100.0_f32 + i as f32).collect();
    let right: Vec<f32> = (0..n_frames).map(|i| -100.0_f32 - i as f32).collect();
    {
        let mut w = StreamingWavWriter::new(path, sample_rate)
            .expect("StreamingWavWriter::new failed");
        w.write_chunk(&left, &right).expect("write_chunk failed");
        w.finalize().expect("finalize failed");
    }

    let raw_bytes = std::fs::read(path).expect("read back failed");
    println!("MEASURED total file bytes: {}", raw_bytes.len());

    // Locate the real "data" chunk to know the true header size (ground
    // truth, read from the file's own RIFF structure — not assumed).
    let data_marker = raw_bytes
        .windows(4)
        .position(|w| w == b"data")
        .expect("no data chunk marker found");
    // "data" tag (4) + chunk size (4) = 8 bytes, then audio starts.
    let true_header_len = data_marker + 8;
    println!(
        "MEASURED true header length (RIFF-parsed, up to start of audio bytes): {}",
        true_header_len
    );
    println!(
        "MEASURED true_header_len % 4 == {} (byte alignment vs f32 samples)",
        true_header_len % 4
    );
    if true_header_len % 4 == 0 {
        println!(
            "MEASURED true_header_len / 4 == {} f32 samples of header-as-audio garbage",
            true_header_len / 4
        );
        println!(
            "MEASURED (true_header_len/4) % 2 == {} (0 = stays L/R aligned, 1 = channel-swaps everything after)",
            (true_header_len / 4) % 2
        );
    }

    // Now do EXACTLY what export.rs's pcm_bytes_to_f32 does: zero header
    // skip, blind chunks_exact(4) over the whole file.
    let naive_pcm = pcm_bytes_to_f32_verbatim(&raw_bytes);
    println!(
        "MEASURED pcm_bytes_to_f32 (no header skip) recovered {} f32 samples ({} frames if stereo)",
        naive_pcm.len(),
        naive_pcm.len() / 2
    );
    println!("MEASURED expected real frames written: {}", n_frames);

    // Print first 24 recovered samples (header garbage + first real frames)
    // so the actual numeric consequence is visible, not inferred.
    println!("MEASURED first 24 recovered f32 values (garbage-then-real boundary):");
    for (i, v) in naive_pcm.iter().take(24).enumerate() {
        println!("  [{i}] = {v}");
    }

    // Ground truth: what does correct parsing (skip true_header_len) give?
    let correct_pcm = pcm_bytes_to_f32_verbatim(&raw_bytes[true_header_len..]);
    println!(
        "MEASURED correctly-parsed (header skipped) sample count: {} ({} frames)",
        correct_pcm.len(),
        correct_pcm.len() / 2
    );
    println!("MEASURED correctly-parsed first 8 values (should be 100,-100,101,-101,...):");
    for (i, v) in correct_pcm.iter().take(8).enumerate() {
        println!("  [{i}] = {v}");
    }
}
