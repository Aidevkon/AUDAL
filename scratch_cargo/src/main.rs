use sp314_dsp::metering::lufs::measure_integrated_lufs;
use std::f32::consts::PI;

fn main() {
    let sample_rate = 48000;
    let duration_sec = 5;
    let total_samples = sample_rate * duration_sec;
    let mut left = vec![0.0f32; total_samples];
    let mut right = vec![0.0f32; total_samples];

    // -23 LUFS = 1 kHz sine wave at peak 10^(-23/20)
    let peak = 10.0f32.powf(-23.0 / 20.0);
    
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = peak * (2.0 * PI * 1000.0 * t).sin();
        left[i] = sample;
        right[i] = sample;
    }

    let lufs = measure_integrated_lufs(&left, &right);
    println!("Measured LUFS: {}", lufs);
}
