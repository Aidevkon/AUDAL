use crate::dsp::biquad::{butter_hp2_prewarped, butter_lp2_prewarped, Biquad};

struct Lr4Splitter {
    lp_1: Biquad,
    lp_2: Biquad,
    hp_1: Biquad,
    hp_2: Biquad,
}

impl Lr4Splitter {
    fn new(freq: f32, sample_rate: f32) -> Self {
        Self {
            lp_1: butter_lp2_prewarped(freq, sample_rate),
            lp_2: butter_lp2_prewarped(freq, sample_rate),
            hp_1: butter_hp2_prewarped(freq, sample_rate),
            hp_2: butter_hp2_prewarped(freq, sample_rate),
        }
    }

    fn process(&mut self, x: f32) -> (f32, f32) {
        let mut lp = self.lp_1.process(x);
        lp = self.lp_2.process(lp);

        let mut hp = self.hp_1.process(x);
        hp = self.hp_2.process(hp);

        (lp, hp)
    }

    fn process_ap(&mut self, x: f32) -> f32 {
        let (lp, hp) = self.process(x);
        lp + hp
    }
}

pub struct Crossover3 {
    split1_x: Lr4Splitter,
    split2_x: Lr4Splitter,

    split2_l: Lr4Splitter,
    split1_h: Lr4Splitter,
    split2_a: Lr4Splitter,

    gain_low: f32,
    gain_mid: f32,
    gain_high: f32,
}

impl Crossover3 {
    pub fn new(sample_rate: f32, low_hz: f32, high_hz: f32) -> Self {
        Self {
            split1_x: Lr4Splitter::new(low_hz, sample_rate),
            split2_x: Lr4Splitter::new(high_hz, sample_rate),

            split2_l: Lr4Splitter::new(high_hz, sample_rate),
            split1_h: Lr4Splitter::new(low_hz, sample_rate),
            split2_a: Lr4Splitter::new(high_hz, sample_rate),

            gain_low: 1.0,
            gain_mid: 1.0,
            gain_high: 1.0,
        }
    }

    pub fn split(&mut self, input: &[f32]) -> [Vec<f32>; 3] {
        let mut low_out = Vec::with_capacity(input.len());
        let mut mid_out = Vec::with_capacity(input.len());
        let mut high_out = Vec::with_capacity(input.len());

        for &x in input {
            let (l1, h1) = self.split1_x.process(x);
            let a1 = l1 + h1;

            let (l2, h2) = self.split2_x.process(x);
            let _a2 = l2 + h2;

            let low = self.split2_l.process_ap(l1);
            let high = self.split1_h.process_ap(h2);
            let total = self.split2_a.process_ap(a1);

            let mid = total - low - high;

            low_out.push(low);
            mid_out.push(mid);
            high_out.push(high);
        }

        [low_out, mid_out, high_out]
    }

    pub fn set_gains(&mut self, low: f32, mid: f32, high: f32) {
        self.gain_low = low;
        self.gain_mid = mid;
        self.gain_high = high;
    }

    pub fn process(&mut self, io: &mut [f32]) {
        for x in io.iter_mut() {
            let (l1, h1) = self.split1_x.process(*x);
            let a1 = l1 + h1;

            let (_l2, h2) = self.split2_x.process(*x);

            let low = self.split2_l.process_ap(l1);
            let high = self.split1_h.process_ap(h2);
            let total = self.split2_a.process_ap(a1);

            let mid = total - low - high;

            *x = low * self.gain_low + mid * self.gain_mid + high * self.gain_high;
        }
    }
}

impl Default for Crossover3 {
    fn default() -> Self {
        // F-103/F-104: μία πηγή — βλ. stream_core.rs:7. ΑΔΡΑΝΕΣ σήμερα
        // (κανένα call site Crossover3::default()/:: βρέθηκε αλλού).
        Self::new(
            lineos_types::analysis::ANALYSIS_SAMPLE_RATE as f32,
            200.0,
            4000.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_noise(len: usize) -> Vec<f32> {
        // pseudo-random noise generator
        let mut noise = Vec::with_capacity(len);
        let mut seed = 12345u32;
        for _ in 0..len {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = (seed >> 16) as i16 as f32 / 32768.0;
            noise.push(n);
        }
        noise
    }

    fn generate_sine(freq: f32, sr: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f32 / sr;
                libm::sinf(2.0 * core::f32::consts::PI * freq * t)
            })
            .collect()
    }

    fn measure_rms(signal: &[f32]) -> f32 {
        let mut sum_sq = 0.0;
        for &s in signal {
            sum_sq += s * s;
        }
        (sum_sq / signal.len() as f32).sqrt()
    }

    fn rms_to_db(rms: f32) -> f32 {
        if rms < 1e-10 {
            -200.0
        } else {
            20.0 * libm::log10f(rms)
        }
    }

    #[test]
    fn test_gate1_flat_sum() {
        let sr = 48000.0;
        let mut xover = Crossover3::new(sr, 200.0, 4000.0);

        let mut noise = Vec::with_capacity(sr as usize * 4);
        let mut seed = 12345u32; // fixed seed
        for _ in 0..(sr as usize * 4) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = (seed >> 16) as i16 as f32 / 32768.0;
            noise.push(n);
        }

        let mut io = noise.clone();
        xover.process(&mut io);

        use rustfft::{num_complex::Complex, FftPlanner};
        let fft_size = 8192;
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);

        let num_windows = (noise.len() - fft_size) / (fft_size / 2);
        let mut power_in = vec![0.0_f32; fft_size / 2];
        let mut power_out = vec![0.0_f32; fft_size / 2];

        for w in 0..num_windows {
            let start = w * (fft_size / 2);
            let mut buf_in = vec![Complex { re: 0.0, im: 0.0 }; fft_size];
            let mut buf_out = vec![Complex { re: 0.0, im: 0.0 }; fft_size];
            for i in 0..fft_size {
                // Hann window
                let win = 0.5
                    * (1.0
                        - libm::cosf(
                            2.0 * core::f32::consts::PI * i as f32 / (fft_size as f32 - 1.0),
                        ));
                buf_in[i].re = noise[start + i] * win;
                buf_out[i].re = io[start + i] * win;
            }
            fft.process(&mut buf_in);
            fft.process(&mut buf_out);
            for i in 0..fft_size / 2 {
                power_in[i] += buf_in[i].norm_sqr();
                power_out[i] += buf_out[i].norm_sqr();
            }
        }

        // 1/3 octave bands from 30 Hz to 18 kHz
        let mut max_dev = 0.0_f32;
        let mut f_edge = 30.0_f32;
        let factor = (2.0_f32).powf(1.0 / 3.0);

        println!("This is a SPECTRAL test by design. Phase may differ freely.");
        while f_edge < 18000.0 {
            let next_edge = f_edge * factor;
            let bin_start = (f_edge * fft_size as f32 / sr) as usize;
            let bin_end = (next_edge * fft_size as f32 / sr) as usize;

            if bin_end > bin_start {
                let mut p_in = 0.0;
                let mut p_out = 0.0;
                for i in bin_start..bin_end {
                    p_in += power_in[i];
                    p_out += power_out[i];
                }

                let db_in = 10.0 * libm::log10f(p_in.max(1e-10));
                let db_out = 10.0 * libm::log10f(p_out.max(1e-10));
                let dev = (db_out - db_in).abs();
                if dev > max_dev {
                    max_dev = dev;
                }
            }
            f_edge = next_edge;
        }

        println!("Max dB deviation in 1/3-octave bands was {}", max_dev);
        assert!(max_dev < 0.5, "Band deviation exceeded 0.5 dB");
    }

    #[test]
    fn test_gate2_band_isolation() {
        let sr = 48000.0;
        let mut xover = Crossover3::new(sr, 200.0, 4000.0);

        let s50 = generate_sine(50.0, sr, 48000);
        let split50 = xover.split(&s50);
        let db_50_low = rms_to_db(measure_rms(&split50[0]));
        let db_50_mid = rms_to_db(measure_rms(&split50[1]));
        let db_50_high = rms_to_db(measure_rms(&split50[2]));
        assert!(db_50_low - db_50_mid > 20.0, "50Hz mid isolation failed");
        assert!(db_50_low - db_50_high > 20.0, "50Hz high isolation failed");

        let mut xover2 = Crossover3::new(sr, 200.0, 4000.0);
        let s1k = generate_sine(1000.0, sr, 48000);
        let split1k = xover2.split(&s1k);
        let db_1k_low = rms_to_db(measure_rms(&split1k[0]));
        let db_1k_mid = rms_to_db(measure_rms(&split1k[1]));
        let db_1k_high = rms_to_db(measure_rms(&split1k[2]));
        assert!(db_1k_mid - db_1k_low > 20.0, "1kHz low isolation failed");
        assert!(db_1k_mid - db_1k_high > 20.0, "1kHz high isolation failed");

        let mut xover3 = Crossover3::new(sr, 200.0, 4000.0);
        let s10k = generate_sine(10000.0, sr, 48000);
        let split10k = xover3.split(&s10k);
        let db_10k_low = rms_to_db(measure_rms(&split10k[0]));
        let db_10k_mid = rms_to_db(measure_rms(&split10k[1]));
        let db_10k_high = rms_to_db(measure_rms(&split10k[2]));
        assert!(
            db_10k_high - db_10k_mid > 20.0,
            "10kHz mid isolation failed"
        );
        assert!(
            db_10k_high - db_10k_low > 20.0,
            "10kHz low isolation failed"
        );

        println!("GATE 2: 50Hz (L:{:.1} M:{:.1} H:{:.1}), 1kHz (L:{:.1} M:{:.1} H:{:.1}), 10kHz (L:{:.1} M:{:.1} H:{:.1})",
            db_50_low, db_50_mid, db_50_high,
            db_1k_low, db_1k_mid, db_1k_high,
            db_10k_low, db_10k_mid, db_10k_high);
    }

    #[test]
    fn test_gate3_remove_midrange() {
        let sr = 48000.0;
        let mut xover = Crossover3::new(sr, 200.0, 4000.0);
        xover.set_gains(1.0, 0.0, 1.0);

        let mut s1k = generate_sine(1000.0, sr, 48000);
        let rms_1k_in = measure_rms(&s1k);
        xover.process(&mut s1k);
        let rms_1k_out = measure_rms(&s1k);
        assert!(
            rms_to_db(rms_1k_in) - rms_to_db(rms_1k_out) > 20.0,
            "1kHz not attenuated enough"
        );

        let mut xover2 = Crossover3::new(sr, 200.0, 4000.0);
        xover2.set_gains(1.0, 0.0, 1.0);
        let mut s50 = generate_sine(50.0, sr, 48000);
        let rms_50_in = measure_rms(&s50);
        xover2.process(&mut s50);
        let rms_50_out = measure_rms(&s50);
        assert!(
            (rms_to_db(rms_50_in) - rms_to_db(rms_50_out)).abs() < 1.0,
            "50Hz dropped more than 1dB"
        );

        let mut xover3 = Crossover3::new(sr, 200.0, 4000.0);
        xover3.set_gains(1.0, 0.0, 1.0);
        let mut s10k = generate_sine(10000.0, sr, 48000);
        let rms_10k_in = measure_rms(&s10k);
        xover3.process(&mut s10k);
        let rms_10k_out = measure_rms(&s10k);
        let diff = (rms_to_db(rms_10k_in) - rms_to_db(rms_10k_out)).abs();
        println!(
            "10kHz: in={} out={} diff={}",
            rms_to_db(rms_10k_in),
            rms_to_db(rms_10k_out),
            diff
        );
        assert!(diff < 1.0, "10kHz dropped more than 1dB");
    }

    #[test]
    fn test_gate4_lengths_and_no_nan() {
        let sr = 48000.0;
        let mut xover = Crossover3::new(sr, 200.0, 4000.0);
        let input: Vec<f32> = (0..48000)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let split = xover.split(&input);

        assert_eq!(split[0].len(), input.len());
        assert_eq!(split[1].len(), input.len());
        assert_eq!(split[2].len(), input.len());

        for i in 0..input.len() {
            assert!(split[0][i].is_finite());
            assert!(split[1][i].is_finite());
            assert!(split[2][i].is_finite());
        }
    }

    #[test]
    fn test_gate5_determinism() {
        let sr = 48000.0;
        let input = generate_noise(4800);

        let mut xover1 = Crossover3::new(sr, 200.0, 4000.0);
        let split1 = xover1.split(&input);

        let mut xover2 = Crossover3::new(sr, 200.0, 4000.0);
        let split2 = xover2.split(&input);

        for b in 0..3 {
            for i in 0..input.len() {
                assert_eq!(split1[b][i], split2[b][i]);
            }
        }
    }
}
