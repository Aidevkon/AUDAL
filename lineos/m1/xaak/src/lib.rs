//! xaak.rs — Internal PCM Kernel
//! Authority: Amendment A-003 §1–§4
//! Phase 12 activation. Lock-free ring buffer. Single PCM owner.
//!
//! Architecture (A-003 §1):
//!   sp314-dsp → PcmTransfer → XaakKernel (ring buffer) → CpalPlayer → speakers
//!
//! Invariants:
//!   - xaak is the SOLE PCM owner after mastering completes (A-003 §2)
//!   - Arc<RwLock<Vec<f32>>> is FORBIDDEN — ring buffer only (A-003 §4)
//!   - No PCM crosses the Tauri IPC boundary (A-003 §2)
//!   - cpal sits UNDER xaak, not above it (A-003 §8)

pub mod crossover;
pub mod downmix_bs775;
pub mod engine;
pub mod flavours;
pub mod player;
pub mod repo;
pub mod spectrum;
pub mod telemetry;
pub mod telemetry_worker;
pub mod tinder;
pub use flavours::{from_name as flavour_from_name, ALL as FLAVOURS};
pub use repo::{AudioRepo, DspState, MixCommit};
pub use tinder::{generate_variations, weighted_centroid};

pub mod playback;
pub use playback::ScrubState;

// ── PlaybackState ─────────────────────────────────────────────────────────────

/// Playback state for Tauri IPC — no PCM, metrics only (A-003 §2).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlaybackState {
    pub blob_id: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub is_playing: bool,
    pub sample_rate: u32,
    pub channels: u16,
    pub ab_target: String,
    pub gain_match: bool,
}

use ringbuf::{traits::*, HeapRb};
use uuid::Uuid;

pub const TARGET_SAMPLE_RATE: u32 = 48_000;
pub const TARGET_CHANNELS: u16 = 2;

// ── PcmTransfer ───────────────────────────────────────────────────────────────

/// One-way ownership transfer of PCM from M0 master handler to xaak.
/// After this is passed to XaakKernel::load(), the caller must not
/// access the samples — ownership is moved unconditionally (A-003 §2).
pub struct PcmTransfer {
    pub pcm_path: std::path::PathBuf,
    pub sample_rate: u32,
    pub channels: u16,
    pub blob_id: Uuid,
    pub num_frames: usize,
}

// ── XaakKernel ────────────────────────────────────────────────────────────────

/// Internal PCM kernel. The sole owner of PCM for the session lifetime.
///
/// A-003 §4: uses lock-free HeapRb ring buffer, NOT Arc<RwLock<Vec<f32>>>.
/// Consumers get a Box<dyn Pop<f32>> from stream_from() — zero-copy slice.
pub struct XaakKernel {
    blob_id: Uuid,
    sample_rate: u32,
    channels: u16,
    duration_ms: u64,
    num_frames: usize,
    /// Authoritative PCM backing store — supports seek via slice offset.
    pcm: memmap2::Mmap,
    /// Kept alive to hold the ring buffer while consumer exists.
    _producer: Option<Box<dyn std::any::Any + Send>>,
}

impl XaakKernel {
    /// Take exclusive ownership of PCM from a PcmTransfer.
    ///
    /// Emits A-003 §2 audit event: m0d.xaak_buffer_allocated.
    /// After this returns, the caller's samples field is consumed.
    pub fn load(transfer: PcmTransfer) -> Self {
        let file = std::fs::File::open(&transfer.pcm_path).expect("Failed to open PCM file");
        let mmap = unsafe { memmap2::Mmap::map(&file).expect("Failed to map PCM file") };
        // We use the actual valid frames, not the mapped file size
        let num_samples = transfer.num_frames * transfer.channels as usize;

        let duration_ms = if transfer.sample_rate > 0 && transfer.channels > 0 {
            (num_samples as u64 * 1000) / (transfer.sample_rate as u64 * transfer.channels as u64)
        } else {
            0
        };

        // A-003 §2: audit trail — xaak_buffer_allocated
        tracing::info!(
            event       = "m0d.xaak_buffer_allocated",
            blob_id     = %transfer.blob_id,
            size_bytes  = num_samples * 4,
            sample_rate = transfer.sample_rate,
            channels    = transfer.channels,
            duration_ms = duration_ms,
            "xaak: PCM buffer loaded"
        );

        Self {
            blob_id: transfer.blob_id,
            sample_rate: transfer.sample_rate,
            channels: transfer.channels,
            duration_ms,
            num_frames: transfer.num_frames,
            pcm: mmap,
            _producer: None,
        }
    }

    /// Create a streaming consumer for cpal playback starting at position_ms.
    ///
    /// The ring buffer is sized to hold all PCM from the seek position.
    /// Returns a Box<dyn Consumer> so the concrete ringbuf type is erased.
    pub fn stream_from(&mut self, position_ms: u64) -> impl Consumer<Item = f32> {
        // frame_offset = number of frames (NOT samples) to skip
        let frame_offset = if self.sample_rate > 0 {
            (position_ms * self.sample_rate as u64 / 1000) as usize
        } else {
            0
        };

        let slice = if frame_offset < self.num_frames {
            let frames_to_play = self.num_frames - frame_offset;
            // byte offset = frame_offset * channels * 4 bytes per f32
            let byte_offset = frame_offset * self.channels as usize * 4;
            let pcm_bytes = &self.pcm[byte_offset..];
            unsafe {
                let full_slice = std::slice::from_raw_parts(
                    pcm_bytes.as_ptr() as *const f32,
                    pcm_bytes.len() / 4,
                );
                &full_slice[..(frames_to_play * self.channels as usize)]
            }
        } else {
            &[]
        };

        let capacity = slice.len().max(4096);
        let rb = HeapRb::<f32>::new(capacity);
        let (mut prod, cons) = rb.split();

        let pushed = prod.push_slice(slice);
        tracing::debug!(
            blob_id = %self.blob_id, position_ms,
            slice_len = slice.len(), pushed,
            "xaak: stream_from ring buffer filled"
        );

        // Keep producer alive so the ring buffer is not dropped
        self._producer = Some(Box::new(prod));
        cons
    }

    // ── Accessors (no PCM exposure) ───────────────────────────────────────────

    pub fn blob_id(&self) -> Uuid {
        self.blob_id
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn channels(&self) -> u16 {
        self.channels
    }
    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }
    /// Returns number of f32 samples in the PCM buffer.
    pub fn pcm_len(&self) -> usize {
        self.pcm.len() / 4
    }
    /// Release PCM and emit A-003 §2 audit event: m0d.xaak_buffer_released.
    pub fn release(self) {
        tracing::info!(
            event       = "m0d.xaak_buffer_released",
            blob_id     = %self.blob_id,
            duration_ms = self.duration_ms,
            "xaak: PCM buffer released"
        );
        // self.pcm dropped here — exclusive ownership ensures no use-after-free
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_transfer(samples: usize) -> PcmTransfer {
        let path = std::path::PathBuf::from(format!("/tmp/xaak-test-{}.pcm", uuid::Uuid::new_v4()));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len((samples * 4) as u64).unwrap();
        PcmTransfer {
            pcm_path: path,
            sample_rate: 48000,
            channels: 2,
            blob_id: uuid::Uuid::new_v4(),
            num_frames: samples / 2,
        }
    }

    #[test]
    fn test_duration_ms_calculation() {
        // stereo 48kHz → 48000 * 2 = 96000 samples/sec
        // 96000 samples → 1000ms
        let xaak = XaakKernel::load(make_transfer(96_000));
        assert_eq!(xaak.duration_ms(), 1000);
    }

    #[test]
    fn test_pcm_len_matches_transfer() {
        let xaak = XaakKernel::load(make_transfer(4096));
        assert_eq!(xaak.pcm_len(), 4096);
    }

    #[test]
    fn test_stream_from_zero_returns_consumer() {
        let mut xaak = XaakKernel::load(make_transfer(48_000));
        let consumer = xaak.stream_from(0);
        // Consumer should hold all samples (occupied_len = filled samples)
        assert_eq!(consumer.occupied_len(), 48_000);
    }

    #[test]
    fn test_stream_from_seek_mid() {
        // Seek to 500ms in stereo 48kHz → skip 48000 * 0.5 * 2 = 48000 samples
        let mut xaak = XaakKernel::load(make_transfer(192_000)); // 2 sec
        let consumer = xaak.stream_from(500);
        // expect 192000 - 48000 = 144000 samples
        assert_eq!(consumer.occupied_len(), 144_000);
    }

    #[test]
    fn test_stream_from_past_end_returns_empty() {
        let mut xaak = XaakKernel::load(make_transfer(96_000)); // 1 sec
        let consumer = xaak.stream_from(5000); // seek to 5s > 1s duration
        assert_eq!(consumer.occupied_len(), 0);
    }

    #[test]
    fn test_playback_state_no_pcm() {
        let state = PlaybackState {
            blob_id: "test-blob".into(),
            position_ms: 1234,
            duration_ms: 60_000,
            is_playing: true,
            sample_rate: TARGET_SAMPLE_RATE,
            channels: TARGET_CHANNELS,
            ab_target: "a".to_string(),
            gain_match: true,
        };
        let json = serde_json::to_string(&state).unwrap();
        // Must NOT contain any PCM — just metrics
        assert!(json.contains("\"is_playing\":true"));
        assert!(!json.contains("samples"));
    }
}
