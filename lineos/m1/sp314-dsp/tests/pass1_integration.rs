use hound;
use lineos_corpus::scout::{smooth_and_segment, SegmentType};
use sp314_dsp::analysis::scout_scanner::scan_file;
use std::path::PathBuf;

fn load_flight_clip_stereo(path: &str) -> (Vec<f32>, Vec<f32>, u32) {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|_| panic!("failed to open clip at {}", path));
    let spec = reader.spec();
    let samples: Vec<i32> = reader.samples().map(|s| s.unwrap()).collect();

    let mut left = Vec::new();
    let mut right = Vec::new();

    if spec.channels == 2 {
        for chunk in samples.chunks_exact(2) {
            let l = chunk[0] as f32 / 32768.0;
            let r = chunk[1] as f32 / 32768.0;
            left.push(l);
            right.push(r);
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

fn test_transition(
    clip_name: &str,
    expected_type_1: SegmentType,
    expected_type_2: SegmentType,
    expected_boundary_sec: f32,
) {
    println!("\n=== {} ===", clip_name);
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let clip_path = manifest_dir
        .join(format!("../../../flight_clips_stereo/{}", clip_name))
        .to_string_lossy()
        .to_string();

    let (left, right, sample_rate) = load_flight_clip_stereo(&clip_path);
    
    // Pass 1: Scan
    let decisions = scan_file(&left, &right, sample_rate);
    
    // Pass 1: Segment
    let boundaries = smooth_and_segment(&decisions);

    println!("Timeline Map:");
    for (i, b) in boundaries.iter().enumerate() {
        println!("  [{}] {:?} | {:.1}s -> {:.1}s | Avg Leaning: {:.3} | Avg Conf: {:.3}", 
                 i, b.segment_type, b.start_sec, b.end_sec, b.avg_leaning, b.avg_confidence);
    }

    // Assert exactly 2 segments (or close). If not 2, we fail loudly to investigate.
    assert_eq!(boundaries.len(), 2, "Expected exactly 2 segments for {}", clip_name);

    // Segment 1 sanity
    let s1 = &boundaries[0];
    assert_eq!(s1.segment_type, expected_type_1);
    assert_eq!(s1.start_sec, 0.0);
    assert!(s1.avg_confidence > 0.5, "Segment 1 confidence too low: {}", s1.avg_confidence);

    // Segment 2 sanity
    let s2 = &boundaries[1];
    assert_eq!(s2.segment_type, expected_type_2);
    assert!(s2.avg_confidence > 0.5, "Segment 2 confidence too low: {}", s2.avg_confidence);

    // Boundary lands near expected time (± 3 seconds to account for 5s window and acoustic blur)
    let boundary_time = s1.end_sec; // Or s2.start_sec
    assert!(
        (boundary_time - expected_boundary_sec).abs() < 3.5, 
        "Boundary {:.1}s is too far from expected {:.1}s", boundary_time, expected_boundary_sec
    );
}

#[test]
#[ignore = "Requires local uncommitted flight clips in flight_clips_stereo/"]
fn test_pass1_end_to_end() {
    // 1. Speech -> IDM
    test_transition(
        "clip_transition_st.wav",
        SegmentType::Speech,
        SegmentType::Music,
        17.0,
    );

    // 2. IDM -> Speech
    test_transition(
        "clip_transition_reverse_st.wav",
        SegmentType::Music,
        SegmentType::Speech,
        18.0,
    );

    // 3. Speech -> Acoustic
    test_transition(
        "clip_transition_sa_st.wav",
        SegmentType::Speech,
        SegmentType::Music,
        17.0,
    );

    // 4. Acoustic -> Speech
    test_transition(
        "clip_transition_reverse_sa_st.wav",
        SegmentType::Music,
        SegmentType::Speech,
        18.0,
    );
    
    println!("\nAll 4 integration clips passed. Pass 1 Pipeline is solid.");
}
