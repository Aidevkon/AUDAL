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
