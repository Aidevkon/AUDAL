pub mod album;
pub mod blob;
pub mod certificate;
pub mod decode; // P7-002: symphonia audio decode
pub mod decode_actor;
pub mod deliver;
#[cfg(debug_assertions)]
pub mod dev_snapshot;
#[cfg(debug_assertions)]
pub mod dev_wait;
pub mod export;
pub mod master;
pub mod mix;
pub mod pdf_gen;
pub mod playback; // Phase 12A: play/pause/stop/seek (A-003 §8)
pub mod png_gen;
pub mod preview;
pub mod progress;
pub mod projects;
pub mod scout;
pub mod timeline;
pub mod tinder;
