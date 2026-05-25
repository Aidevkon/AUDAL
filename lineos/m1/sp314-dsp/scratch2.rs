use sp314_dsp::limiter::core::{BrickwallLimiter, LimiterConfig};

fn main() {
    let lim_44 = BrickwallLimiter::new(LimiterConfig::default(), 44100);
    let lim_48 = BrickwallLimiter::new(LimiterConfig::default(), 48000);
    let lim_96 = BrickwallLimiter::new(LimiterConfig::default(), 96000);

    println!("44100 latency ms: {}", lim_44.lookahead_samples() as f32 / 44.1);
    println!("48000 latency ms: {}", lim_48.lookahead_samples() as f32 / 48.0);
    println!("96000 latency ms: {}", lim_96.lookahead_samples() as f32 / 96.0);
}
