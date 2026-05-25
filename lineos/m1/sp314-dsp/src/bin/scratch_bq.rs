use sp314_dsp::restoration::biquad::{Biquad, FilterType};

fn main() {
    let mut bq = Biquad::new(FilterType::Notch, 50.0, 20.0, 48000.0);
    let mut energy = 0.0;
    for i in 0..96000 {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 50.0 * t).sin();
        let (out, _) = bq.process_stereo(s, s);
        if i >= 48000 {
            energy += out * out;
        }
    }
    println!("Energy: {}", energy);
}
