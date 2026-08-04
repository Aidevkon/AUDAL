use std::sync::atomic::{AtomicUsize, AtomicU32, AtomicBool, Ordering, fence};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlFrame {
    pub p_speech: f32,     // 0..=1, from the micro-VAD
    pub duck_gain: f32,    // linear gain for the Music bus, 0..=1
    pub voice_gate: bool,  // Voice bus open/closed
    pub snr_db: f32,
}

/// A lock-free Single-Writer Multiple-Reader (SWMR) bus using a Seqlock.
/// We use per-field atomics so that racy reads are well-defined by the Rust
/// memory model (avoiding UB from reading an UnsafeCell while a write is in flight).
/// The sequence counter remains strictly necessary for consistency: if a reader
/// reads `p_speech` then the writer updates all fields, the reader's subsequent 
/// read of `duck_gain` would belong to a new frame. The sequence counter ensures
/// the entire read snapshot belongs to a single COMPLETE frame.
pub struct ControlBus {
    seq: AtomicUsize,
    p_speech: AtomicU32,
    duck_gain: AtomicU32,
    voice_gate: AtomicBool,
    snr_db: AtomicU32,
}

// Safe because reads/writes are synchronized via the sequence lock and atomics
unsafe impl Sync for ControlBus {}
unsafe impl Send for ControlBus {}

impl ControlBus {
    pub fn new(_sample_rate: f32) -> Self {
        Self {
            seq: AtomicUsize::new(0),
            p_speech: AtomicU32::new(0.0f32.to_bits()),
            duck_gain: AtomicU32::new(1.0f32.to_bits()),
            voice_gate: AtomicBool::new(false),
            snr_db: AtomicU32::new(0.0f32.to_bits()),
        }
    }

    pub fn publish(&self, frame: ControlFrame) {
        let seq = self.seq.load(Ordering::Relaxed);
        // Odd sequence number indicates a write is in progress
        self.seq.store(seq + 1, Ordering::Relaxed);
        fence(Ordering::Release);

        self.p_speech.store(frame.p_speech.to_bits(), Ordering::Relaxed);
        self.duck_gain.store(frame.duck_gain.to_bits(), Ordering::Relaxed);
        self.voice_gate.store(frame.voice_gate, Ordering::Relaxed);
        self.snr_db.store(frame.snr_db.to_bits(), Ordering::Relaxed);

        fence(Ordering::Release);
        // Even sequence number indicates write is complete
        self.seq.store(seq + 2, Ordering::Relaxed);
    }

    pub fn read(&self) -> ControlFrame {
        loop {
            let seq1 = self.seq.load(Ordering::Acquire);
            if seq1 & 1 == 1 {
                // Write in progress, spin wait
                std::hint::spin_loop();
                continue;
            }
            
            // Read the individual atomics
            let p_speech = f32::from_bits(self.p_speech.load(Ordering::Relaxed));
            let duck_gain = f32::from_bits(self.duck_gain.load(Ordering::Relaxed));
            let voice_gate = self.voice_gate.load(Ordering::Relaxed);
            let snr_db = f32::from_bits(self.snr_db.load(Ordering::Relaxed));
            
            fence(Ordering::Acquire);
            
            // Check if a write started or completed while we were reading
            let seq2 = self.seq.load(Ordering::Acquire);
            if seq1 == seq2 {
                return ControlFrame {
                    p_speech,
                    duck_gain,
                    voice_gate,
                    snr_db,
                };
            }
        }
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
        let frame_rate = sample_rate / 512.0; 
        
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
        
        let floor = 10.0f32.powf(-12.0 / 20.0);
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
        let frame_rate = sample_rate / 512.0;
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
        for _ in 0..100 { ducker.update(1.0); }

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
        assert!(attack_ms >= 20.0 && attack_ms <= 60.0, "Attack ms outside gate");
    }

    // Gate 4: release ms
    #[test]
    fn test_ducker_release_ms() {
        let (_, release_ms) = run_ballistics_test();
        println!("Measured Release: {:.2} ms", release_ms);
        assert!(release_ms >= 300.0 && release_ms <= 800.0, "Release ms outside gate");
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
        for p in [0.0, 1.0, 0.5, 1.5, -0.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let gain = ducker.update(p);
            assert!(gain >= 0.0 && gain <= 1.0, "Bounds check failed: gain = {}", gain);
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
        for _ in 0..100 { ducker.update(1.0); }
        let pre_nan = ducker.update(1.0);
        
        // feed NaN for 100 frames
        for _ in 0..100 {
            let gain = ducker.update(f32::NAN);
            assert_eq!(gain.to_bits(), pre_nan.to_bits(), "duck_gain did not perfectly freeze on NaN");
        }
        
        // feed 0.0 and confirm release
        let release_gain = ducker.update(0.0);
        assert!(release_gain > pre_nan, "did not begin releasing after NaN freeze");
    }

    // Gate 9: Feed Inf and -Inf
    #[test]
    fn test_ducker_inf_freeze() {
        let mut ducker = Ducker::new(48000.0);
        for _ in 0..100 { ducker.update(1.0); }
        let pre_inf = ducker.update(1.0);
        
        for _ in 0..100 {
            let gain = ducker.update(f32::INFINITY);
            assert_eq!(gain.to_bits(), pre_inf.to_bits(), "duck_gain did not perfectly freeze on INFINITY");
        }
        for _ in 0..100 {
            let gain = ducker.update(f32::NEG_INFINITY);
            assert_eq!(gain.to_bits(), pre_inf.to_bits(), "duck_gain did not perfectly freeze on NEG_INFINITY");
        }
        
        let release_gain = ducker.update(0.0);
        assert!(release_gain > pre_inf, "did not begin releasing after Inf freeze");
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
}
