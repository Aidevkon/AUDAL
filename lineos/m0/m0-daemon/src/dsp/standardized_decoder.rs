use crate::dsp::audio_source::AudioSource;
use crate::dsp::standardized_stream::StandardizedAudioStream;
use sp314_dsp::io::decode_types::{DecodeChunk, DecodeError};
use sp314_orchestrator::decode_provider::DecodeProvider;
use std::cell::RefCell;

const CHUNK_FRAMES: usize = 4096;

/// DecodeProvider adapter over StandardizedAudioStream: every chunk
/// the pipeline sees is 48 kHz, stereo, sanitized (NaN→0, clamped)
/// — the same standardization contract the v2 batch path and the
/// Episode path guarantee. Fixes the v3 gap where files entered the
/// DSP graph at their native rate, unsanitized (untested until now
/// because every fixture happened to be 48k).
///
/// Also carries the input-identity hashes for certification: the
/// stream hashes post-resample post-sanitize interleaved samples
/// internally (blake3 LE / sha256 BE — Episode parity by
/// construction, it's the same code), exposed via input_hashes()
/// after the stream has been drained.
pub struct StandardizedDecoder {
    stream: RefCell<StandardizedAudioStream>,
}

impl StandardizedDecoder {
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        Ok(Self {
            stream: RefCell::new(StandardizedAudioStream::open(path)?),
        })
    }

    /// (blake3_hex, sha256_hex) over the standardized input.
    /// Meaningful only after stream_to has fully drained the file.
    pub fn input_hashes(&self) -> (String, String) {
        self.stream.borrow().input_hashes()
    }

    /// Consume the adapter and return the dead-air summary.
    pub fn into_dead_air(self) -> crate::dsp::signal_health::DeadAirSummary {
        self.stream.into_inner().into_dead_air()
    }
}

impl DecodeProvider for StandardizedDecoder {
    fn stream_to<E, F>(&self, mut on_chunk: F) -> Result<(u32, u16), DecodeError>
    where
        E: ToString,
        F: FnMut(DecodeChunk<'_>) -> Result<(), E>,
    {
        let mut stream = self.stream.borrow_mut();
        let sample_rate = stream.sample_rate();
        let mut buf = vec![0f32; CHUNK_FRAMES * 2];
        loop {
            let frames = stream
                .fill_buffer(&mut buf)
                .map_err(DecodeError::DecodeFailure)?;
            if frames == 0 {
                break;
            }
            on_chunk(DecodeChunk::Samples(&buf[..frames * 2]))
                .map_err(|e| DecodeError::ConsumerError(e.to_string()))?;
        }
        on_chunk(DecodeChunk::EndOfStream)
            .map_err(|e| DecodeError::ConsumerError(e.to_string()))?;
        Ok((sample_rate, 2))
    }
}
