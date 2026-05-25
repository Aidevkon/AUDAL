use sp314_dsp::limiter::core::{BrickwallLimiter, LimiterConfig};

fn main() {
    let mut limiter = BrickwallLimiter::new(LimiterConfig { release_ms: 100.0, ceiling_db: -0.5 }, 48000);
    
    let mut left = vec![0.5f32; 1000];
    let mut right = vec![0.5f32; 1000];
    
    // Inject transient
    left[500] = 5.0;
    right[500] = 5.0;
    
    limiter.process_block(&mut left, &mut right);
    
    // The transient enters the buffer at 500.
    // The delay is 240 samples.
    // The transient exits at 500 + 240 = 740.
    // Let's check what happens exactly 240 samples before the transient exits.
    // When the transient enters at i=500, the sample exiting the buffer is the one that entered at i=260.
    // So the gain reduction will instantly drop at i=500, applying to the sample that entered at i=260 (which is now exiting).
    println!("Output at 499 (pre-transient): {}", left[499]);
    println!("Output at 500 (transient enters): {}", left[500]);
    println!("Output at 740 (transient exits): {}", left[740]);
}
