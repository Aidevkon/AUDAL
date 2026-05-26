const DIGITAL_PEAK: f32 = 0.9;


#[test]
fn true_peak_exceeds_digital_peak() {
    use sp314_dsp::limiter::true_peak::TruePeakDetector;

    let mut detector = TruePeakDetector::new();
    let mut max_true_peak = 0.0_f32;
    let mut max_digital_peak = 0.0_f32;

    // A sequence like [0.9, 0.9, -0.9, -0.9] guarantees a massive
    // intersample peak because the true analog peak happens between
    // the two consecutive 0.9 samples.
    let sequence = [0.9_f32, 0.9_f32, -0.9_f32, -0.9_f32];

    for i in 0..100 {
        let sample = sequence[i % 4];
        let tp = detector.process(sample, sample);
        let dp = sample.abs();
        if tp > max_true_peak  { max_true_peak  = tp; }
        if dp > max_digital_peak { max_digital_peak = dp; }
    }

    println!("Digital peak:  {:.6}", max_digital_peak);
    println!("True peak:     {:.6}", max_true_peak);
    println!("Difference:    {:.6}", max_true_peak - max_digital_peak);

    // Digital peak must be exactly 0.9
    assert!((max_digital_peak - DIGITAL_PEAK).abs() < 1e-6,
        "Digital peak should be 0.9, got {}", max_digital_peak);

    // True peak must be meaningfully higher than digital peak
    // It should jump to ~1.27 for this sequence!
    assert!(max_true_peak > max_digital_peak + 0.1,
        "True peak {} should significantly exceed digital peak {}",
        max_true_peak, max_digital_peak);
}

#[test]
fn true_peak_stereo_independent() {
    use sp314_dsp::limiter::true_peak::TruePeakDetector;

    let mut detector = TruePeakDetector::new();
    let sequence_l = [0.9_f32, 0.9_f32, -0.9_f32, -0.9_f32];
    let sequence_r = [-0.9_f32, -0.9_f32, 0.9_f32, 0.9_f32]; // Inverted phase

    let mut max_tp = 0.0_f32;
    for i in 0..100 {
        let tp = detector.process(sequence_l[i % 4], sequence_r[i % 4]);
        if tp > max_tp { max_tp = tp; }
    }

    println!("Stereo true peak: {:.6}", max_tp);
    // Should catch the ISP from both/either channels
    assert!(max_tp > 0.9_f32 + 0.1,
        "Stereo true peak {} must significantly exceed 0.9", max_tp);
}

#[test]
fn true_peak_silence_is_zero() {
    use sp314_dsp::limiter::true_peak::TruePeakDetector;

    let mut detector = TruePeakDetector::new();
    for _ in 0..100 {
        let tp = detector.process(0.0_f32, 0.0_f32);
        assert!(tp < 1e-6,
            "True peak on silence should be ~0, got {}", tp);
    }
}
