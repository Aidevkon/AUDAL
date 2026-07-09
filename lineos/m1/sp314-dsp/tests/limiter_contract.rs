// tests/limiter_contract.rs

use approx::assert_abs_diff_eq;
use sp314_dsp::limiter::delay::RingBuffer;
use sp314_dsp::limiter::{BrickwallLimiter, LimiterConfig, PeakFollower, DEFAULT_CEILING_LINEAR};

#[test]
fn limiter_ring_buffer_delays_by_n_samples() {
    let mut rb: RingBuffer = RingBuffer::new(240);
    let mut out = Vec::new();
    // Feed impulse
    out.push(rb.push_and_pop(1.0));
    // Feed 0.0s
    for _ in 0..(240 + 10) {
        out.push(rb.push_and_pop(0.0));
    }

    // Check output
    for i in 0..out.len() {
        if i == 240 {
            assert_eq!(out[i], 1.0, "Impulse should appear at 240");
        } else {
            assert_eq!(out[i], 0.0, "All other outputs should be 0.0");
        }
    }
}

#[test]
fn limiter_ring_buffer_wraps_correctly() {
    let mut rb: RingBuffer = RingBuffer::new(240);
    for i in 0..(3 * 240) {
        let val = (i as f32) + 1.0;
        let delayed = rb.push_and_pop(val);
        if i >= 240 {
            assert_eq!(delayed, val - 240_f32);
        } else {
            assert_eq!(delayed, 0.0);
        }
    }
}

#[test]
fn limiter_peak_follower_linear_attack_ramp() {
    let mut follower = PeakFollower::new(100.0, 100.0, DEFAULT_CEILING_LINEAR, 48000, 240);
    // Feed 1.0 peak
    let gr = follower.process(1.0);
    // Because it ramps over 240 samples, the first sample's envelope is 1.0/240
    // which is below the ceiling (0.9441), so GR is 1.0
    assert_eq!(gr, 1.0, "Attack must ramp linearly, no immediate ducking");

    // After 240 samples of processing 1.0, it should reach the peak
    for _ in 1..240 {
        follower.process(1.0);
    }
    let gr_final = follower.process(1.0);
    assert_abs_diff_eq!(gr_final, DEFAULT_CEILING_LINEAR, epsilon = 1e-4);
}

#[test]
fn limiter_peak_follower_stereo_linked() {
    let mut limiter = BrickwallLimiter::new(LimiterConfig::default(), 48000);
    let mut left = 0.0_f32;
    let mut right = 1.0_f32;
    limiter.process(&mut left, &mut right);
    // The lookahead processes the peak now, but the audio out is from the delay buffer (which is 0.0).
    // Let's flush the delay buffer
    let mut out_l = 0.0;
    let mut out_r = 0.0;
    for _ in 0..240 {
        out_l = 0.0;
        out_r = 0.0;
        limiter.process(&mut out_l, &mut out_r);
    }
    // At exactly 240, the impulse (0, 1) exits.
    assert_eq!(out_l, 0.0);
    assert_abs_diff_eq!(out_r, DEFAULT_CEILING_LINEAR, epsilon = 1e-4);

    // Now test stereo link: left channel has a smaller signal, but should be reduced by the same amount.
    limiter.reset();
    let mut left = 0.5_f32;
    let mut right = 1.0_f32;
    limiter.process(&mut left, &mut right);
    for _ in 0..240 {
        out_l = 0.0;
        out_r = 0.0;
        limiter.process(&mut out_l, &mut out_r);
    }
    // The gain reduction is determined by the max (which is 1.0).
    // The GR is 0.9441. So left should be 0.5 * 0.9441.
    assert_abs_diff_eq!(out_l, 0.5 * DEFAULT_CEILING_LINEAR, epsilon = 1e-4);
}

#[test]
fn limiter_ceiling_never_exceeded() {
    let mut limiter = BrickwallLimiter::new(LimiterConfig::default(), 48000);
    // Generate some noise or loud sine waves
    let mut max_out = 0.0_f32;
    for i in 0..10000 {
        let t = i as f32 / 48000.0;
        // Loud sine wave with amplitude 2.0 (well above ceiling)
        let mut l = 2.0 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin();
        let mut r = 2.0 * (2.0 * std::f32::consts::PI * 500.0 * t).cos();
        limiter.process(&mut l, &mut r);
        if l.abs() > max_out {
            max_out = l.abs();
        }
        if r.abs() > max_out {
            max_out = r.abs();
        }
    }
    assert!(
        max_out <= DEFAULT_CEILING_LINEAR + 1e-5,
        "Output peak {} exceeded ceiling {}",
        max_out,
        DEFAULT_CEILING_LINEAR
    );
}

#[test]
fn limiter_decay_floor_prevents_pumping() {
    let mut follower = PeakFollower::new(100.0, 100.0, DEFAULT_CEILING_LINEAR, 48000, 240);
    follower.process(1.0); // loud transient

    let mut gr = 0.0;
    // Feed silence, should eventually snap to 1.0
    for _ in 0..96000 {
        // 2 seconds
        gr = follower.process(0.0);
        if (gr - 1.0).abs() < 1e-7 {
            break;
        }
    }
    assert_abs_diff_eq!(gr, 1.0, epsilon = 1e-7);
}

#[test]
fn limiter_no_denormals_after_silence() {
    let mut follower = PeakFollower::new(100.0, 100.0, DEFAULT_CEILING_LINEAR, 48000, 240);
    follower.process(1.0);
    for _ in 0..100000 {
        follower.process(0.0);
    }
    // Envelope should be flushed to exactly 0.0, not a subnormal.
    // We can't access envelope directly, but we can verify GR is exactly 1.0
    let gr = follower.process(0.0);
    assert_eq!(gr, 1.0, "GR should be exactly 1.0 (no denormals)");
}

#[test]
fn limiter_deterministic() {
    let mut limiter1 = BrickwallLimiter::new(LimiterConfig::default(), 48000);
    let mut limiter2 = BrickwallLimiter::new(LimiterConfig::default(), 48000);

    let mut left1 = vec![0.5; 1000];
    let mut right1 = vec![1.5; 1000];
    let mut left2 = left1.clone();
    let mut right2 = right1.clone();

    limiter1.process_block(&mut left1, &mut right1);
    limiter2.process_block(&mut left2, &mut right2);

    assert_eq!(left1, left2);
    assert_eq!(right1, right2);
}

#[test]
fn limiter_reset_clears_state() {
    let mut limiter = BrickwallLimiter::new(LimiterConfig::default(), 48000);

    let mut left1 = vec![0.5; 1000];
    let mut right1 = vec![1.5; 1000];
    limiter.process_block(&mut left1, &mut right1);

    limiter.reset();

    let mut left2 = vec![0.5; 1000];
    let mut right2 = vec![1.5; 1000];
    limiter.process_block(&mut left2, &mut right2);

    assert_eq!(left1, left2);
    assert_eq!(right1, right2);
}

#[test]
fn limiter_lookahead_alignment() {
    let mut limiter = BrickwallLimiter::new(LimiterConfig::default(), 48000);

    // Feed silence for 240 samples
    for _ in 0..240 {
        let mut l = 0.0;
        let mut r = 0.0;
        limiter.process(&mut l, &mut r);
        assert_eq!(l, 0.0);
    }

    // Now feed a loud impulse at current time (t=0)
    let mut l = 1.0;
    let mut r = 1.0;
    limiter.process(&mut l, &mut r);
    assert_eq!(l, 0.0, "Impulse should not appear yet");

    // It should cause gain reduction immediately for samples that are in the delay buffer,
    // wait, the peak is now at the start of the delay buffer. The output is the delayed sample (which is 0).
    // If the delayed sample was non-zero, it would be reduced.
    // Let's do a better test: Feed DC 0.5, then a peak of 2.0.
    limiter.reset();
    for _ in 0..240 {
        let mut l = 0.5;
        let mut r = 0.5;
        limiter.process(&mut l, &mut r);
        // The first 240 outputs are 0.0 because the delay buffer is initially 0
    }

    // Now the delay buffer is full of 0.5s.
    // If we feed 0.5, the output is 0.5.
    let mut l = 0.5;
    let mut r = 0.5;
    limiter.process(&mut l, &mut r);
    assert_eq!(l, 0.5);

    // Now feed a loud peak of 2.0
    let mut l = 2.0;
    let mut r = 2.0;
    limiter.process(&mut l, &mut r);
    // The output here is the sample from 240 samples ago, which was 0.5.
    // Because the TRUE PEAK is now 2.0, the gain reduction ramps up linearly.
    // The initial ducking should be very small or zero, avoiding pre-clicks.
    assert_abs_diff_eq!(l, 0.5, epsilon = 1e-4);
}
