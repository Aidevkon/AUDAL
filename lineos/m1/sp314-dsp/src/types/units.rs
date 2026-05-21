//! Typed physical units — sp314-dsp v2.9 §Typed Units System.
//! Zero-cost newtypes; libm-only conversions (no std::f32 transcendental methods).

/// A value in decibels (dB). May be negative. Not a linear ratio.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Decibels(pub f32);

/// A linear amplitude multiplier (0.0 = silence, 1.0 = unity, >1.0 = boost).
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct LinearGain(pub f32);

/// A frequency value in Hz.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Hertz(pub f32);

/// A count of audio samples (frame-rate agnostic).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Samples(pub u32);

impl Decibels {
    /// Convert dB to linear amplitude using libm.
    #[inline(always)]
    pub fn to_linear(self) -> LinearGain {
        LinearGain(libm::powf(10.0, self.0 / 20.0))
    }
}

impl LinearGain {
    /// Convert linear amplitude to dB using libm.
    /// 0.0 linear → -144.0 dBFS floor (24-bit noise floor).
    #[inline(always)]
    pub fn to_db(self) -> Decibels {
        if self.0 <= 0.0 {
            return Decibels(-144.0);
        }
        Decibels(20.0 * libm::log10f(self.0))
    }

    /// Unity gain.
    pub const UNITY: LinearGain = LinearGain(1.0);

    /// Silence.
    pub const SILENCE: LinearGain = LinearGain(0.0);
}

impl Samples {
    /// Convert samples to milliseconds given a sample rate.
    #[inline(always)]
    pub fn to_ms(self, sample_rate: u32) -> f32 {
        (self.0 as f32 / sample_rate as f32) * 1000.0
    }
}
