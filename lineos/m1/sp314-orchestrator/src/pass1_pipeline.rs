use crate::decode_provider::WholeBufferProvider;
use lineos_corpus::scout::SegmentBoundary;
use std::error::Error;

/// Deprecated: production path now uses `trunk_pass::run_trunk_pass` which
/// reads the raw PCM dump instead of decoding the original file into RAM.
/// Kept alive for test compatibility (pass1_integration, streaming_integration).
pub fn build_timeline_map(
    decoder: impl WholeBufferProvider,
) -> Result<Vec<SegmentBoundary>, Box<dyn Error>> {
    let (interleaved, sample_rate, channels) = decoder
        .decode_to_memory()
        .map_err(|e| -> Box<dyn Error> { format!("{:?}", e).into() })?;

    // De-interleave
    let mut left = Vec::with_capacity(interleaved.len() / channels as usize);
    let mut right = Vec::with_capacity(interleaved.len() / channels as usize);

    if channels == 1 {
        left.extend_from_slice(&interleaved);
        right.extend_from_slice(&interleaved);
    } else if channels >= 2 {
        for frame in interleaved.chunks_exact(channels as usize) {
            left.push(frame[0]);
            right.push(frame[1]);
        }
    } else {
        return Err(format!("Unsupported channel count: {}", channels).into());
    }

    // Call SegmentScout
    let decisions = sp314_dsp::analysis::scout_scanner::scan_file(&left, &right, sample_rate);
    let boundaries = lineos_corpus::scout::smooth_and_segment(&decisions);

    Ok(boundaries)
}
