use super::decode::{DecodeError, MAX_DURATION_SECS};
use crate::config::MAX_FILE_BYTES;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub use sp314_dsp::io::decode_types::DecodeChunk;

/// Streaming decode: calls `on_chunk` for each decoded packet's samples
/// instead of accumulating into one Vec<f32>. Preserves the existing
/// MAX_FILE_BYTES (pre-loop, file-size-based) and MAX_DURATION_SECS
/// (now tracked incrementally, since there's no final raw_samples.len()
/// to check against) safety nets from decode_raw_interleaved.
///
/// Returns (original_sample_rate, original_channels) once decoding completes —
/// callers needing duration_ms can derive it from frame count tracked in
/// their own on_chunk callback if needed.
pub fn decode_streaming<E, F>(path: &str, mut on_chunk: F) -> Result<(u32, u16), DecodeError>
where
    E: ToString,
    F: FnMut(DecodeChunk<'_>) -> Result<(), E>,
{
    // ── 1. File size check ────────────────────────────────────────────────────
    let meta = std::fs::metadata(path).map_err(|_| DecodeError::FileNotFound(path.to_string()))?;

    if meta.len() > MAX_FILE_BYTES {
        return Err(DecodeError::FileTooLarge(meta.len()));
    }

    // ── 2. symphonia probe + decode ───────────────────────────────────────────
    let file =
        std::fs::File::open(path).map_err(|_| DecodeError::FileNotFound(path.to_string()))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let metadata_opts = MetadataOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| DecodeError::UnsupportedFormat(e.to_string()))?;

    let mut format = probed.format;

    // Select default audio track
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| DecodeError::UnsupportedFormat("No audio track found".into()))?;

    let track_id = track.id;
    // X1: το sample rate είναι η ΚΛΙΜΑΚΑ ΤΟΥ ΧΡΟΝΟΥ, όχι
    // μεταδεδομένο. Λάθος τιμή = pitch shift + κάθε
    // μέτρηση μετατοπισμένη (44.1k ως 48k = +1.47
    // ημιτόνια, 8.8% σε LUFS φίλτρα, mel filterbank,
    // frame 480). Τέσσερα σημεία μάντευαν με τρεις
    // διαφορετικές τιμές. ΑΡΝΗΣΗ αντί για μαντεψιά.
    let original_sr = track.codec_params.sample_rate.ok_or(DecodeError::MissingSampleRate)?;
    let original_ch = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(2);

    let dec_opts = DecoderOptions::default();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .map_err(|e| DecodeError::UnsupportedFormat(format!("Codec not supported: {e}")))?;

    // SampleBuffer<f32> is the universal sink — symphonia converts any type to f32.
    // We allocate lazily on first decoded packet to know the spec.
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    let mut total_frames_decoded: u64 = 0;
    let max_frames = MAX_DURATION_SECS * original_sr as u64; // sample_rate γνωστό μετά το probe setup

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(DecodeError::DecodeFailure(e.to_string())),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                // Initialise SampleBuffer on first decoded packet
                let sb = sample_buf.get_or_insert_with(|| {
                    SampleBuffer::<f32>::new(audio_buf.capacity() as u64, *audio_buf.spec())
                });
                // Copy all samples (interleaved) into the f32 SampleBuffer
                sb.copy_interleaved_ref(audio_buf);

                let frames_this_packet = sb.samples().len() as u64 / original_ch.max(1) as u64;
                total_frames_decoded += frames_this_packet;
                if total_frames_decoded > max_frames {
                    return Err(DecodeError::DurationExceeded(
                        total_frames_decoded / original_sr as u64,
                    ));
                }

                on_chunk(DecodeChunk::Samples(sb.samples()))
                    .map_err(|e| DecodeError::ConsumerError(e.to_string()))?;
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(DecodeError::DecodeFailure(e.to_string())),
        }
    }

    on_chunk(DecodeChunk::EndOfStream).map_err(|e| DecodeError::ConsumerError(e.to_string()))?;
    Ok((original_sr, original_ch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_streaming_matches_batch_decode_exactly() {
        let path = "../../m1/sp314-dsp/tests/fixtures/sine_1khz_3s.wav";

        // Batch (old way)
        let (batch_samples, batch_sr, batch_ch) =
            super::super::decode::decode_raw_interleaved(path).unwrap();

        // Streaming (new way) — accumulate chunks to compare
        let mut streamed_samples: Vec<f32> = Vec::new();
        let (stream_sr, stream_ch) =
            decode_streaming(path, |chunk| -> Result<(), Box<dyn std::error::Error>> {
                if let DecodeChunk::Samples(s) = chunk {
                    streamed_samples.extend_from_slice(s);
                }
                Ok(())
            })
            .unwrap();

        assert_eq!(batch_sr, stream_sr, "sample rate must match");
        assert_eq!(batch_ch, stream_ch, "channel count must match");
        assert_eq!(
            batch_samples, streamed_samples,
            "streaming decode must produce byte-identical samples to batch decode"
        );
    }
}
