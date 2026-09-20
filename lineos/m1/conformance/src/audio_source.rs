//! AudioSource — the streaming input contract.
//!
//! episode_render pulls interleaved f32 samples
//! from anything implementing this trait, so it
//! doesn't care whether the bytes come from a
//! raw file (LazyAudioReader), a resampling
//! streaming decoder (StandardizedAudioStream),
//! or a test mock. The render layer is pure DSP;
//! decode/resample/normalize live behind this
//! contract.
//!
//! Linear, forward-only: no seek. The render
//! always reads from start to end.

/// A forward-only source of interleaved f32
/// audio. `fill_buffer` reports FRAMES, not
/// samples; the buffer length must be an exact
/// multiple of `channels()`.
pub trait AudioSource {
    /// Sample rate of the delivered audio.
    fn sample_rate(&self) -> u32;

    /// Channel count of the delivered audio.
    /// (buffer length in fill_buffer must be a
    /// multiple of this.)
    fn channels(&self) -> usize;

    /// Total frame count if known ahead of time
    /// (metadata only — never reads the audio).
    fn total_frames_hint(&self) -> Option<u64>;

    /// Fill `buffer` with interleaved f32 and
    /// return the number of FRAMES written.
    /// Returns 0 at end of stream. `buffer.len()`
    /// must be an exact multiple of `channels()`.
    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String>;
}
