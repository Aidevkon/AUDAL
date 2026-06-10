// src/io/wav_reader.rs

/// Contains decoded audio data (stereo, f32) and metadata.
pub struct DecodedAudio {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub sample_rate: u32,
    pub num_channels: u16,
    pub duration_secs: f32,
}

/// Simple WAV reader using `hound`.
pub struct WavReader;

impl WavReader {
    /// Reads a WAV file into a `DecodedAudio` struct.
    /// Converts 16/24-bit int and 32-bit float to f32.
    pub fn read(path: &str) -> Result<DecodedAudio, Box<dyn std::error::Error>> {
        let mut reader = hound::WavReader::open(path)?;
        let spec = reader.spec();

        let samples_raw: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
            hound::SampleFormat::Int => {
                let max_val = match spec.bits_per_sample {
                    16 => 32768.0,
                    24 => 8388608.0,
                    _ => {
                        return Err(
                            format!("Unsupported bit depth: {}", spec.bits_per_sample).into()
                        )
                    }
                };
                reader
                    .samples::<i32>()
                    .map(|s| s.unwrap() as f32 / max_val)
                    .collect()
            }
        };

        let (left, right) = if spec.channels == 2 {
            let l: Vec<f32> = samples_raw.iter().step_by(2).cloned().collect();
            let r: Vec<f32> = samples_raw.iter().skip(1).step_by(2).cloned().collect();
            (l, r)
        } else if spec.channels == 1 {
            (samples_raw.clone(), samples_raw)
        } else {
            return Err(format!("Unsupported channel count: {}", spec.channels).into());
        };

        let duration_secs = left.len() as f32 / spec.sample_rate as f32;

        Ok(DecodedAudio {
            left,
            right,
            sample_rate: spec.sample_rate,
            num_channels: spec.channels,
            duration_secs,
        })
    }
}
