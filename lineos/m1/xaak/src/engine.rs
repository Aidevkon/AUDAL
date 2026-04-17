//! engine.rs — PlaybackEngine: unified play/pause/stop/seek API.
//! Authority: Amendment A-003 §1, §4, Phase 12A P12A-004
//!
//! cpal::Stream is !Send (NotSendSyncAcrossAllPlatforms).
//! PlaybackEngine MUST live on a single dedicated thread.
//!
//! Architecture:
//!   AppState holds PlaybackHandle (Sender<PlaybackCmd> — Send + Sync).
//!   PlaybackWorker runs on a std::thread with the cpal::Stream.
//!   Axum handlers send commands over mpsc; responses return via oneshot.

use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Sender, Receiver, SyncSender};
use std::thread;
use crate::{XaakKernel, PlaybackState, PcmTransfer, player::CpalPlayer};

// ── Commands ──────────────────────────────────────────────────────────────────

pub enum PlaybackCmd {
    Load(PcmTransfer),
    Play,
    Pause,
    Stop,
    Seek(u64),
    /// Caller sends a SyncSender; worker replies with Option<PlaybackState>.
    GetState(SyncSender<Option<PlaybackState>>),
}

// ── PlaybackHandle — the Send/Sync token held by AppState ────────────────────

/// Cloneable handle to the playback worker thread.
/// Contains only a `Sender<PlaybackCmd>` which is `Send + Sync`.
#[derive(Clone)]
pub struct PlaybackHandle {
    tx: Sender<PlaybackCmd>,
}

impl PlaybackHandle {
    /// Spawn the playback worker thread and return a handle.
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<PlaybackCmd>();
        thread::Builder::new()
            .name("xaak-playback".into())
            .spawn(move || PlaybackWorker::run(rx))
            .expect("xaak: failed to spawn playback worker thread");
        Self { tx }
    }

    pub fn load(&self, transfer: PcmTransfer) {
        let _ = self.tx.send(PlaybackCmd::Load(transfer));
    }

    pub fn play(&self) {
        let _ = self.tx.send(PlaybackCmd::Play);
    }

    pub fn pause(&self) {
        let _ = self.tx.send(PlaybackCmd::Pause);
    }

    pub fn stop(&self) {
        let _ = self.tx.send(PlaybackCmd::Stop);
    }

    pub fn seek(&self, position_ms: u64) {
        let _ = self.tx.send(PlaybackCmd::Seek(position_ms));
    }

    /// Synchronously fetch PlaybackState from the worker thread.
    pub fn get_state(&self) -> Option<PlaybackState> {
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        let _ = self.tx.send(PlaybackCmd::GetState(resp_tx));
        resp_rx.recv().unwrap_or(None)
    }
}

// ── PlaybackWorker — owns cpal::Stream on a dedicated thread ─────────────────

struct PlaybackWorker {
    kernel:   Option<XaakKernel>,
    player:   CpalPlayer,
    position: Arc<Mutex<u64>>,
    playing:  bool,
}

impl PlaybackWorker {
    fn new() -> Self {
        Self {
            kernel:   None,
            player:   CpalPlayer::new(),
            position: Arc::new(Mutex::new(0)),
            playing:  false,
        }
    }

    /// Drive the command loop — blocks the dedicated thread.
    fn run(rx: Receiver<PlaybackCmd>) {
        let mut worker = Self::new();
        for cmd in rx {
            match cmd {
                PlaybackCmd::Load(transfer)    => worker.load(transfer),
                PlaybackCmd::Play              => { let _ = worker.play(); }
                PlaybackCmd::Pause             => worker.pause(),
                PlaybackCmd::Stop              => worker.stop(),
                PlaybackCmd::Seek(ms)          => { let _ = worker.seek(ms); }
                PlaybackCmd::GetState(resp_tx) => {
                    let _ = resp_tx.send(worker.state());
                }
            }
        }
        tracing::info!("xaak: playback worker thread exiting");
    }

    fn load(&mut self, transfer: PcmTransfer) {
        self.player.stop();
        if let Ok(mut p) = self.position.lock() { *p = 0; }
        self.playing = false;
        if let Some(old) = self.kernel.take() { old.release(); }
        self.kernel = Some(XaakKernel::load(transfer));
    }

    fn play(&mut self) -> Result<(), String> {
        let kernel = self.kernel.as_mut()
            .ok_or_else(|| "xaak: play() — no PCM loaded".to_string())?;
        let pos_ms   = *self.position.lock().unwrap();
        let consumer = kernel.stream_from(pos_ms);
        self.player.play(consumer, kernel.sample_rate(), kernel.channels(), self.position.clone())?;
        self.playing = true;
        tracing::info!(blob_id = %kernel.blob_id(), pos_ms, "xaak: play");
        Ok(())
    }

    fn pause(&mut self) {
        self.player.pause();
        self.playing = false;
        tracing::debug!("xaak: paused at {}ms", self.position_ms());
    }

    fn stop(&mut self) {
        self.player.stop();
        if let Ok(mut p) = self.position.lock() { *p = 0; }
        self.playing = false;
    }

    fn seek(&mut self, position_ms: u64) -> Result<(), String> {
        let was_playing = self.playing;
        self.player.stop();
        if let Ok(mut p) = self.position.lock() { *p = position_ms; }
        self.playing = false;
        if was_playing { self.play()?; }
        Ok(())
    }

    fn position_ms(&self) -> u64 {
        self.position.lock().map(|p| *p).unwrap_or(0)
    }

    fn state(&self) -> Option<PlaybackState> {
        self.kernel.as_ref().map(|k| PlaybackState {
            blob_id:     k.blob_id().to_string(),
            position_ms: self.position_ms(),
            duration_ms: k.duration_ms(),
            is_playing:  self.playing,
            sample_rate: k.sample_rate(),
            channels:    k.channels(),
        })
    }
}

// ── PlaybackEngine — backwards-compat façade for tests ───────────────────────
// Used by unit tests that don't need the thread boundary.

/// Synchronous engine — used in unit tests only.
/// Production code uses PlaybackHandle.
#[cfg(test)]
pub struct PlaybackEngine {
    kernel:   Option<XaakKernel>,
    player:   CpalPlayer,
    pub position: Arc<Mutex<u64>>,
    playing:  bool,
}

#[cfg(test)]
impl PlaybackEngine {
    pub fn new() -> Self {
        Self {
            kernel:   None,
            player:   CpalPlayer::new(),
            position: Arc::new(Mutex::new(0)),
            playing:  false,
        }
    }

    pub fn load(&mut self, transfer: PcmTransfer) {
        self.player.stop();
        if let Ok(mut p) = self.position.lock() { *p = 0; }
        self.playing = false;
        if let Some(old) = self.kernel.take() { old.release(); }
        self.kernel = Some(XaakKernel::load(transfer));
    }

    pub fn stop(&mut self) {
        self.player.stop();
        if let Ok(mut p) = self.position.lock() { *p = 0; }
        self.playing = false;
    }

    pub fn seek(&mut self, position_ms: u64) -> Result<(), String> {
        if let Ok(mut p) = self.position.lock() { *p = position_ms; }
        Ok(())
    }

    pub fn position_ms(&self) -> u64 {
        self.position.lock().map(|p| *p).unwrap_or(0)
    }

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

    pub fn has_kernel(&self) -> bool { self.kernel.is_some() }
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
        engine.load(make_transfer(96_000));
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
