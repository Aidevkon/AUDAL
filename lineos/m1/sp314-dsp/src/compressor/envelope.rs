pub fn rc_coeff(time_ms: f32, sample_rate: u32) -> f32 {
    libm::expf(-2.2_f32 / (time_ms * 0.001_f32 * sample_rate as f32))
}

pub struct EnvelopeFollower {
    envelope: f32,
    attack_coeff: f32,
    release_coeff: f32,
}

impl EnvelopeFollower {
    pub fn new(attack_ms: f32, release_ms: f32, sample_rate: u32) -> Self {
        let attack_coeff = 1.0 - rc_coeff(attack_ms, sample_rate);
        let release_coeff = 1.0 - rc_coeff(release_ms, sample_rate);
        Self {
            envelope: 0.0,
            attack_coeff,
            release_coeff,
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        // Envelope detection and RC smoothing run in LINEAR domain.
        // Convert to dB AFTER smoothing.
        // Smoothing in dB domain sounds robotic (constant dB/sec rate).
        // Smoothing in linear domain sounds natural (exponential dB/sec rate).
        let x_abs = libm::fabsf(x);

        if x_abs > self.envelope {
            self.envelope += (x_abs - self.envelope) * self.attack_coeff;
        } else {
            self.envelope += (x_abs - self.envelope) * self.release_coeff;
        }

        if libm::fabsf(self.envelope) < 1e-15 {
            self.envelope = 0.0;
        }

        20.0 * libm::log10f(self.envelope + 1e-10)
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
    }
}
