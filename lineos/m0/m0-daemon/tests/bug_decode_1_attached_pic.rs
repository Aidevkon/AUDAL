use m0d::handlers::decode::decode_raw_interleaved;
use std::path::PathBuf;

#[test]
fn test_decode_attached_picture_bug_decode_1() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("test_attached_pic.mp3");

    let (samples, sr, ch) = decode_raw_interleaved(path.to_str().unwrap()).expect("Decode failed");

    assert_eq!(sr, 48000);
    assert_eq!(ch, 2);

    let frames = samples.len() / ch as usize;
    let expected_frames = 2 * 48000; // 2 seconds
    let diff = (frames as isize - expected_frames as isize).abs();

    assert!(diff < 5000, "Duration mismatch: {} frames", frames);
}
