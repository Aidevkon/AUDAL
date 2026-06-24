//! playback.rs — Zero-latency playback state via ArcSwap.
//! UI thread writes, audio thread reads — zero locks.
//! Foundation for EDL scrubbing (edl-spec-v1_0.md Phase 1)

#[derive(Debug, Clone, Default)]
pub struct ScrubState {
    pub position_ms: u64,
    pub playing: bool,
}

impl ScrubState {
    pub fn new() -> Self {
        Self {
            position_ms: 0,
            playing: false,
        }
    }

    pub fn playing_at(position_ms: u64) -> Self {
        Self {
            position_ms,
            playing: true,
        }
    }

    pub fn paused_at(position_ms: u64) -> Self {
        Self {
            position_ms,
            playing: false,
        }
    }
}
