/// Energy-based beat detector.
/// No ML — pure DSP (threshold on energy flux).
/// INV: deterministic — same input → same output.
pub struct BeatDetector {
    sample_rate: u32,
    hop_size: usize,    // 512 samples
    window_size: usize, // 1024 samples
}

impl BeatDetector {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            hop_size: 512,
            window_size: 1024,
        }
    }

    /// Returns (bpm, beats_ms, downbeats_ms, transients_ms)
    /// NOTE: `samples` is assumed to be a MONO mixdown buffer.
    pub fn analyze(&self, samples: &[f32]) -> (f32, Vec<u32>, Vec<u32>, Vec<u32>) {
        let n_samples = samples.len();
        if n_samples < self.window_size {
            return (0.0, vec![], vec![], vec![]);
        }

        // 2. Energy Flux
        let mut flux = Vec::with_capacity(n_samples / self.hop_size + 1);
        let mut prev_energy = 0.0_f32;
        let mut offset = 0;

        while offset + self.window_size <= n_samples {
            let chunk = &samples[offset..offset + self.window_size];
            let mut sum_sq = 0.0_f32;
            for &s in chunk {
                sum_sq += s * s;
            }
            let rms = libm::sqrtf(sum_sq / self.window_size as f32);
            let diff = rms - prev_energy;
            flux.push(if diff > 0.0 { diff } else { 0.0 });
            prev_energy = rms;
            offset += self.hop_size;
        }

        // 3. Onsets (Transients)
        let hop_rate = self.sample_rate as f32 / self.hop_size as f32;
        let slide_win = (1.5 * hop_rate) as usize;
        let slide_win = slide_win.max(5);
        let mut transients_ms = Vec::new();

        for i in 0..flux.len() {
            let start = i.saturating_sub(slide_win / 2);
            let end = (i + slide_win / 2).min(flux.len());
            let local_window = &flux[start..end];
            let n = local_window.len() as f32;

            let mut mean = 0.0_f32;
            for &f in local_window {
                mean += f;
            }
            mean /= n;

            let mut var = 0.0_f32;
            for &f in local_window {
                var += (f - mean) * (f - mean);
            }
            var /= n;
            let std_dev = libm::sqrtf(var);

            let threshold = mean + 1.5 * std_dev;

            let is_peak = if i > 0 && i < flux.len() - 1 {
                flux[i] > flux[i - 1] && flux[i] > flux[i + 1]
            } else {
                false
            };

            if is_peak && flux[i] > threshold && flux[i] > 1e-4 {
                let center_sample = i * self.hop_size + self.window_size / 2;
                let ms = (center_sample as f64 * 1000.0 / self.sample_rate as f64) as u32;
                transients_ms.push(ms);
            }
        }

        // 7. Fallback
        if transients_ms.len() < 2 {
            return (0.0, vec![], vec![], transients_ms);
        }

        // 4. BPM Estimation
        let mut iois = Vec::with_capacity(transients_ms.len() - 1);
        for i in 1..transients_ms.len() {
            iois.push(transients_ms[i] - transients_ms[i - 1]);
        }

        let mut histogram = [0u32; 1000]; // up to 10 seconds (1000 buckets of 10ms)
        for &ioi in &iois {
            let bucket = (ioi / 10) as usize;
            if bucket < histogram.len() {
                histogram[bucket] += 1;
            }
        }

        let mut best_bucket = 0;
        let mut best_count = 0;

        // Bucket size is 10ms, peak_ioi_ms = bucket * 10
        // We iterate and keep the highest count.
        for bucket in 1..histogram.len() {
            let count = histogram[bucket];
            let bpm = 60000.0 / (bucket * 10) as f32;
            if bpm >= 60.0 && bpm <= 200.0 {
                if count > best_count {
                    best_count = count;
                    best_bucket = bucket;
                }
            }
        }

        if best_bucket == 0 {
            return (0.0, vec![], vec![], transients_ms);
        }

        let peak_ioi_ms = (best_bucket * 10) as f32;
        let bpm = 60000.0 / peak_ioi_ms;

        // 5. Beat Grid Construction
        let phase_0 = transients_ms[0];
        let step_ms = peak_ioi_ms as u32;
        let total_duration_ms = (n_samples as f64 * 1000.0 / self.sample_rate as f64) as u32;

        let mut beats_ms = Vec::new();
        let mut current = phase_0;
        while current < total_duration_ms {
            beats_ms.push(current);
            current += step_ms;
        }

        // 6. Downbeats
        let mut downbeats_ms = Vec::new();
        for (i, &b) in beats_ms.iter().enumerate() {
            if i % 4 == 0 {
                downbeats_ms.push(b);
            }
        }

        (bpm, beats_ms, downbeats_ms, transients_ms)
    }
}
