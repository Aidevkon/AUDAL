//! engine.rs — PlaybackEngine: unified play/pause/stop/seek API.
//! Authority: Amendment A-003 §1, §4, Phase 12A P12A-004
//!
//! Owns XaakKernel (PCM) and drives CpalPlayer (cpal backend).
//! Exposes playback control without leaking PCM (A-003 §2).

use std::sync::{Arc, Mutex};
use crate::{XaakKernel, PlaybackState, PcmTransfer, player::CpalPlayer};

/// High-level playback controller.
///
/// Holds the XaakKernel (PCM owner) and CpalPlayer (cpal backend).
/// Lives in AppState as Arc<Mutex<PlaybackEngine>> — shared across M0 handlers.
pub struct PlaybackEngine {
    kernel:   Option<XaakKernel>,
    player:   CpalPlayer,
    position: Arc<Mutex<u64>>,
    playing:  bool,
}

impl PlaybackEngine {
    pub fn new() -> Self {
        Self {
            kernel:   None,
            player:   CpalPlayer::new(),
            position: Arc::new(Mutex::new(0)),
            playing:  false,
        }
    }

    /// Load PCM into the kernel. Takes exclusive ownership — caller drops samples.
    /// Previous session's kernel is released (drops PCM + emits audit event).
    pub fn load(&mut self, transfer: PcmTransfer) {
        self.player.stop();
        if let Ok(mut p) = self.position.lock() { *p = 0; }
        self.playing = false;

        // Release old kernel (emits m0d.xaak_buffer_released)
        if let Some(old) = self.kernel.take() {
            old.release();
        }

        self.kernel = Some(XaakKernel::load(transfer));
    }

    /// Play from current position. Returns error if no PCM loaded or cpal fails.
    pub fn play(&mut self) -> Result<(), String> {
        let kernel = self.kernel.as_mut()
            .ok_or_else(|| "xaak: play() called with no PCM loaded".to_string())?;
        let pos_ms   = *self.position.lock().unwrap();
        let consumer = kernel.stream_from(pos_ms);
        self.player.play(consumer, kernel.sample_rate(), kernel.channels(), self.position.clone())?;
        self.playing = true;
        tracing::info!(blob_id = %kernel.blob_id(), pos_ms, "xaak: play");
        Ok(())
    }

    /// Pause at current position.
    pub fn pause(&mut self) {
        self.player.pause();
        self.playing = false;
        tracing::debug!("xaak: paused at {}ms", self.position_ms());
    }

    /// Stop and reset position to zero.
    pub fn stop(&mut self) {
        self.player.stop();
        if let Ok(mut p) = self.position.lock() { *p = 0; }
        self.playing = false;
        tracing::debug!("xaak: stopped");
    }

    /// Seek to position_ms. Resumes if was playing.
    pub fn seek(&mut self, position_ms: u64) -> Result<(), String> {
        let was_playing = self.playing;
        self.player.stop();
        if let Ok(mut p) = self.position.lock() { *p = position_ms; }
        self.playing = false;
        if was_playing { self.play()?; }
        Ok(())
    }

    /// Current playback position in ms.
    pub fn position_ms(&self) -> u64 {
        self.position.lock().map(|p| *p).unwrap_or(0)
    }

    /// Expose PlaybackState for IPC — no PCM included (A-003 §2).
    pub fn state(&self) -> Option<PlaybackState> {
        self.kernel.as_ref().map(|k| PlaybackState {
            blob_id:     k.blob_id().to_string(),
            position_ms: self.position_ms(),
            duration_ms: k.duration_ms(),
            is_playing:  self.playing,
            sample_rate: k.sample_rate(),
            channels:    k.channels(),
        })
    }

    /// Returns true if PCM is loaded.
    pub fn has_kernel(&self) -> bool {
        self.kernel.is_some()
    }
}

impl Default for PlaybackEngine {
    fn default() -> Self { Self::new() }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_transfer(samples: usize) -> PcmTransfer {
        PcmTransfer {
            samples:     vec![0.0f32; samples],
            sample_rate: 48_000,
            channels:    2,
            blob_id:     Uuid::new_v4(),
        }
    }

    #[test]
    fn test_load_sets_kernel() {
        let mut engine = PlaybackEngine::new();
        assert!(!engine.has_kernel());
        engine.load(make_transfer(96_000));
        assert!(engine.has_kernel());
    }

    #[test]
    fn test_state_none_before_load() {
        let engine = PlaybackEngine::new();
        assert!(engine.state().is_none());
    }

    #[test]
    fn test_state_after_load() {
        let mut engine = PlaybackEngine::new();
        engine.load(make_transfer(96_000)); // 1 sec stereo 48kHz
        let s = engine.state().unwrap();
        assert_eq!(s.duration_ms, 1000);
        assert!(!s.is_playing);
    }

    #[test]
    fn test_load_replaces_kernel() {
        let mut engine = PlaybackEngine::new();
        engine.load(make_transfer(96_000));
        let id1 = engine.state().unwrap().blob_id.clone();
        engine.load(make_transfer(192_000));
        let id2 = engine.state().unwrap().blob_id.clone();
        assert_ne!(id1, id2);
        assert_eq!(engine.state().unwrap().duration_ms, 2000);
    }

    #[test]
    fn test_stop_resets_position() {
        let mut engine = PlaybackEngine::new();
        engine.load(make_transfer(96_000));
        *engine.position.lock().unwrap() = 500;
        engine.stop();
        assert_eq!(engine.position_ms(), 0);
    }

    #[test]
    fn test_seek_updates_position() {
        let mut engine = PlaybackEngine::new();
        engine.load(make_transfer(96_000));
        engine.seek(750).unwrap();
        assert_eq!(engine.position_ms(), 750);
    }

    #[test]
    fn test_state_not_exposed_pcm() {
        let mut engine = PlaybackEngine::new();
        engine.load(make_transfer(9_600));
        let state = engine.state().unwrap();
        let json  = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("\"samples\""));
        assert!(json.contains("\"position_ms\""));
        assert!(json.contains("\"duration_ms\""));
        assert!(json.contains("\"is_playing\""));
    }
}
