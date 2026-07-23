use sp314_dsp::io::decode_types::{DecodeChunk, DecodeError};

pub trait DecodeProvider {
    fn stream_to<E, F>(&self, on_chunk: F) -> Result<(u32, u16), DecodeError>
    where
        E: ToString,
        F: FnMut(DecodeChunk<'_>) -> Result<(), E>;
}

pub trait WholeBufferProvider {
    fn decode_to_memory(&self) -> Result<(Vec<f32>, u32, u16), DecodeError>;
}

use std::cell::RefCell;
use std::io::Write;

/// Decorator over any DecodeProvider that tees every raw interleaved
/// chunk to a file on disk, byte-for-byte, before the pipeline sees
/// it. The dump matches xaak's PcmTransfer layout (interleaved f32,
/// native-endian) because that is exactly what DecodeChunk::Samples
/// already carries — no transformation happens here.
///
/// Tap failures are deliberately BEST-EFFORT: an I/O error stops
/// further tap writes and is stored for the caller to inspect via
/// take_tap_error(), but never interrupts the audio stream — the
/// mastered output must not fail because the A/B monitoring dump
/// could not be written. (The closure's generic error type E also
/// cannot be constructed by this decorator, so an in-band error
/// would masquerade as a ConsumerError — worse than best-effort.)
pub struct TappedDecoder<D: DecodeProvider> {
    inner: D,
    tap_path: std::path::PathBuf,
    tap_error: RefCell<Option<std::io::Error>>,
}

impl<D: DecodeProvider> TappedDecoder<D> {
    pub fn new(inner: D, tap_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            inner,
            tap_path: tap_path.into(),
            tap_error: RefCell::new(None),
        }
    }

    /// Access the wrapped decoder (e.g. to read post-run state
    /// like StandardizedDecoder::input_hashes after the stream
    /// has drained).
    pub fn inner(&self) -> &D {
        &self.inner
    }

    /// Consume the wrapper and return the inner decoder by value —
    /// needed for operations like StandardizedDecoder::into_dead_air
    /// that require ownership, not just a reference. Call this LAST,
    /// after any &self-based operations (input_hashes(),
    /// take_tap_error()) have already run.
    pub fn into_inner(self) -> D {
        self.inner
    }

    /// Returns and clears the tap error, if any occurred.
    pub fn take_tap_error(&self) -> Option<std::io::Error> {
        self.tap_error.borrow_mut().take()
    }
}

impl<D: DecodeProvider> DecodeProvider for TappedDecoder<D> {
    fn stream_to<E, F>(&self, mut on_chunk: F) -> Result<(u32, u16), DecodeError>
    where
        E: ToString,
        F: FnMut(DecodeChunk<'_>) -> Result<(), E>,
    {
        let mut writer = match std::fs::File::create(&self.tap_path) {
            Ok(f) => Some(std::io::BufWriter::new(f)),
            Err(e) => {
                *self.tap_error.borrow_mut() = Some(e);
                None
            }
        };

        let result = self.inner.stream_to(|chunk| {
            if let DecodeChunk::Samples(interleaved) = &chunk {
                if let Some(w) = writer.as_mut() {
                    // SAFETY: reinterpreting &[f32] as raw bytes for an
                    // in-process, same-architecture dump — the exact
                    // layout xaak's reader mmaps back (matches the v2
                    // decode_node.rs dump, verified there byte-for-byte).
                    let bytes: &[u8] = unsafe {
                        std::slice::from_raw_parts(
                            interleaved.as_ptr() as *const u8,
                            std::mem::size_of_val(*interleaved),
                        )
                    };
                    if let Err(e) = w.write_all(bytes) {
                        *self.tap_error.borrow_mut() = Some(e);
                        writer = None; // stop tapping, keep streaming
                    }
                }
            }
            on_chunk(chunk)
        });

        if let Some(w) = writer.as_mut() {
            if let Err(e) = w.flush() {
                *self.tap_error.borrow_mut() = Some(e);
            }
        }
        result
    }
}

/// Blanket impl so callers can pass &decoder and retain ownership —
/// needed by decorators like TappedDecoder whose post-run state
/// (take_tap_error) must be inspected after the pipeline returns.
impl<T: DecodeProvider> DecodeProvider for &T {
    fn stream_to<E, F>(&self, on_chunk: F) -> Result<(u32, u16), DecodeError>
    where
        E: ToString,
        F: FnMut(DecodeChunk<'_>) -> Result<(), E>,
    {
        (**self).stream_to(on_chunk)
    }
}

/// DecodeProvider over a raw PCM dump file (interleaved f32 LE,
/// 48 kHz, stereo — the format pass0_decode_to_dump writes via
/// TappedDecoder through StandardizedDecoder, which guarantees
/// 48k/2ch by construction [standardized_decoder.rs:70]).
///
/// Reads the dump back with EXPLICIT from_le_bytes conversion —
/// no pointer casts — so behavior is correct on both LE and BE
/// platforms (unlike RawPcmFileSource's native-endian cast, which
/// is paired with a same-endian writer and therefore safe in
/// practice but relies on architecture invariants we choose not
/// to propagate).
///
/// This replaces the second StandardizedDecoder::open that
/// previously re-decoded the original audio file for the render
/// pass. The dump is the artery: if it is missing, truncated, or
/// misaligned, that is a hard error — not a recoverable fallback.
pub struct DumpDecodeProvider {
    path: std::path::PathBuf,
}

impl DumpDecodeProvider {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

const DUMP_CHUNK_FRAMES: usize = 4096;

impl DecodeProvider for DumpDecodeProvider {
    fn stream_to<E, F>(&self, mut on_chunk: F) -> Result<(u32, u16), DecodeError>
    where
        E: ToString,
        F: FnMut(DecodeChunk<'_>) -> Result<(), E>,
    {
        use std::io::Read;

        let file = std::fs::File::open(&self.path).map_err(|e| {
            DecodeError::FileNotFound(format!(
                "P0 dump artery missing ({}): {e}",
                self.path.display()
            ))
        })?;
        let mut reader = std::io::BufReader::new(file);

        // Read buffer: 4 bytes per f32 × 2 channels × DUMP_CHUNK_FRAMES
        let byte_chunk_len = DUMP_CHUNK_FRAMES * 2 * 4;
        let mut byte_buf = vec![0u8; byte_chunk_len];
        let mut sample_scratch = vec![0f32; DUMP_CHUNK_FRAMES * 2];

        loop {
            let mut total_read = 0;
            while total_read < byte_chunk_len {
                match reader.read(&mut byte_buf[total_read..]) {
                    Ok(0) => break,
                    Ok(n) => total_read += n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        return Err(DecodeError::DecodeFailure(format!(
                            "P0 dump read error ({}): {e}",
                            self.path.display()
                        )));
                    }
                }
            }
            if total_read == 0 {
                break; // EOF
            }
            if total_read % 8 != 0 {
                return Err(DecodeError::DecodeFailure(format!(
                    "P0 dump truncated at non-frame boundary ({} bytes, {} mod 8 = {}): {}",
                    total_read,
                    total_read,
                    total_read % 8,
                    self.path.display()
                )));
            }
            let n_samples = total_read / 4;
            for (i, chunk) in byte_buf[..total_read].chunks_exact(4).enumerate() {
                sample_scratch[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            }
            on_chunk(DecodeChunk::Samples(&sample_scratch[..n_samples]))
                .map_err(|e| DecodeError::ConsumerError(e.to_string()))?;
        }

        on_chunk(DecodeChunk::EndOfStream)
            .map_err(|e| DecodeError::ConsumerError(e.to_string()))?;
        // 48_000 Hz, 2 channels — by construction: the dump was written
        // by pass0_decode_to_dump through StandardizedDecoder, which
        // guarantees 48k/stereo [standardized_decoder.rs:70: Ok((sample_rate, 2))].
        Ok((48_000, 2))
    }
}
