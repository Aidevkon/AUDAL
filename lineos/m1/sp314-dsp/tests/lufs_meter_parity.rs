use sp314_dsp::metering::lufs::measure_integrated_lufs;
use sp314_dsp::metering::LufsMeter;

fn sine_stereo(sr: u32, dur_secs: f32, freq: f32, amp: f32) -> (Vec<f32>, Vec<f32>) {
    let n = (sr as f32 * dur_secs) as usize;
    let mut l = Vec::with_capacity(n);
    let mut r = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let s = (2.0 * std::f32::consts::PI * freq * t).sin() * amp;
        l.push(s);
        r.push(s);
    }
    (l, r)
}

/// Core parity test: streaming LufsMeter
/// must produce the same result as the
/// monolithic measure_integrated_lufs
/// for the same signal, regardless of
/// chunk size.
fn check_parity(left: &[f32], right: &[f32], chunk_size: usize, label: &str) {
    // Monolithic reference
    let reference = measure_integrated_lufs(left, right);

    // Streaming path
    let mut meter = LufsMeter::new();
    for (cl, cr) in left.chunks(chunk_size).zip(right.chunks(chunk_size)) {
        meter.process_chunk(cl, cr);
    }
    let streaming = meter
        .finish()
        .expect("signal too short for any gating block");

    let diff = (streaming - reference).abs();
    println!(
        "{}: reference={:.3} streaming={:.3} diff={:.4}",
        label, reference, streaming, diff
    );

    assert_eq!(
        streaming,
        reference,
        "{}: streaming LufsMeter diverges from monolithic reference. streaming_bits={:08x}, reference_bits={:08x}, diff={:.8}",
        label,
        streaming.to_bits(),
        reference.to_bits(),
        diff
    );
}

#[test]
fn parity_chunk_512() {
    let (l, r) = sine_stereo(48000, 10.0, 440.0, 0.5);
    check_parity(&l, &r, 512, "chunk=512");
}

#[test]
fn parity_chunk_4096() {
    let (l, r) = sine_stereo(48000, 10.0, 440.0, 0.5);
    check_parity(&l, &r, 4096, "chunk=4096");
}

#[test]
fn parity_chunk_1() {
    // Single-sample chunks — maximum
    // stress on the hop accumulation
    // logic. If sample_count % HOP_SAMPLES
    // boundaries are handled correctly,
    // this will match the monolithic path.
    let (l, r) = sine_stereo(48000, 5.0, 440.0, 0.5);
    check_parity(&l, &r, 1, "chunk=1");
}

#[test]
fn parity_misaligned_chunk() {
    // Chunk size that doesn't divide
    // evenly into HOP_SAMPLES (4800).
    // Stresses boundary accumulation.
    let (l, r) = sine_stereo(48000, 10.0, 1000.0, 0.3);
    check_parity(&l, &r, 666, "chunk=666 (misaligned)");
}

#[test]
fn parity_low_amplitude() {
    // Signal near the absolute gating
    // threshold (-70 LUFS). Some blocks
    // may fall below the gate — both paths
    // must agree on which blocks survive.
    let (l, r) = sine_stereo(48000, 10.0, 440.0, 0.001);
    // This may return None (all blocks
    // gated out) from the streaming path.
    // Monolithic may return a very low
    // number. Both outcomes are valid as
    // long as they agree.
    let reference = measure_integrated_lufs(&l, &r);

    let mut meter = LufsMeter::new();
    for (cl, cr) in l.chunks(512).zip(r.chunks(512)) {
        meter.process_chunk(cl, cr);
    }
    let streaming = meter.finish().unwrap_or(-144.0);

    println!(
        "low_amplitude: reference={:.3} streaming={:.3}",
        reference, streaming
    );

    let diff = (streaming - reference).abs();
    assert_eq!(
        streaming,
        reference,
        "low amplitude parity failed. streaming_bits={:08x}, reference_bits={:08x}, diff={:.8}",
        streaming.to_bits(),
        reference.to_bits(),
        diff
    );
}
