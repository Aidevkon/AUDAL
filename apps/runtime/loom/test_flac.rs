fn main() {
    let samples = vec![0.0_f32; 48000];
    sp314_dsp::io::flac_encode::flac_encode(&samples, 48000, 1).unwrap();
}
