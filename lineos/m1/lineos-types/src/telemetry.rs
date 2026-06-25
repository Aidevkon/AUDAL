//! Real-time telemetry frame — DSP → UI bridge.
//! Authority: telemetry-bridge-spec-v1_3.md
//!
//! Binary transport via bincode (UDP).
//! serde NOT used here — src-tauri has its own local struct.

/// Real-time visualization frame — 520 bytes, Copy.
/// Transmitted via UDP as bincode binary.
/// INV-TB-6: Copy type, no heap allocation.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct RealtimeFrame {
    /// 64-band log-spaced spectrum (dBFS) for the raw input audio. -120.0 = silence.
    pub spectrum_before: [f32; 64],
    /// 64-band log-spaced spectrum (dBFS) for the mastered audio. -120.0 = silence.
    pub spectrum_after: [f32; 64],
    /// Mid energy (RMS, dBFS) for spatial visualizer.
    pub energy_mid: f32,
    /// Side energy (RMS, dBFS) for spatial visualizer.
    pub energy_side: f32,
    /// 32 (L, R) pairs for Lissajous goniometer path.
    pub gonio_path: [(f32, f32); 32],
    /// Playback position ms.
    pub position_ms: u64,
}

impl RealtimeFrame {
    pub fn silence(position_ms: u64) -> Self {
        Self {
            spectrum_before: [-120.0f32; 64],
            spectrum_after: [-120.0f32; 64],
            energy_mid: -120.0,
            energy_side: -120.0,
            gonio_path: [(0.0f32, 0.0f32); 32],
            position_ms,
        }
    }
}

impl Default for RealtimeFrame {
    fn default() -> Self {
        Self::silence(0)
    }
}
