struct XorShiftRng {
    state: u64,
}

impl XorShiftRng {
    fn new(seed: u64) -> Self {
        let state = if seed == 0 { 0xDEAD_BEEF_CAFE_1234 } else { seed };
        Self { state }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    #[inline]
    fn next_float(&mut self) -> f32 {
        let mantissa = (self.next_u64() & 0xFFFFFF) as f32;
        (mantissa / 16777216.0_f32) - 0.5_f32
    }
}

pub struct TpdfDither {
    rng: XorShiftRng,
}

impl TpdfDither {
    pub fn new(seed: u64) -> Self {
        Self { rng: XorShiftRng::new(seed) }
    }

    #[inline]
    pub fn process_sample(&mut self, x: f32, positive: bool) -> i32 {
        let max_val = if positive { 8388607.0_f32 } else { 8388608.0_f32 };
        let scaled = x * max_val;

        let dither = self.rng.next_float() + self.rng.next_float();

        (scaled + dither).clamp(-8388608.0_f32, 8388607.0_f32) as i32
    }
}
