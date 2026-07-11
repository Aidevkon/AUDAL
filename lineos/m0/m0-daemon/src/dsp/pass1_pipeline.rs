use crate::handlers::decode::decode_raw_interleaved;
use lineos_corpus::scout::SegmentBoundary;
use std::error::Error;

pub fn build_timeline_map(path: &str) -> Result<Vec<SegmentBoundary>, Box<dyn Error>> {
    let (interleaved, sample_rate, channels) = decode_raw_interleaved(path)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_build_timeline_map() {
        let path = "../../../flight_clips_stereo/clip_transition_st.wav";
        let map = build_timeline_map(path).unwrap();

        println!("=== Pass 1: Timeline Map ===");
        for (i, b) in map.iter().enumerate() {
            println!(
                "[Segment {}] {:.2}s -> {:.2}s : {:?}",
                i, b.start_sec, b.end_sec, b.segment_type
            );
        }

        assert_eq!(
            map.len(),
            2,
            "Expected exactly 2 segments for the transition clip"
        );

        // Assert boundary is near the known ~15-18s region
        let transition_time = map[0].end_sec;
        assert!(
            transition_time >= 15.0 && transition_time < 18.0,
            "Expected transition boundary between 15s and 18s, got {:.2}s",
            transition_time
        );
    }
}
