use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug)]
pub enum StemError {
    DecodeError(String),
    UnsupportedFormat(String),
    EmptyBuffer,
}

pub struct StemBuffer {
    pub id: String,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub sample_rate: u32,
    pub num_frames: usize,
}

impl StemBuffer {
    pub fn from_flac_bytes(id: &str, bytes: &[u8]) -> Result<Self, StemError> {
        // Create a MediaSourceStream from the in-memory bytes
        let mss = MediaSourceStream::new(
            Box::new(std::io::Cursor::new(bytes.to_vec())),
            Default::default(),
        );

        let mut hint = Hint::new();
        hint.with_extension("flac");

        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &fmt_opts, &meta_opts)
            .map_err(|e| StemError::DecodeError(e.to_string()))?;

        let mut format = probed.format;

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| StemError::UnsupportedFormat("No audio track found".into()))?;

        let track_id = track.id;
        let sample_rate = track.codec_params.sample_rate.unwrap_or(48000);
        let channels = track
            .codec_params
            .channels
            .unwrap_or(
                symphonia::core::audio::Channels::FRONT_LEFT
                    | symphonia::core::audio::Channels::FRONT_RIGHT,
            )
            .count();

        let dec_opts: DecoderOptions = Default::default();
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &dec_opts)
            .map_err(|e| StemError::DecodeError(e.to_string()))?;

        let mut left = Vec::new();
        let mut right = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(SymphoniaError::IoError(err)) => {
                    // EOF or standard IO error
                    if err.kind() == std::io::ErrorKind::UnexpectedEof {
                        break;
                    }
                    return Err(StemError::DecodeError(err.to_string()));
                }
                Err(err) => {
                    return Err(StemError::DecodeError(err.to_string()));
                }
            };

            if packet.track_id() != track_id {
                continue;
            }

            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let mut sample_buf =
                        SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                    sample_buf.copy_interleaved_ref(decoded);
                    let samples = sample_buf.samples();

                    if channels == 1 {
                        for &sample in samples {
                            left.push(sample);
                            right.push(sample);
                        }
                    } else if channels >= 2 {
                        for chunk in samples.chunks_exact(channels) {
                            left.push(chunk[0]);
                            right.push(chunk[1]);
                        }
                    }
                }
                Err(SymphoniaError::IoError(_)) | Err(SymphoniaError::DecodeError(_)) => {
                    // Could log error but we continue Decoding
                }
                Err(e) => return Err(StemError::DecodeError(e.to_string())),
            }
        }

        let num_frames = left.len();
        if num_frames == 0 {
            return Err(StemError::EmptyBuffer);
        }

        Ok(Self {
            id: id.to_string(),
            left,
            right,
            sample_rate,
            num_frames,
        })
    }

    pub fn read_block(&self, frame_offset: usize, left: &mut [f32], right: &mut [f32]) {
        let block_size = left.len();
        if frame_offset >= self.num_frames {
            left.fill(0.0);
            right.fill(0.0);
            return;
        }

        let frames_to_read = std::cmp::min(block_size, self.num_frames - frame_offset);

        left[..frames_to_read]
            .copy_from_slice(&self.left[frame_offset..frame_offset + frames_to_read]);
        right[..frames_to_read]
            .copy_from_slice(&self.right[frame_offset..frame_offset + frames_to_read]);

        if frames_to_read < block_size {
            left[frames_to_read..].fill(0.0);
            right[frames_to_read..].fill(0.0);
        }
    }
}
