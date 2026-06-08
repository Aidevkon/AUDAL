//! Real-time telemetry types for DSP → UI bridge.
//! Authority: telemetry-bridge-spec-v1_2.md
//!
//! System B: RealtimeBridge — spectrum + goniometer from xaak.
//! Transport: Tauri IPC (lock-free ring buffer → Tauri command).
//!
//! NOT for LUFS/TruePeak — those live in LiveTelemetryResponse (System A).

/// Single real-time visualization frame from xaak audio callback.
/// Copy type — zero heap allocation per frame.
///
/// Size: 64×4 + 32×8 + 8 = 520 bytes.
/// Ring buffer: 4 × 520 = ~2KB total — INV-ST-3 safe.
///
/// INV-TB-6: Must remain Copy. No Vec, no String, no heap.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct RealtimeFrame {
    /// 64-band log-spaced spectrum magnitude in dBFS.
    /// Band 0 ≈ 20Hz, Band 63 ≈ 20kHz.
    /// -120.0 = silence (no negative infinity in Copy types).
    #[cfg_attr(feature = "serde", serde(with = "serde_arrays"))]
    pub spectrum:    [f32; 64],

    /// 32 decimated (Left, Right) sample pairs for Lissajous goniometer.
    /// Evenly spaced across the audio block.
    /// Gives smooth Lissajous curve — not a single jumping dot.
    /// INV-TB-8: must be 32 pairs minimum for smooth visualization.
    pub gonio_path:  [(f32, f32); 32],

    /// Playback position in milliseconds.
    /// Synchronized with xaak PlaybackState.position_ms.
    pub position_ms: u64,
}

impl RealtimeFrame {
    /// Silence frame — all spectrum at -120dBFS, gonio centered.
    pub fn silence(position_ms: u64) -> Self {
        Self {
            spectrum:    [-120.0f32; 64],
            gonio_path:  [(0.0f32, 0.0f32); 32],
            position_ms,
        }
    }
}

impl Default for RealtimeFrame {
    fn default() -> Self {
        Self::silence(0)
    }
}
