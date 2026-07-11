use crate::stft::stem_renderer::FiveStemRenderer;

pub struct BusSplit {
    pub vocal: Vec<f32>, // the voice stem, as-is
    pub music: Vec<f32>, // drums+bass+harmonics+ambience, SUMMED
}

pub fn escalate_to_stems(
    full_left: &[f32],
    full_right: &[f32],
    start_sec: f32,
    end_sec: f32,
    sample_rate: u32,
) -> BusSplit {
    let start_idx = (start_sec * sample_rate as f32) as usize;
    let end_idx = (end_sec * sample_rate as f32) as usize;

    let start_idx = start_idx.min(full_left.len());
    let end_idx = end_idx.min(full_left.len());

    if start_idx >= end_idx {
        return BusSplit {
            vocal: Vec::new(),
            music: Vec::new(),
        };
    }

    // derive mono slice
    let mut mono = Vec::with_capacity(end_idx - start_idx);
    for i in start_idx..end_idx {
        mono.push((full_left[i] + full_right[i]) * 0.5);
    }

    let mut renderer = FiveStemRenderer::new();
    // render() handles the 10s sample-fit internally and expects a mono slice.
    let stems = renderer.render(&mono);

    // Sum the non-vocal stems
    let len = stems.voice.len();
    let mut music = Vec::with_capacity(len);
    for i in 0..len {
        music.push(stems.drums[i] + stems.bass[i] + stems.harmonics[i] + stems.ambience[i]);
    }

    BusSplit {
        vocal: stems.voice,
        music,
    }
}

pub fn escalate_flagged_segments(
    boundaries: &[lineos_corpus::scout::SegmentBoundary],
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
) -> Vec<Option<BusSplit>> {
    // We return a parallel array of Option<BusSplit>.
    // If a segment needed escalation, it contains Some(BusSplit) with the separated audio.
    // If it didn't, it contains None.
    let flagged_indices = lineos_corpus::scout::flag_escalation_candidates(boundaries);

    let mut results = Vec::with_capacity(boundaries.len());
    for _ in 0..boundaries.len() {
        results.push(None);
    }

    for &idx in &flagged_indices {
        let b = &boundaries[idx];
        let split = escalate_to_stems(left, right, b.start_sec, b.end_sec, sample_rate);
        results[idx] = Some(split);
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound;
    use std::path::PathBuf;

    fn load_flight_clip_stereo(path: &str) -> (Vec<f32>, Vec<f32>, u32) {
        let reader = hound::WavReader::open(path);
        if reader.is_err() {
            // gracefully skip if running on a fresh clone without fixtures
            return (vec![], vec![], 0);
        }
        let mut reader = reader.unwrap();
        let spec = reader.spec();
        let samples: Vec<i32> = reader.samples().map(|s| s.unwrap()).collect();

        let mut left = Vec::new();
        let mut right = Vec::new();

        if spec.channels == 2 {
            for chunk in samples.chunks_exact(2) {
                left.push(chunk[0] as f32 / 32768.0);
                right.push(chunk[1] as f32 / 32768.0);
            }
        } else {
            for s in samples {
                let m = s as f32 / 32768.0;
                left.push(m);
                right.push(m);
            }
        }

        (left, right, spec.sample_rate)
    }

    fn compute_rms(signal: &[f32]) -> f32 {
        if signal.is_empty() {
            return 0.0;
        }
        let sq_sum: f32 = signal.iter().map(|&x| x * x).sum();
        (sq_sum / signal.len() as f32).sqrt()
    }

    #[test]
    #[ignore = "Requires local uncommitted flight clips in flight_clips_stereo/"]
    fn test_escalate_to_stems_on_blur_zone() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let clip_path = manifest_dir
            .join("../../../flight_clips_stereo/clip_transition_st.wav")
            .to_string_lossy()
            .to_string();

        let (left, right, sample_rate) = load_flight_clip_stereo(&clip_path);
        if left.is_empty() {
            println!("Fixture missing, skipping test safely.");
            return;
        }

        // We target the blur zone around the transition (speech -> IDM, boundary ~17s).
        // Let's escalate a 4-second slice from 13.0s to 17.0s.
        let start_sec = 13.0;
        let end_sec = 17.0;

        let split = escalate_to_stems(&left, &right, start_sec, end_sec, sample_rate);

        assert!(!split.vocal.is_empty(), "Vocal stem should not be empty");
        assert!(!split.music.is_empty(), "Music stem should not be empty");
        assert_eq!(
            split.vocal.len(),
            split.music.len(),
            "Stems must be equal length"
        );

        // Length should match the input slice length exactly (4s * 48000 = 192000)
        let expected_len = (4.0 * sample_rate as f32) as usize;
        assert_eq!(
            split.vocal.len(),
            expected_len,
            "Length mismatch from iSTFT reconstruction"
        );

        // The signals must not be identical (actual separation happened)
        let mut identical = true;
        for i in 0..100 {
            if (split.vocal[i] - split.music[i]).abs() > 1e-5 {
                identical = false;
                break;
            }
        }
        assert!(!identical, "Stems are identical! NMF did nothing.");

        let vocal_rms = compute_rms(&split.vocal);
        let music_rms = compute_rms(&split.music);

        println!("\n=== A7 Stem Escalation Stats ===");
        println!("Target Slice: {:.1}s -> {:.1}s", start_sec, end_sec);
        println!("Vocal RMS: {:.4}", vocal_rms);
        println!("Music RMS: {:.4}", music_rms);
        println!("Stems computed successfully.");
    }

    #[test]
    #[ignore = "Requires local uncommitted flight clips in flight_clips_stereo/"]
    fn test_orchestrate_escalation_on_transition() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let clip_path = manifest_dir
            .join("../../../flight_clips_stereo/clip_transition_st.wav")
            .to_string_lossy()
            .to_string();

        let (left, right, sample_rate) = load_flight_clip_stereo(&clip_path);
        if left.is_empty() {
            println!("Fixture missing, skipping test safely.");
            return;
        }

        // 1. Scan
        let decisions = crate::analysis::scout_scanner::scan_file(&left, &right, sample_rate);

        // 2. Segment
        let boundaries = lineos_corpus::scout::smooth_and_segment(&decisions);

        // 3. Orchestrate Escalation
        let splits = escalate_flagged_segments(&boundaries, &left, &right, sample_rate);

        assert_eq!(boundaries.len(), splits.len());

        let escalated_count = splits.iter().filter(|s| s.is_some()).count();

        println!("\n=== A7 Orchestration Pipeline ===");
        println!("Total Segments: {}", boundaries.len());
        for (i, b) in boundaries.iter().enumerate() {
            println!(
                "[{}] {:?} | {:.1}s -> {:.1}s | Leaning: {:.3} | Conf: {:.3} | Escalated: {}",
                i,
                b.segment_type,
                b.start_sec,
                b.end_sec,
                b.avg_leaning,
                b.avg_confidence,
                splits[i].is_some()
            );
        }
        println!("Escalated Segments: {}", escalated_count);

        // As verified earlier, the real transition clip has a confidence around 0.526,
        // which is above our 0.4 threshold. Therefore we expect 0 escalations on this clean clip.
        assert_eq!(escalated_count, 0, "Clean transition should not escalate");
    }
}
