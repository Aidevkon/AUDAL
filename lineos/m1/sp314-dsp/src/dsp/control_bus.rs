use arc_swap::ArcSwap;
use std::sync::Arc;

// Duck depth: hardcoded until the persona/settings layer lands (W3.b+).
const DUCK_FLOOR_DB: f32 = -12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlFrame {
    pub p_speech: f32,    // 0..=1, from the micro-VAD
    pub duck_gain: f32,   // linear gain for the Music bus, 0..=1
    pub voice_gate: bool, // Voice bus open/closed
    pub snr_db: f32,
}

/// A lock-free Single-Writer Multiple-Reader (SWMR) bus using ArcSwap.
/// Read path performs no allocations. Publish allocates a single Arc per frame.
pub struct ControlBus {
    data: ArcSwap<ControlFrame>,
}

impl ControlBus {
    pub fn new(_sample_rate: f32) -> Self {
        Self {
            data: ArcSwap::from_pointee(ControlFrame {
                p_speech: 0.0,
                duck_gain: 1.0,
                voice_gate: false,
                snr_db: 0.0,
            }),
        }
    }

    pub fn publish(&self, frame: ControlFrame) {
        self.data.store(Arc::new(frame));
    }

    pub fn read(&self) -> ControlFrame {
        **self.data.load()
    }
}

pub struct Ducker {
    duck_gain: f32,
    attack_alpha: f32,
    release_alpha: f32,
    last_valid_p: f32,
    nonfinite_count: u32,
}

impl Ducker {
    pub fn new(sample_rate: f32) -> Self {
        let frame_rate = sample_rate / crate::analysis::vad_sensors::FRAME_SAMPLES as f32;

        let attack_alpha = f32::exp(-2.2 / (0.030 * frame_rate));
        let release_alpha = f32::exp(-2.2 / (0.500 * frame_rate));

        Self {
            duck_gain: 1.0,
            attack_alpha,
            release_alpha,
            last_valid_p: 0.0,
            nonfinite_count: 0,
        }
    }

    pub fn last_valid_p(&self) -> f32 {
        self.last_valid_p
    }

    pub fn nonfinite_count(&self) -> u32 {
        self.nonfinite_count
    }

    pub fn update(&mut self, p_speech: f32) -> f32 {
        if !p_speech.is_finite() {
            self.nonfinite_count = self.nonfinite_count.saturating_add(1);
            return self.duck_gain;
        }

        self.last_valid_p = p_speech;

        let floor = 10.0f32.powf(DUCK_FLOOR_DB / 20.0);
        let p_speech_clamped = p_speech.clamp(0.0, 1.0);
        let target = 1.0 - p_speech_clamped * (1.0 - floor);

        if target < self.duck_gain {
            self.duck_gain = target + self.attack_alpha * (self.duck_gain - target);
        } else {
            self.duck_gain = target + self.release_alpha * (self.duck_gain - target);
        }

        self.duck_gain = self.duck_gain.clamp(0.0, 1.0);
        self.duck_gain
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    // Gate 1: read-back exact
    #[test]
    fn test_bus_read_exact_publish() {
        let bus = ControlBus::new(48000.0);
        let frame = ControlFrame {
            p_speech: 0.75,
            duck_gain: 0.5,
            voice_gate: true,
            snr_db: 12.3,
        };
        bus.publish(frame);
        let read = bus.read();
        assert_eq!(read.p_speech, 0.75);
        assert_eq!(read.duck_gain, 0.5);
        assert_eq!(read.voice_gate, true);
        assert_eq!(read.snr_db, 12.3);
    }

    // Gate 2: sane default
    #[test]
    fn test_bus_sane_default() {
        let bus = ControlBus::new(48000.0);
        let frame = bus.read();
        assert_eq!(frame.p_speech, 0.0);
        assert_eq!(frame.duck_gain, 1.0);
        assert_eq!(frame.voice_gate, false);
    }

    // Shared helper for testing ballistics
    fn run_ballistics_test() -> (f32, f32) {
        let sample_rate = 48000.0;
        let mut ducker = Ducker::new(sample_rate);
        // test clock must match Ducker's frame rate
        let frame_rate = sample_rate / crate::analysis::vad_sensors::FRAME_SAMPLES as f32;
        let frame_ms = 1000.0 / frame_rate;

        // ATTACK: from silence, feed p_speech = 1.0
        let threshold_linear = 10.0f32.powf(-11.0 / 20.0);
        let mut attack_frames = 0;
        loop {
            let gain = ducker.update(1.0);
            attack_frames += 1;
            if gain <= threshold_linear || attack_frames > 1000 {
                break;
            }
        }
        let attack_ms = attack_frames as f32 * frame_ms;

        // Ensure steady state
        for _ in 0..100 {
            ducker.update(1.0);
        }

        // RELEASE: from fully ducked, feed p_speech = 0.0
        let release_threshold_linear = 10.0f32.powf(-1.0 / 20.0);
        let mut release_frames = 0;
        loop {
            let gain = ducker.update(0.0);
            release_frames += 1;
            if gain >= release_threshold_linear || release_frames > 5000 {
                break;
            }
        }
        let release_ms = release_frames as f32 * frame_ms;
        (attack_ms, release_ms)
    }

    // Gate 3: attack ms
    #[test]
    fn test_ducker_attack_ms() {
        let (attack_ms, _) = run_ballistics_test();
        println!("Measured Attack: {:.2} ms", attack_ms);
        assert!(
            attack_ms >= 20.0 && attack_ms <= 60.0,
            "Attack ms outside gate"
        );
    }

    // Gate 4: release ms
    #[test]
    fn test_ducker_release_ms() {
        let (_, release_ms) = run_ballistics_test();
        println!("Measured Release: {:.2} ms", release_ms);
        assert!(
            release_ms >= 300.0 && release_ms <= 800.0,
            "Release ms outside gate"
        );
    }

    // Gate 5: asymmetry ratio
    #[test]
    fn test_ducker_asymmetry_ratio() {
        let (attack_ms, release_ms) = run_ballistics_test();
        let ratio = release_ms / attack_ms;
        println!("Measured Ratio: {:.2}x", ratio);
        assert!(ratio >= 5.0, "Asymmetry ratio outside gate");
    }

    // Gate 6: bounds+no NaN leaves update
    #[test]
    fn test_ducker_bounds() {
        let mut ducker = Ducker::new(48000.0);
        for p in [
            0.0,
            1.0,
            0.5,
            1.5,
            -0.5,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let gain = ducker.update(p);
            assert!(
                gain >= 0.0 && gain <= 1.0,
                "Bounds check failed: gain = {}",
                gain
            );
            assert!(gain.is_finite(), "Gain is not finite for p_speech = {}", p);
        }
    }

    // Gate 7: determinism
    #[test]
    fn test_ducker_determinism() {
        let mut d1 = Ducker::new(48000.0);
        let mut d2 = Ducker::new(48000.0);
        let seq = [0.0, 0.1, 0.5, 0.9, 1.0, 1.0, 0.2, 0.0, 0.0];
        for &p in &seq {
            assert_eq!(d1.update(p), d2.update(p));
        }
    }

    // Gate 8: Feed NaN
    #[test]
    fn test_ducker_nan_freeze() {
        let mut ducker = Ducker::new(48000.0);
        // feed 1.0 until fully ducked
        for _ in 0..100 {
            ducker.update(1.0);
        }
        let pre_nan = ducker.update(1.0);

        // feed NaN for 100 frames
        for _ in 0..100 {
            let gain = ducker.update(f32::NAN);
            assert_eq!(
                gain.to_bits(),
                pre_nan.to_bits(),
                "duck_gain did not perfectly freeze on NaN"
            );
        }

        // feed 0.0 and confirm release
        let release_gain = ducker.update(0.0);
        assert!(
            release_gain > pre_nan,
            "did not begin releasing after NaN freeze"
        );
    }

    // Gate 9: Feed Inf and -Inf
    #[test]
    fn test_ducker_inf_freeze() {
        let mut ducker = Ducker::new(48000.0);
        for _ in 0..100 {
            ducker.update(1.0);
        }
        let pre_inf = ducker.update(1.0);

        for _ in 0..100 {
            let gain = ducker.update(f32::INFINITY);
            assert_eq!(
                gain.to_bits(),
                pre_inf.to_bits(),
                "duck_gain did not perfectly freeze on INFINITY"
            );
        }
        for _ in 0..100 {
            let gain = ducker.update(f32::NEG_INFINITY);
            assert_eq!(
                gain.to_bits(),
                pre_inf.to_bits(),
                "duck_gain did not perfectly freeze on NEG_INFINITY"
            );
        }

        let release_gain = ducker.update(0.0);
        assert!(
            release_gain > pre_inf,
            "did not begin releasing after Inf freeze"
        );
    }

    // Gate 10: nonfinite_count and last_valid_p
    #[test]
    fn test_ducker_nonfinite_metrics() {
        let mut ducker = Ducker::new(48000.0);
        ducker.update(0.7);
        assert_eq!(ducker.last_valid_p(), 0.7);
        assert_eq!(ducker.nonfinite_count(), 0);

        ducker.update(f32::NAN);
        assert_eq!(ducker.last_valid_p(), 0.7);
        assert_eq!(ducker.nonfinite_count(), 1);

        ducker.update(f32::INFINITY);
        assert_eq!(ducker.last_valid_p(), 0.7);
        assert_eq!(ducker.nonfinite_count(), 2);

        ducker.update(0.2);
        assert_eq!(ducker.last_valid_p(), 0.2);
        assert_eq!(ducker.nonfinite_count(), 2);
    }

    // Gate 11: Concurrent stress test
    #[test]
    fn test_concurrent_read_write() {
        use std::sync::Arc;
        use std::thread;

        let bus = Arc::new(ControlBus::new(48000.0));
        let mut readers = vec![];

        let read_flags = Arc::new(std::sync::atomic::AtomicBool::new(true));

        for i in 0..4 {
            let bus_clone = bus.clone();
            let flags = read_flags.clone();
            readers.push(thread::spawn(move || {
                let mut reads = 0;
                while flags.load(std::sync::atomic::Ordering::Relaxed) {
                    let frame = bus_clone.read();
                    reads += 1;

                    // Verify internal consistency: duck_gain = p_speech + 1.0, snr_db = p_speech + 2.0
                    // except default frame where p_speech = 0.0, duck_gain = 1.0, snr_db = 0.0
                    if frame.snr_db != 0.0 {
                        assert_eq!(
                            frame.duck_gain,
                            frame.p_speech + 1.0,
                            "Torn frame detected!"
                        );
                        assert_eq!(frame.snr_db, frame.p_speech + 2.0, "Torn frame detected!");
                    }
                }
                println!("Thread {} performed {} reads", i, reads);
            }));
        }

        for i in 1..=10_000 {
            bus.publish(ControlFrame {
                p_speech: i as f32,
                duck_gain: (i + 1) as f32,
                voice_gate: i % 2 == 0,
                snr_db: (i + 2) as f32,
            });
        }

        read_flags.store(false, std::sync::atomic::Ordering::Relaxed);
        for t in readers {
            t.join().unwrap();
        }
    }

    // Gate 12: No staleness beyond one publish
    #[test]
    fn test_no_staleness() {
        let bus = ControlBus::new(48000.0);
        bus.publish(ControlFrame {
            p_speech: 999.0,
            duck_gain: 999.0,
            voice_gate: true,
            snr_db: 999.0,
        });
        let frame = bus.read();
        assert_eq!(frame.p_speech, 999.0);
        assert_eq!(frame.duck_gain, 999.0);
    }
}
