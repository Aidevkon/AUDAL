// src/io/wav_writer.rs

/// Simple WAV writer using `hound`.
pub struct WavWriter;

impl WavWriter {
    /// Writes left and right channels to a 32-bit float stereo WAV file.
    pub fn write(
        path: &str,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if left.len() != right.len() {
            return Err("Left and right channels must have the same length".into());
        }

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = hound::WavWriter::create(path, spec)?;

        for i in 0..left.len() {
            writer.write_sample(left[i])?;
            writer.write_sample(right[i])?;
        }

        writer.finalize()?;
        Ok(())
    }
}
