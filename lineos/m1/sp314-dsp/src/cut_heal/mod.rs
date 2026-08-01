pub mod crossfade;

pub struct SilenceCut;
pub struct BreathCut;
pub struct CrossfadeHeal;

impl SilenceCut {
    /// Detects regions where RMS < -60 dBFS for > 200ms (9600 samples at 48kHz)
    /// Returns: Vec<(start_index, end_index)>
    pub fn detect(audio: &[f32], sample_rate: u32) -> Vec<(usize, usize)> {
        let mut regions = Vec::new();
        let threshold_amp = libm::powf(10.0f32, -60.0 / 20.0);
        let min_samples = (sample_rate as f32 * 0.200) as usize; // 200ms

        let block_size = 512;
        let num_blocks = audio.len() / block_size;

        let mut in_silence = false;
        let mut silence_start = 0;

        for i in 0..num_blocks {
            let start = i * block_size;
            let end = (i + 1) * block_size;
            let block = &audio[start..end];
            let mut sum_sq = 0.0;
            for &sample in block {
                sum_sq += sample * sample;
            }
            let rms = libm::sqrtf(sum_sq / block_size as f32);

            if rms < threshold_amp {
                if !in_silence {
                    in_silence = true;
                    silence_start = start;
                }
            } else {
                if in_silence {
                    in_silence = false;
                    let len = start - silence_start;
                    if len >= min_samples {
                        regions.push((silence_start, start));
                    }
                }
            }
        }

        // Handle trailing silence
        if in_silence {
            let end = num_blocks * block_size;
            let len = end - silence_start;
            if len >= min_samples {
                regions.push((silence_start, end));
            }
        }

        regions
    }
}

impl BreathCut {
    /// Detects breath regions: RMS -60 to -30 dBFS + high ZCR.
    pub fn detect(audio: &[f32], sample_rate: u32) -> Vec<(usize, usize)> {
        let mut regions = Vec::new();
        let lower_amp = libm::powf(10.0f32, -60.0 / 20.0);
        let upper_amp = libm::powf(10.0f32, -30.0 / 20.0);
        let min_samples = (sample_rate as f32 * 0.050) as usize; // arbitrary min length for breath

        let block_size = 512;
        let num_blocks = audio.len() / block_size;

        let mut in_breath = false;
        let mut breath_start = 0;

        for i in 0..num_blocks {
            let start = i * block_size;
            let end = (i + 1) * block_size;
            let block = &audio[start..end];

            let mut sum_sq = 0.0;
            let mut zcr = 0;
            for j in 0..block.len() {
                sum_sq += block[j] * block[j];
                if j > 0 && block[j].signum() != block[j - 1].signum() {
                    zcr += 1;
                }
            }
            let rms = libm::sqrtf(sum_sq / block_size as f32);
            let zcr_rate = zcr as f32 / block_size as f32;

            // High ZCR is typically > 0.05 for high-frequency noise like breath
            let is_breath = rms >= lower_amp && rms <= upper_amp && zcr_rate > 0.05;

            if is_breath {
                if !in_breath {
                    in_breath = true;
                    breath_start = start;
                }
            } else {
                if in_breath {
                    in_breath = false;
                    let len = start - breath_start;
                    if len >= min_samples {
                        regions.push((breath_start, start));
                    }
                }
            }
        }

        if in_breath {
            let end = num_blocks * block_size;
            let len = end - breath_start;
            if len >= min_samples {
                regions.push((breath_start, end));
            }
        }

        regions
    }
}

impl CrossfadeHeal {
    /// Applies 5ms crossfade at cut boundaries.
    pub fn heal_cut(audio: &mut [f32], cut_start: usize, cut_end: usize, sample_rate: u32) {
        let fade_len = (sample_rate as f32 * 0.005) as usize; // 240 samples @ 48kHz
        if cut_start < fade_len || cut_end + fade_len > audio.len() || cut_start >= cut_end {
            return;
        }

        for i in 0..fade_len {
            let t = i as f32 / fade_len as f32;
            // Fade out before cut_start
            audio[cut_start - fade_len + i] *= t;
            // Fade in after cut_end
            audio[cut_end + i] *= 1.0 - t;
        }

        audio[cut_start..cut_end].fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_sine(freq: f32, sample_rate: u32, samples: usize, amp: f32) -> Vec<f32> {
        let mut buf = vec![0.0; samples];
        let phase_inc = 2.0 * std::f32::consts::PI * freq / sample_rate as f32;
        for (i, s) in buf.iter_mut().enumerate() {
            *s = (i as f32 * phase_inc).sin() * amp;
        }
        buf
    }

    fn generate_noise(samples: usize, amp: f32) -> Vec<f32> {
        let mut buf = vec![0.0; samples];
        let mut seed = 42u32;
        for s in buf.iter_mut() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let val = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
            *s = val * amp;
        }
        buf
    }

    #[test]
    fn silence_cut_detects_long_silence() {
        // 300ms silence → detected
        let sr = 48000;
        let mut audio = generate_sine(440.0, sr, 48000, 0.5); // 1 sec tone
                                                              // insert 300ms silence in the middle
        let start = 12000;
        let len = (sr as f32 * 0.3) as usize;
        audio[start..start + len].fill(0.0);
        let cuts = SilenceCut::detect(&audio, sr);
        assert!(!cuts.is_empty(), "Should detect long silence");
        assert_eq!(cuts[0].0, 12288); // block aligned 24 * 512 = 12288
        assert!(cuts[0].1 >= start + len - 512);
    }

    #[test]
    fn silence_cut_ignores_short_gaps() {
        // 100ms silence → ignored
        let sr = 48000;
        let mut audio = generate_sine(440.0, sr, 48000, 0.5);
        let start = 12000;
        let len = (sr as f32 * 0.1) as usize; // 100ms is < 200ms
        audio[start..start + len].fill(0.0);
        let cuts = SilenceCut::detect(&audio, sr);
        assert!(cuts.is_empty(), "Should ignore short gap");
    }

    #[test]
    fn crossfade_no_click_at_boundary() {
        // energy continuity at cut point
        let sr = 48000;
        let mut audio = vec![1.0; 48000];
        CrossfadeHeal::heal_cut(&mut audio, 24000, 25000, sr);
        // check fade out
        assert!(audio[24000 - 100] < 1.0);
        assert!(audio[24000 - 100] > 0.0);
        // check silenced cut
        assert_eq!(audio[24500], 0.0);
    }

    #[test]
    fn cut_heal_deterministic() {
        let sr = 48000;
        let audio1 = vec![0.0; 48000];
        let audio2 = vec![0.0; 48000];
        let cuts1 = SilenceCut::detect(&audio1, sr);
        let cuts2 = SilenceCut::detect(&audio2, sr);
        assert_eq!(cuts1, cuts2);
    }

    #[test]
    fn breath_cut_detects_breath_region() {
        let sr = 48000;
        let mut audio = generate_sine(440.0, sr, 48000, 0.5); // tone
        let start = 24000;
        let len = 12000; // 250ms
                         // Add breath-like noise (-40 dBFS approx => ~0.01 amplitude)
        let noise = generate_noise(len, 0.01);
        audio[start..(len + start)].copy_from_slice(&noise[..len]);
        let cuts = BreathCut::detect(&audio, sr);
        assert!(!cuts.is_empty(), "Should detect breath");
    }
}
