//! lame-sys — FFI bindings to libmp3lame (LGPL, dynamically linked).
//!
//! Authority: docs/licenses/LAME-LGPL-NOTICE.md §1 · Creator OS Constitution v2.6
//!
//! Only the subset of the LAME C API needed for MP3 export is exposed here:
//!   - lame_init         — create encoder context
//!   - lame_set_*        — configure sample rate (in/out), channels, mode,
//!     VBR/CBR, bitrate, quality
//!   - lame_init_params  — finalise configuration
//!   - lame_encode_buffer_interleaved_ieee_float — encode interleaved f32 PCM (2ch)
//!   - lame_encode_buffer_ieee_float — encode planar/mono f32 PCM
//!   - lame_encode_flush_nogap — flush without decoder delay padding
//!   - lame_close        — destroy encoder context
//!   - lame_get_lametag_frame — not used by Creator OS (no ID3 tag needed)
//!
//! The library is linked DYNAMICALLY (see build.rs). Static linking is forbidden
//! by the LGPL v2.1 — see docs/licenses/LAME-LGPL-NOTICE.md.
//!
//! Header reference: /usr/include/lame/lame.h

#![allow(non_camel_case_types, non_snake_case, dead_code)]

use std::os::raw::{c_float, c_int, c_uchar};

/// Opaque LAME global flags context. Obtained from `lame_init()`.
pub enum lame_global_flags {}

// C enums are typically equivalent to int ABI-wise; #[repr(C)] ensures this layout.
#[repr(C)]
pub enum vbr_mode {
    vbr_off = 0,
    vbr_mt,
    vbr_rh,
    vbr_abr,
    vbr_mtrh,
    vbr_max_indicator,
}

#[repr(C)]
pub enum MPEG_mode {
    STEREO = 0,
    JOINT_STEREO,
    DUAL_CHANNEL,
    MONO,
    NOT_SET,
    MAX_INDICATOR,
}

/// Alias used throughout the LAME API.
pub type lame_t = *mut lame_global_flags;

extern "C" {
    /// Initialise a new LAME encoder context. Returns NULL on failure.
    pub fn lame_init() -> lame_t;

    /// Set number of channels (1 = mono, 2 = stereo).
    pub fn lame_set_num_channels(gfp: lame_t, channels: c_int) -> c_int;

    /// Set input sample rate in Hz (e.g. 48000).
    pub fn lame_set_in_samplerate(gfp: lame_t, rate: c_int) -> c_int;

    /// Set output sample rate in Hz. default = 0, which means LAME picks best value
    /// based on the amount of compression.
    pub fn lame_set_out_samplerate(gfp: lame_t, rate: c_int) -> c_int;

    /// Set VBR mode (e.g. vbr_off for CBR).
    pub fn lame_set_VBR(gfp: lame_t, mode: vbr_mode) -> c_int;

    /// Set bitrate in kbps.
    pub fn lame_set_brate(gfp: lame_t, brate: c_int) -> c_int;

    /// Set MPEG mode (e.g. MONO).
    pub fn lame_set_mode(gfp: lame_t, mode: MPEG_mode) -> c_int;

    /// Set VBR/CBR quality (2 = near-lossless mastering grade; 0 = highest).
    pub fn lame_set_quality(gfp: lame_t, quality: c_int) -> c_int;

    /// Finalise encoder parameters. Must be called before encoding.
    /// Returns < 0 on error.
    pub fn lame_init_params(gfp: lame_t) -> c_int;

    /// Encode interleaved IEEE 754 float PCM.
    ///
    /// * `pcm`        — interleaved L/R samples
    /// * `num_samples`— number of samples *per channel* (not total)
    /// * `mp3buf`     — output buffer (must be pre-allocated)
    /// * `mp3buf_size`— size of output buffer in bytes
    ///
    /// Returns bytes written, 0 if buffer needs more input, or < 0 on error.
    pub fn lame_encode_buffer_interleaved_ieee_float(
        gfp: lame_t,
        pcm: *const c_float,
        num_samples: c_int,
        mp3buf: *mut c_uchar,
        mp3buf_size: c_int,
    ) -> c_int;

    /// Encode IEEE 754 float PCM into mono or dual-mono.
    ///
    /// * `pcm_l`      — left channel samples (or mono channel)
    /// * `pcm_r`      — right channel samples (or NULL / duplicate for mono)
    /// * `nsamples`   — number of samples *per channel*
    /// * `mp3buf`     — output buffer (must be pre-allocated)
    /// * `mp3buf_size`— size of output buffer in bytes
    ///
    /// Returns bytes written, 0 if buffer needs more input, or < 0 on error.
    pub fn lame_encode_buffer_ieee_float(
        gfp: lame_t,
        pcm_l: *const c_float,
        pcm_r: *const c_float,
        nsamples: c_int,
        mp3buf: *mut c_uchar,
        mp3buf_size: c_int,
    ) -> c_int;

    /// Flush the encoder and write remaining frames. No decoder delay compensation.
    ///
    /// Returns bytes written or < 0 on error.
    pub fn lame_encode_flush_nogap(gfp: lame_t, mp3buf: *mut c_uchar, mp3buf_size: c_int) -> c_int;

    /// Free LAME context. Call after encoding is complete.
    pub fn lame_close(gfp: lame_t) -> c_int;

    /// Get LAME version string (for logging / compliance output).
    pub fn get_lame_version() -> *const std::os::raw::c_char;
}
