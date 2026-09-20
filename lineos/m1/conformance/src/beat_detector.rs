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
        for (bucket, &count) in histogram.iter().enumerate().skip(1) {
            let bpm = 60000.0 / (bucket * 10) as f32;
            if (60.0..=200.0).contains(&bpm) && count > best_count {
                best_count = count;
                best_bucket = bucket;
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

// Streaming (chunk-fed) port of BeatDetector — same energy-flux →
// IOI-histogram → tempo algorithm, no new DSP math, adapted for
// O(1)-memory incremental use (feed_chunk/finish, matching
// LufsMeter/TruePeakMeter's established pattern).
//
// KEY DESIGN NOTE: the original's onset/peak detection uses a
// CENTERED sliding window (flux[i - slide_win/2 .. i + slide_win/2])
// to decide if position i is a transient — meaning you need
// ~slide_win/2 FUTURE flux values before you can finalize position
// i. This is a real lookahead requirement, not an implementation
// shortcut: streaming here means "bounded memory, finalized with a
// fixed delay," not "zero latency." For an offline streaming
// render (not a live/real-time path), a ~0.75s fixed delay is
// completely acceptable.

use std::collections::VecDeque;

pub struct StreamingBeatDetector {
    sample_rate: u32,
    hop_size: usize,
    window_size: usize,
    slide_win: usize,

    // Windowing state (persists across feed_chunk calls)
    sample_carry: Vec<f32>, // leftover samples not yet forming a full window
    prev_energy: f32,
    total_samples_seen: usize,

    // Flux history, holding enough entries to resolve the centered
    // window with lookahead. Entries older than slide_win/2 from the
    // newest one are finalized (peak-checked) and popped.
    flux_history: VecDeque<f32>,
    flux_index_of_front: usize, // absolute flux-index of flux_history[0]

    // Accumulated results (mirrors analyze()'s return shape exactly,
    // for a direct oracle-test comparison)
    transients_ms: Vec<u32>,
    histogram: [u32; 1000],
    last_transient_ms: Option<u32>,
    first_transient_ms: Option<u32>,
}

impl StreamingBeatDetector {
    pub fn new(sample_rate: u32) -> Self {
        let hop_size = 512;
        let hop_rate = sample_rate as f32 / hop_size as f32;
        let slide_win = ((1.5 * hop_rate) as usize).max(5);
        Self {
            sample_rate,
            hop_size,
            window_size: 1024,
            slide_win,
            sample_carry: Vec::new(),
            prev_energy: 0.0,
            total_samples_seen: 0,
            flux_history: VecDeque::new(),
            flux_index_of_front: 0,
            transients_ms: Vec::new(),
            histogram: [0u32; 1000],
            last_transient_ms: None,
            first_transient_ms: None,
        }
    }

    /// Feed the next chunk of MONO samples (caller must mix down
    /// stereo to mono before calling, same assumption as the
    /// original analyze()).
    pub fn feed_chunk(&mut self, samples: &[f32]) {
        self.total_samples_seen += samples.len();
        self.sample_carry.extend_from_slice(samples);

        let mut offset = 0;
        let n_samples = self.sample_carry.len();

        while offset + self.window_size <= n_samples {
            let chunk = &self.sample_carry[offset..offset + self.window_size];
            let mut sum_sq = 0.0_f32;
            for &s in chunk {
                sum_sq += s * s;
            }
            let rms = libm::sqrtf(sum_sq / self.window_size as f32);
            let diff = rms - self.prev_energy;
            self.flux_history
                .push_back(if diff > 0.0 { diff } else { 0.0 });
            self.prev_energy = rms;
            offset += self.hop_size;

            let lookahead = self.slide_win / 2;
            let total_flux = self.flux_index_of_front + self.flux_history.len();

            if total_flux > lookahead {
                let c = total_flux - 1 - lookahead;

                let start_abs = c.saturating_sub(lookahead);
                let end_abs = c + lookahead + 1;

                let start_idx = start_abs - self.flux_index_of_front;
                let end_idx = end_abs - self.flux_index_of_front;

                let n = (end_idx - start_idx) as f32;
                let mut mean = 0.0_f32;
                for i in start_idx..end_idx {
                    mean += self.flux_history[i];
                }
                mean /= n;

                let mut var = 0.0_f32;
                for i in start_idx..end_idx {
                    let f = self.flux_history[i];
                    var += (f - mean) * (f - mean);
                }
                var /= n;
                let std_dev = libm::sqrtf(var);
                let threshold = mean + 1.5 * std_dev;

                let c_idx = c - self.flux_index_of_front;
                let center_val = self.flux_history[c_idx];

                // Boundary logic:
                // Streaming doesn't know the FINAL `flux.len()` until `finish()`.
                // But during `feed_chunk`, we only resolve element `c` when we have `lookahead` elements AFTER it.
                // Since `slide_win.max(5)` means `lookahead` >= 2, we are guaranteed to have at least 2 elements
                // after `c` in the current stream. Thus, `c` is NEVER the very last element of the stream during `feed_chunk`.
                // Therefore, `c < flux.len() - 1` is always TRUE here. We only need to check `c > 0`.
                let is_peak = if c > 0 {
                    center_val > self.flux_history[c_idx - 1]
                        && center_val > self.flux_history[c_idx + 1]
                } else {
                    false
                };

                if is_peak && center_val > threshold && center_val > 1e-4 {
                    let center_sample = c * self.hop_size + self.window_size / 2;
                    let ms = (center_sample as f64 * 1000.0 / self.sample_rate as f64) as u32;
                    self.transients_ms.push(ms);

                    if self.first_transient_ms.is_none() {
                        self.first_transient_ms = Some(ms);
                    }

                    if let Some(last) = self.last_transient_ms {
                        let ioi = ms - last;
                        let bucket = (ioi / 10) as usize;
                        if bucket < self.histogram.len() {
                            self.histogram[bucket] += 1;
                        }
                    }
                    self.last_transient_ms = Some(ms);
                }

                // Pop elements that are no longer needed.
                // The next element to be finalized will be `c + 1`.
                // Its lookbehind will start at `(c + 1).saturating_sub(lookahead)`.
                let next_start_abs = (c + 1).saturating_sub(lookahead);
                while self.flux_index_of_front < next_start_abs {
                    self.flux_history.pop_front();
                    self.flux_index_of_front += 1;
                }
            }
        }

        self.sample_carry.drain(..offset);
    }

    /// Consume the detector and resolve final BPM + beat grid.
    /// Mirrors analyze()'s tail exactly (steps 4-6).
    pub fn finish(mut self) -> (f32, Vec<u32>, Vec<u32>, Vec<u32>) {
        let lookahead = self.slide_win / 2;
        let total_flux = self.flux_index_of_front + self.flux_history.len();
        let start_c = total_flux.saturating_sub(lookahead);

        for c in start_c..total_flux {
            let start_abs = c.saturating_sub(lookahead);
            let end_abs = (c + lookahead + 1).min(total_flux);

            let start_idx = start_abs - self.flux_index_of_front;
            let end_idx = end_abs - self.flux_index_of_front;

            let n = (end_idx - start_idx) as f32;
            let mut mean = 0.0_f32;
            for i in start_idx..end_idx {
                mean += self.flux_history[i];
            }
            mean /= n;

            let mut var = 0.0_f32;
            for i in start_idx..end_idx {
                let f = self.flux_history[i];
                var += (f - mean) * (f - mean);
            }
            var /= n;
            let std_dev = libm::sqrtf(var);
            let threshold = mean + 1.5 * std_dev;

            let c_idx = c - self.flux_index_of_front;
            let center_val = self.flux_history[c_idx];

            let is_peak = if c > 0 && c < total_flux - 1 {
                center_val > self.flux_history[c_idx - 1]
                    && center_val > self.flux_history[c_idx + 1]
            } else {
                false
            };

            if is_peak && center_val > threshold && center_val > 1e-4 {
                let center_sample = c * self.hop_size + self.window_size / 2;
                let ms = (center_sample as f64 * 1000.0 / self.sample_rate as f64) as u32;
                self.transients_ms.push(ms);

                if self.first_transient_ms.is_none() {
                    self.first_transient_ms = Some(ms);
                }

                if let Some(last) = self.last_transient_ms {
                    let ioi = ms - last;
                    let bucket = (ioi / 10) as usize;
                    if bucket < self.histogram.len() {
                        self.histogram[bucket] += 1;
                    }
                }
                self.last_transient_ms = Some(ms);
            }
        }

        if self.transients_ms.len() < 2 {
            return (0.0, vec![], vec![], self.transients_ms);
        }

        let mut best_bucket = 0;
        let mut best_count = 0;

        for (bucket, &count) in self.histogram.iter().enumerate().skip(1) {
            let bpm = 60000.0 / (bucket * 10) as f32;
            if (60.0..=200.0).contains(&bpm) && count > best_count {
                best_count = count;
                best_bucket = bucket;
            }
        }

        if best_bucket == 0 {
            return (0.0, vec![], vec![], self.transients_ms);
        }

        let peak_ioi_ms = (best_bucket * 10) as f32;
        let bpm = 60000.0 / peak_ioi_ms;

        let phase_0 = self.transients_ms[0];
        let step_ms = peak_ioi_ms as u32;
        let total_duration_ms =
            (self.total_samples_seen as f64 * 1000.0 / self.sample_rate as f64) as u32;

        let mut beats_ms = Vec::new();
        let mut current = phase_0;
        while current < total_duration_ms {
            beats_ms.push(current);
            current += step_ms;
        }

        let mut downbeats_ms = Vec::new();
        for (i, &b) in beats_ms.iter().enumerate() {
            if i % 4 == 0 {
                downbeats_ms.push(b);
            }
        }

        (bpm, beats_ms, downbeats_ms, self.transients_ms)
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;

    fn generate_click_track(sample_rate: u32, bpm: f32, duration_secs: f32) -> Vec<f32> {
        let n = (sample_rate as f32 * duration_secs) as usize;
        let mut buf = vec![0.0f32; n];
        let interval_samples = (60.0 / bpm * sample_rate as f32) as usize;
        let mut pos = 0;
        while pos + 20 < n {
            for i in 0..20 {
                buf[pos + i] = 0.9 * (1.0 - i as f32 / 20.0); // sharp decaying click
            }
            pos += interval_samples;
        }
        buf
    }

    #[test]
    fn streaming_matches_whole_buffer_reference() {
        let sr = 48_000;
        let audio = generate_click_track(sr, 120.0, 8.0);

        let whole_buffer_result = BeatDetector::new(sr).analyze(&audio);

        let mut streaming = StreamingBeatDetector::new(sr);
        for chunk in audio.chunks(2048) {
            streaming.feed_chunk(chunk);
        }
        let streaming_result = streaming.finish();

        eprintln!(
            "Oracle BPM: whole={}, streaming={}",
            whole_buffer_result.0, streaming_result.0
        );
        assert!(
            (whole_buffer_result.0 - streaming_result.0).abs() < 1.0,
            "BPM mismatch: whole-buffer={}, streaming={}",
            whole_buffer_result.0,
            streaming_result.0
        );
        assert_eq!(
            whole_buffer_result.1.len(),
            streaming_result.1.len(),
            "beats_ms count mismatch"
        );
        assert_eq!(
            whole_buffer_result.3.len(),
            streaming_result.3.len(),
            "transients_ms count mismatch"
        );
    }

    #[test]
    fn streaming_handles_short_audio_gracefully() {
        let sr = 48_000;
        let mut streaming = StreamingBeatDetector::new(sr);
        streaming.feed_chunk(&vec![0.0f32; 100]); // way too short
        let result = streaming.finish();
        assert_eq!(result.0, 0.0);
        assert!(result.1.is_empty());
    }
}
