use sp314_dsp::metering::lufs::measure_integrated_lufs;
use sp314_dsp::limiter::TruePeakDetector;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    } else {
        panic!("not float");
    };
    
    let mut left = Vec::with_capacity(samples.len() / 2);
    let mut right = Vec::with_capacity(samples.len() / 2);
    for chunk in samples.chunks_exact(2) {
        left.push(chunk[0]);
        right.push(chunk[1]);
    }
    
    let lufs = measure_integrated_lufs(&left, &right);
    let mut tp_detector = TruePeakDetector::new();
    let mut tp = 0.0_f32;
    for (&l, &r) in left.iter().zip(right.iter()) {
        let max_tp = tp_detector.process(l, r);
        if max_tp > tp { tp = max_tp; }
    }
    for _ in 0..18 {
        let max_tp = tp_detector.process(0.0, 0.0);
        if max_tp > tp { tp = max_tp; }
    }
    let tp_db = 20.0 * tp.log10();
    
    println!("file: {} -> lufs: {:.4}, tp: {:.2}", path, lufs, tp_db);
}
