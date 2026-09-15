use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;

pub const PHI1_FRAME_SAMPLES_16K: usize = 160;
pub const PHI1_CONTEXT_FRAMES: usize = 51;

pub const PHI2_PCEN_S: f32 = 0.025;
pub const PHI2_PCEN_ALPHA: f32 = 0.98;
pub const PHI2_PCEN_DELTA: f32 = 2.0;
pub const PHI2_PCEN_R: f32 = 0.5;
pub const PHI2_PCEN_EPS: f32 = 1e-6;

pub struct Phi1Norm {
    pub mean: f32,
    pub std: f32,
}

pub struct Phi1MelFrontend {
    w: Vec<f32>,
    fb: Vec<Vec<f32>>,
    planner: FftPlanner<f32>,
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10_f32.powf(mel / 2595.0) - 1.0)
}

impl Phi1MelFrontend {
    pub fn new() -> Self {
        let n_fft = 400;
        let n_mels = 64;
        let sample_rate = 16000.0;

        let mut w = vec![0.0f32; n_fft];
        for i in 0..n_fft {
            w[i] = 0.5 - 0.5 * (2.0 * PI * i as f32 / n_fft as f32).cos();
        }

        let mel_min = hz_to_mel(0.0);
        let mel_max = hz_to_mel(8000.0);
        let mut mel_points = vec![0.0f32; n_mels + 2];
        for i in 0..(n_mels + 2) {
            mel_points[i] = mel_min + (i as f32) * (mel_max - mel_min) / (n_mels as f32 + 1.0);
        }
        let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

        let n_freqs = n_fft / 2 + 1;
        let mut fb = vec![vec![0.0f32; n_mels]; n_freqs];
        for f in 0..n_freqs {
            let freq = (f as f32) * sample_rate / n_fft as f32;
            for m in 0..n_mels {
                let left = hz_points[m];
                let center = hz_points[m + 1];
                let right = hz_points[m + 2];

                if freq > left && freq < center {
                    fb[f][m] = (freq - left) / (center - left);
                } else if freq >= center && freq < right {
                    fb[f][m] = (right - freq) / (right - center);
                }
            }
        }

        Self {
            w,
            fb,
            planner: FftPlanner::<f32>::new(),
        }
    }

    pub fn compute_all(&mut self, mono_48k: &[f32]) -> Vec<[f32; 64]> {
        // F-103/F-104: μία πηγή — βλ. stream_core.rs:7.
        let orig_sr = lineos_types::analysis::ANALYSIS_SAMPLE_RATE as f32;
        let fc = 7600.0 / orig_sr;
        let m = 120;
        let mut h = vec![0.0f32; m + 1];
        let mut sum_h = 0.0;
        for n in 0..=m {
            let n_f32 = n as f32;
            let m_f32 = m as f32;
            let hamming = 0.54 - 0.46 * (2.0 * PI * n_f32 / m_f32).cos();
            let x_val = n_f32 - m_f32 / 2.0;
            let sinc = if x_val.abs() < 1e-6 {
                2.0 * fc
            } else {
                (2.0 * PI * fc * x_val).sin() / (PI * x_val)
            };
            h[n] = sinc * hamming;
            sum_h += h[n];
        }
        for n in 0..=m {
            h[n] /= sum_h;
        }

        let len = mono_48k.len();
        let mut filtered = vec![0.0f32; len];
        let half_m = m / 2;
        for i in 0..len {
            let mut acc = 0.0;
            for j in 0..=m {
                let mut src_idx = i as isize + j as isize - half_m as isize;
                if src_idx < 0 { src_idx = 0; }
                if src_idx >= len as isize { src_idx = len as isize - 1; }
                acc += h[j] * mono_48k[src_idx as usize];
            }
            filtered[i] = acc;
        }

        let x: Vec<f32> = filtered.into_iter().step_by(3).collect();

        let n = x.len();
        let pad = 200;
        let padded_len = pad + n + pad;
        let mut padded = vec![0.0f32; padded_len];
        padded[pad..pad + n].copy_from_slice(&x);
        for i in 0..pad {
            if i + 1 < n {
                padded[pad - 1 - i] = x[i + 1];
            }
            if n >= 2 + i {
                padded[pad + n + i] = x[n - 2 - i];
            }
        }

        let n_fft = 400;
        let hop = 160;
        let n_freqs = n_fft / 2 + 1;
        let fft = self.planner.plan_fft_forward(n_fft);

        let mut mel_matrix = Vec::new();
        let mut k = 0;
        loop {
            let start = k * hop;
            if start + n_fft > padded_len {
                break;
            }

            let frame = &padded[start..start + n_fft];
            let mut buffer: Vec<Complex<f32>> = frame.iter().zip(self.w.iter())
                .map(|(&xv, &win)| Complex { re: xv * win, im: 0.0 })
                .collect();
            
            fft.process(&mut buffer);

            let mut power = vec![0.0f32; n_freqs];
            for i in 0..n_freqs {
                power[i] = buffer[i].re * buffer[i].re + buffer[i].im * buffer[i].im;
            }

            let mut mel_frame = [0.0f32; 64];
            for m_idx in 0..64 {
                let mut energy = 0.0f32;
                for f in 0..n_freqs {
                    energy += self.fb[f][m_idx] * power[f];
                }
                mel_frame[m_idx] = (energy + 1e-9).ln();
            }
            mel_matrix.push(mel_frame);
            k += 1;
        }

        mel_matrix
    }




    pub fn decimate_only(&self, mono_48k: &[f32]) -> Vec<f32> {
        // F-103/F-104: μία πηγή — βλ. stream_core.rs:7.
        let orig_sr = lineos_types::analysis::ANALYSIS_SAMPLE_RATE as f32;
        let fc = 7600.0 / orig_sr;
        let m = 120;
        let mut h = vec![0.0f32; m + 1];
        let mut sum_h = 0.0;
        for n in 0..=m {
            let n_f32 = n as f32;
            let m_f32 = m as f32;
            let hamming = 0.54 - 0.46 * (2.0 * std::f32::consts::PI * n_f32 / m_f32).cos();
            let x_val = n_f32 - m_f32 / 2.0;
            let sinc = if x_val.abs() < 1e-6 {
                2.0 * fc
            } else {
                (2.0 * std::f32::consts::PI * fc * x_val).sin() / (std::f32::consts::PI * x_val)
            };
            h[n] = sinc * hamming;
            sum_h += h[n];
        }
        for n in 0..=m {
            h[n] /= sum_h;
        }

        let len = mono_48k.len();
        let mut filtered = vec![0.0f32; len];
        let half_m = m / 2;
        for i in 0..len {
            let mut acc = 0.0;
            for j in 0..=m {
                let mut src_idx = i as isize + j as isize - half_m as isize;
                if src_idx < 0 { src_idx = 0; }
                if src_idx >= len as isize { src_idx = len as isize - 1; }
                acc += h[j] * mono_48k[src_idx as usize];
            }
            filtered[i] = acc;
        }

        filtered.into_iter().step_by(3).collect()
    }

    pub fn compute_all_power(&mut self, mono_48k: &[f32]) -> Vec<[f32; 64]> {
        // F-103/F-104: μία πηγή — βλ. stream_core.rs:7.
        let orig_sr = lineos_types::analysis::ANALYSIS_SAMPLE_RATE as f32;
        let fc = 7600.0 / orig_sr;
        let m = 120;
        let mut h = vec![0.0f32; m + 1];
        let mut sum_h = 0.0;
        for n in 0..=m {
            let n_f32 = n as f32;
            let m_f32 = m as f32;
            let hamming = 0.54 - 0.46 * (2.0 * PI * n_f32 / m_f32).cos();
            let x_val = n_f32 - m_f32 / 2.0;
            let sinc = if x_val.abs() < 1e-6 {
                2.0 * fc
            } else {
                (2.0 * PI * fc * x_val).sin() / (PI * x_val)
            };
            h[n] = sinc * hamming;
            sum_h += h[n];
        }
        for n in 0..=m {
            h[n] /= sum_h;
        }

        let len = mono_48k.len();
        let mut filtered = vec![0.0f32; len];
        let half_m = m / 2;
        for i in 0..len {
            let mut acc = 0.0;
            for j in 0..=m {
                let mut src_idx = i as isize + j as isize - half_m as isize;
                if src_idx < 0 { src_idx = 0; }
                if src_idx >= len as isize { src_idx = len as isize - 1; }
                acc += h[j] * mono_48k[src_idx as usize];
            }
            filtered[i] = acc;
        }

        let x: Vec<f32> = filtered.into_iter().step_by(3).collect();

        let n = x.len();
        let pad = 200;
        let padded_len = pad + n + pad;
        let mut padded = vec![0.0f32; padded_len];
        padded[pad..pad + n].copy_from_slice(&x);
        for i in 0..pad {
            if i + 1 < n {
                padded[pad - 1 - i] = x[i + 1];
            }
            if n >= 2 + i {
                padded[pad + n + i] = x[n - 2 - i];
            }
        }

        let n_fft = 400;
        let hop = 160;
        let n_freqs = n_fft / 2 + 1;
        let fft = self.planner.plan_fft_forward(n_fft);

        let mut mel_matrix = Vec::new();
        let mut k = 0;
        loop {
            let start = k * hop;
            if start + n_fft > padded_len {
                break;
            }

            let frame = &padded[start..start + n_fft];
            let mut buffer: Vec<Complex<f32>> = frame.iter().zip(self.w.iter())
                .map(|(&xv, &win)| Complex { re: xv * win, im: 0.0 })
                .collect();
            
            fft.process(&mut buffer);

            let mut power = vec![0.0f32; n_freqs];
            for i in 0..n_freqs {
                power[i] = buffer[i].re * buffer[i].re + buffer[i].im * buffer[i].im;
            }

            let mut mel_frame = [0.0f32; 64];
            for m_idx in 0..64 {
                let mut energy = 0.0f32;
                for f in 0..n_freqs {
                    energy += self.fb[f][m_idx] * power[f];
                }
                mel_frame[m_idx] = energy;
            }
            mel_matrix.push(mel_frame);
            k += 1;
        }

        mel_matrix
    }
}

pub struct Phi2StreamingFrontend {
    mel: Phi1MelFrontend,
    pending_48k: Vec<f32>,
    dec_buf: Vec<f32>,
    dec_consumed: usize,
    first: bool,
    total_48k: usize,
    carry_48k: usize,
}

impl Phi2StreamingFrontend {
    pub fn new() -> Self {
        Self {
            mel: Phi1MelFrontend::new(),
            pending_48k: Vec::new(),
            dec_buf: Vec::new(),
            dec_consumed: 0,
            first: true,
            total_48k: 0,
            carry_48k: 0,
        }
    }

    pub fn push(&mut self, new_48k: &[f32]) -> Vec<[f32; 64]> {
        self.pending_48k.extend_from_slice(new_48k);
        let mut frames = Vec::new();
        
        let m = 120;
        if self.pending_48k.len() <= m {
            return frames;
        }

        let len = self.pending_48k.len();
        let valid_end = len.saturating_sub(60);
        if valid_end <= self.carry_48k {
            return frames;
        }

        // F-103/F-104: μία πηγή — βλ. stream_core.rs:7.
        let orig_sr = lineos_types::analysis::ANALYSIS_SAMPLE_RATE as f32;
        let fc = 7600.0 / orig_sr;
        let mut h = vec![0.0f32; m + 1];
        let mut sum_h = 0.0;
        for n in 0..=m {
            let n_f32 = n as f32;
            let m_f32 = m as f32;
            let hamming = 0.54 - 0.46 * (2.0 * std::f32::consts::PI * n_f32 / m_f32).cos();
            let x_val = n_f32 - m_f32 / 2.0;
            let sinc = if x_val.abs() < 1e-6 {
                2.0 * fc
            } else {
                (2.0 * std::f32::consts::PI * fc * x_val).sin() / (std::f32::consts::PI * x_val)
            };
            h[n] = sinc * hamming;
            sum_h += h[n];
        }
        for n in 0..=m {
            h[n] /= sum_h;
        }

        let half_m = m / 2;
        let out_n = valid_end - self.carry_48k;
        let mut filtered = vec![0.0f32; out_n];
        let src = &self.pending_48k[..];

        for (idx, i) in (self.carry_48k..valid_end).enumerate() {
            let lo = i as isize - half_m as isize;
            let hi = lo + m as isize;
            let acc = if lo >= 0 && hi < len as isize {
                let base = lo as usize;
                let mut a = 0.0f32;
                for j in 0..=m {
                    a += h[j] * src[base + j];
                }
                a
            } else {
                let mut a = 0.0f32;
                for j in 0..=m {
                    let mut s = i as isize + j as isize - half_m as isize;
                    if s < 0 { s = 0; }
                    if s >= len as isize { s = len as isize - 1; }
                    a += h[j] * src[s as usize];
                }
                a
            };
            filtered[idx] = acc;
        }

        let phase = (3 - (self.total_48k % 3)) % 3;
        let x: Vec<f32> = filtered.iter().skip(phase).step_by(3).copied().collect();
        
        if self.first {
            let mut padded = vec![0.0f32; 200 + x.len()];
            padded[200..].copy_from_slice(&x);
            for i in 0..200 {
                if i + 1 < x.len() {
                    padded[200 - 1 - i] = x[i + 1];
                }
            }
            self.dec_buf.extend_from_slice(&padded);
            self.first = false;
        } else {
            self.dec_buf.extend_from_slice(&x);
        }

        let keep = 120;
        let _consume;
        if valid_end > keep {
            let drop = valid_end - keep;
            _consume = drop;
            self.pending_48k.drain(0..drop);
            self.carry_48k = keep;
        } else {
            _consume = 0;
            self.carry_48k = valid_end;
        }

        self.total_48k += out_n;

        let n_fft = 400;
        let hop = 160;
        let n_freqs = n_fft / 2 + 1;
        let fft = self.mel.planner.plan_fft_forward(n_fft);

        loop {
            let start = self.dec_consumed * hop;
            if start + n_fft > self.dec_buf.len() {
                break;
            }

            let frame = &self.dec_buf[start..start + n_fft];
            let mut buffer: Vec<rustfft::num_complex::Complex<f32>> = frame.iter().zip(self.mel.w.iter())
                .map(|(&xv, &win)| rustfft::num_complex::Complex { re: xv * win, im: 0.0 })
                .collect();
            
            fft.process(&mut buffer);

            let mut power = vec![0.0f32; n_freqs];
            for i in 0..n_freqs {
                power[i] = buffer[i].re * buffer[i].re + buffer[i].im * buffer[i].im;
            }

            let mut mel_frame = [0.0f32; 64];
            for m_idx in 0..64 {
                let mut energy = 0.0f32;
                for f in 0..n_freqs {
                    energy += self.mel.fb[f][m_idx] * power[f];
                }
                mel_frame[m_idx] = energy;
            }

            frames.push(mel_frame);
            self.dec_consumed += 1;
        }

        let consume_dec = self.dec_consumed * hop;
        if consume_dec > 0 {
            self.dec_buf.drain(0..consume_dec);
            self.dec_consumed = 0;
        }

        frames
    }

    pub fn finish(&mut self) -> Vec<[f32; 64]> {
        let mut frames = Vec::new();
        let m = 120;
        
        let len = self.pending_48k.len();
        if len == 0 {
            return frames;
        }

        // F-103/F-104: μία πηγή — βλ. stream_core.rs:7.
        let orig_sr = lineos_types::analysis::ANALYSIS_SAMPLE_RATE as f32;
        let fc = 7600.0 / orig_sr;
        let mut h = vec![0.0f32; m + 1];
        let mut sum_h = 0.0;
        for n in 0..=m {
            let n_f32 = n as f32;
            let m_f32 = m as f32;
            let hamming = 0.54 - 0.46 * (2.0 * std::f32::consts::PI * n_f32 / m_f32).cos();
            let x_val = n_f32 - m_f32 / 2.0;
            let sinc = if x_val.abs() < 1e-6 {
                2.0 * fc
            } else {
                (2.0 * std::f32::consts::PI * fc * x_val).sin() / (std::f32::consts::PI * x_val)
            };
            h[n] = sinc * hamming;
            sum_h += h[n];
        }
        for n in 0..=m {
            h[n] /= sum_h;
        }

        let half_m = m / 2;
        let out_n = len.saturating_sub(self.carry_48k);
        let mut filtered = vec![0.0f32; out_n];
        for (idx, i) in (self.carry_48k..len).enumerate() {
            let mut acc = 0.0;
            for j in 0..=m {
                let mut src_idx = i as isize + j as isize - half_m as isize;
                if src_idx < 0 { src_idx = 0; }
                if src_idx >= len as isize { src_idx = len as isize - 1; }
                acc += h[j] * self.pending_48k[src_idx as usize];
            }
            filtered[idx] = acc;
        }

        let phase = (3 - (self.total_48k % 3)) % 3;
        let x: Vec<f32> = filtered.iter().skip(phase).step_by(3).copied().collect();
        
        self.dec_buf.extend_from_slice(&x);
        
        let n = self.dec_buf.len();
        for i in 0..200 {
            if n >= 2 + i {
                let v = self.dec_buf[n - 2 - i];
                self.dec_buf.push(v);
            }
        }
        
        self.pending_48k.clear();
        
        let n_fft = 400;
        let hop = 160;
        let n_freqs = n_fft / 2 + 1;
        let fft = self.mel.planner.plan_fft_forward(n_fft);

        loop {
            let start = self.dec_consumed * hop;
            if start + n_fft > self.dec_buf.len() {
                break;
            }

            let frame = &self.dec_buf[start..start + n_fft];
            let mut buffer: Vec<rustfft::num_complex::Complex<f32>> = frame.iter().zip(self.mel.w.iter())
                .map(|(&xv, &win)| rustfft::num_complex::Complex { re: xv * win, im: 0.0 })
                .collect();
            
            fft.process(&mut buffer);

            let mut power = vec![0.0f32; n_freqs];
            for i in 0..n_freqs {
                power[i] = buffer[i].re * buffer[i].re + buffer[i].im * buffer[i].im;
            }

            let mut mel_frame = [0.0f32; 64];
            for m_idx in 0..64 {
                let mut energy = 0.0f32;
                for f in 0..n_freqs {
                    energy += self.mel.fb[f][m_idx] * power[f];
                }
                mel_frame[m_idx] = energy;
            }
            frames.push(mel_frame);
            self.dec_consumed += 1;
        }

        frames
    }
}

pub fn compute_norm(frames: &[[f32; 64]]) -> Phi1Norm {
    let mut sum = 0.0;
    let mut count = 0;
    for frame in frames {
        for &val in frame {
            sum += val;
            count += 1;
        }
    }
    if count == 0 {
        return Phi1Norm { mean: 0.0, std: 1.0 };
    }
    let mean = sum / count as f32;

    let mut sum_sq = 0.0;
    for frame in frames {
        for &val in frame {
            let diff = val - mean;
            sum_sq += diff * diff;
        }
    }
    let std = if count > 1 {
        (sum_sq / (count as f32 - 1.0)).sqrt()
    } else {
        1.0
    };

    Phi1Norm { mean, std }
}

fn read_f32_le(data: &[u8], offset: &mut usize) -> f32 {
    let bytes = [
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ];
    *offset += 4;
    f32::from_le_bytes(bytes)
}

pub struct Phi1Sensor {
    norm: Phi1Norm,
    ring_buf: Vec<[f32; 64]>,
    count: usize,
    
    conv1_w: Vec<Vec<Vec<f32>>>,
    conv1_b: Vec<f32>,
    conv2_w: Vec<Vec<Vec<f32>>>,
    conv2_b: Vec<f32>,
    fc1_w: Vec<Vec<f32>>,
    fc1_b: Vec<f32>,
    fc2_w: Vec<Vec<f32>>,
    fc2_b: f32,
}

impl Phi1Sensor {
    pub fn new(norm: Phi1Norm) -> Self {
        let model_bytes = include_bytes!("../../assets/phi1_v3.bin");
        if model_bytes.len() != 243332 {
            panic!("Invalid model size: {} bytes", model_bytes.len());
        }

        let mut offset = 0;
        
        let mut conv1_w = vec![vec![vec![0.0f32; 11]; 64]; 48];
        for o in 0..48 {
            for c in 0..64 {
                for k in 0..11 {
                    conv1_w[o][c][k] = read_f32_le(model_bytes, &mut offset);
                }
            }
        }
        let mut conv1_b = vec![0.0f32; 48];
        for o in 0..48 {
            conv1_b[o] = read_f32_le(model_bytes, &mut offset);
        }

        let mut conv2_w = vec![vec![vec![0.0f32; 11]; 48]; 48];
        for o in 0..48 {
            for c in 0..48 {
                for k in 0..11 {
                    conv2_w[o][c][k] = read_f32_le(model_bytes, &mut offset);
                }
            }
        }
        let mut conv2_b = vec![0.0f32; 48];
        for o in 0..48 {
            conv2_b[o] = read_f32_le(model_bytes, &mut offset);
        }

        let mut fc1_w = vec![vec![0.0f32; 48]; 32];
        for o in 0..32 {
            for c in 0..48 {
                fc1_w[o][c] = read_f32_le(model_bytes, &mut offset);
            }
        }
        let mut fc1_b = vec![0.0f32; 32];
        for o in 0..32 {
            fc1_b[o] = read_f32_le(model_bytes, &mut offset);
        }

        let mut fc2_w = vec![vec![0.0f32; 32]; 1];
        for c in 0..32 {
            fc2_w[0][c] = read_f32_le(model_bytes, &mut offset);
        }
        let fc2_b = read_f32_le(model_bytes, &mut offset);

        Self {
            norm,
            ring_buf: Vec::with_capacity(PHI1_CONTEXT_FRAMES),
            count: 0,
            conv1_w, conv1_b,
            conv2_w, conv2_b,
            fc1_w, fc1_b,
            fc2_w, fc2_b,
        }
    }

    pub fn push_frame(&mut self, mel: &[f32; 64]) -> Option<f32> {
        let mut norm_mel = [0.0f32; 64];
        for c in 0..64 {
            norm_mel[c] = (mel[c] - self.norm.mean) / (self.norm.std + 1e-5);
        }

        if self.ring_buf.len() == PHI1_CONTEXT_FRAMES {
            self.ring_buf.remove(0);
        }
        self.ring_buf.push(norm_mel);
        self.count += 1;

        if self.ring_buf.len() < PHI1_CONTEXT_FRAMES {
            return None;
        }

        // window[64][51]
        let mut window = vec![vec![0.0f32; 51]; 64];
        for t in 0..51 {
            for c in 0..64 {
                window[c][t] = self.ring_buf[t][c];
            }
        }

        let mut y1 = vec![vec![0.0f32; 41]; 48];
        for o in 0..48 {
            for t in 0..41 {
                let mut sum_val = self.conv1_b[o];
                for c in 0..64 {
                    for k in 0..11 {
                        sum_val += self.conv1_w[o][c][k] * window[c][t + k];
                    }
                }
                y1[o][t] = sum_val.tanh();
            }
        }

        let mut y2 = vec![0.0f32; 48];
        for o in 0..48 {
            let mut sum_val = self.conv2_b[o];
            for c in 0..48 {
                for k in 0..11 {
                    sum_val += self.conv2_w[o][c][k] * y1[c][k * 4];
                }
            }
            y2[o] = sum_val.tanh();
        }

        let mut fc1_out = vec![0.0f32; 32];
        for o in 0..32 {
            let mut sum_val = self.fc1_b[o];
            for c in 0..48 {
                sum_val += self.fc1_w[o][c] * y2[c];
            }
            fc1_out[o] = sum_val.tanh();
        }

        let mut sum_val = self.fc2_b;
        for c in 0..32 {
            sum_val += self.fc2_w[0][c] * fc1_out[c];
        }
        
        let p = 1.0 / (1.0 + (-sum_val).exp());
        Some(p)
    }
}

pub struct Phi2Pcen {
    m: [f32; 64],
    initialized: bool,
}

impl Phi2Pcen {
    pub fn new() -> Self {
        Self {
            m: [0.0; 64],
            initialized: false,
        }
    }

    pub fn process(&mut self, mel_pow: &[f32; 64]) -> [f32; 64] {
        let mut out = [0.0f32; 64];
        if !self.initialized {
            for m in 0..64 {
                self.m[m] = mel_pow[m];
            }
            self.initialized = true;
        }
        for m in 0..64 {
            self.m[m] = (1.0 - PHI2_PCEN_S) * self.m[m] + PHI2_PCEN_S * mel_pow[m];
            out[m] = (mel_pow[m] / (PHI2_PCEN_EPS + self.m[m]).powf(PHI2_PCEN_ALPHA) + PHI2_PCEN_DELTA).powf(PHI2_PCEN_R) - PHI2_PCEN_DELTA.powf(PHI2_PCEN_R);
        }
        out
    }
}

pub struct Phi2Sensor {
    ring_buf: Vec<[f32; 64]>,
    count: usize,
    
    conv1_w: Vec<f32>,
    conv1_b: Vec<f32>,
    conv2_w: Vec<f32>,
    conv2_b: Vec<f32>,
    fc1_w: Vec<f32>,
    fc1_b: Vec<f32>,
    fc2_w: Vec<f32>,
    fc2_b: f32,
    window: Vec<f32>,
    y1: Vec<f32>,
    y2: Vec<f32>,
    fc1_out: Vec<f32>,
}

impl Phi2Sensor {
    pub fn new() -> Self {
        let model_bytes = include_bytes!("../../assets/phi2_pcen.bin");
        if model_bytes.len() != 243332 {
            panic!("Invalid model size: {} bytes", model_bytes.len());
        }

        let mut offset = 0;
        
        let mut conv1_w = vec![0.0f32; 48 * 64 * 11];
        for i in 0..conv1_w.len() {
            conv1_w[i] = read_f32_le(model_bytes, &mut offset);
        }
        let mut conv1_b = vec![0.0f32; 48];
        for o in 0..48 {
            conv1_b[o] = read_f32_le(model_bytes, &mut offset);
        }

        let mut conv2_w = vec![0.0f32; 48 * 48 * 11];
        for i in 0..conv2_w.len() {
            conv2_w[i] = read_f32_le(model_bytes, &mut offset);
        }
        let mut conv2_b = vec![0.0f32; 48];
        for o in 0..48 {
            conv2_b[o] = read_f32_le(model_bytes, &mut offset);
        }

        let mut fc1_w = vec![0.0f32; 32 * 48];
        for i in 0..fc1_w.len() {
            fc1_w[i] = read_f32_le(model_bytes, &mut offset);
        }
        let mut fc1_b = vec![0.0f32; 32];
        for o in 0..32 {
            fc1_b[o] = read_f32_le(model_bytes, &mut offset);
        }

        let mut fc2_w = vec![0.0f32; 32];
        for i in 0..fc2_w.len() {
            fc2_w[i] = read_f32_le(model_bytes, &mut offset);
        }
        let fc2_b = read_f32_le(model_bytes, &mut offset);

        Self {
            ring_buf: Vec::with_capacity(PHI1_CONTEXT_FRAMES),
            count: 0,
            conv1_w,
            conv1_b,
            conv2_w,
            conv2_b,
            fc1_w,
            fc1_b,
            fc2_w,
            fc2_b,
            window: vec![0.0f32; 64 * 51],
            y1: vec![0.0f32; 11 * 48],
            y2: vec![0.0f32; 48],
            fc1_out: vec![0.0f32; 32],
        }
    }

    pub fn push_frame(&mut self, pcen: &[f32; 64]) -> Option<f32> {
        if self.ring_buf.len() < PHI1_CONTEXT_FRAMES {
            self.ring_buf.push(*pcen);
        } else {
            let idx = self.count % PHI1_CONTEXT_FRAMES;
            self.ring_buf[idx] = *pcen;
        }
        self.count += 1;

        if self.count >= PHI1_CONTEXT_FRAMES {
            for t in 0..PHI1_CONTEXT_FRAMES {
                let physical_idx = (self.count - PHI1_CONTEXT_FRAMES + t) % PHI1_CONTEXT_FRAMES;
                for c in 0..64 {
                    self.window[c * 51 + t] = self.ring_buf[physical_idx][c];
                }
            }

            // Conv1
            for ki in 0..11 {
                let t = ki * 4;
                for o in 0..48 {
                    let mut acc = self.conv1_b[o];
                    for c in 0..64 {
                        for k in 0..11 {
                            acc += self.window[c * 51 + t + k] * self.conv1_w[(o * 64 + c) * 11 + k];
                        }
                    }
                    self.y1[ki * 48 + o] = acc.tanh();
                }
            }

            // Conv2
            for o in 0..48 {
                let mut acc = self.conv2_b[o];
                for c in 0..48 {
                    for k in 0..11 {
                        acc += self.y1[k * 48 + c] * self.conv2_w[(o * 48 + c) * 11 + k];
                    }
                }
                self.y2[o] = acc.tanh();
            }

            // FC1
            for o in 0..32 {
                let mut acc = self.fc1_b[o];
                for c in 0..48 {
                    acc += self.y2[c] * self.fc1_w[o * 48 + c];
                }
                self.fc1_out[o] = acc.tanh();
            }

            // FC2
            let mut acc = self.fc2_b;
            for c in 0..32 {
                acc += self.fc1_out[c] * self.fc2_w[c];
            }
            let p = 1.0 / (1.0 + (-acc).exp());
            Some(p)
        } else {
            None
        }
    }

    /// Batch inference: παίρνει ΟΛΑ τα pcen frames και
    /// επιστρέφει ένα p ανά frame (None για τα πρώτα 50).
    /// Δεν χρησιμοποιεί ring_buf — διαβάζει απευθείας από
    /// τον πίνακα. ΙΔΙΑ αριθμητική με την push_frame.
    pub fn infer_batch(&self, frames: &[[f32; 64]]) -> Vec<Option<f32>> {
        use rayon::prelude::*;

        frames
            .par_iter()
            .enumerate()
            .map(|(i, _)| {
                if i + 1 < PHI1_CONTEXT_FRAMES {
                    None
                } else {
                    let mut window = vec![0.0f32; 64 * 51];
                    let window_start = i + 1 - PHI1_CONTEXT_FRAMES;
                    for t in 0..PHI1_CONTEXT_FRAMES {
                        let frame = &frames[window_start + t];
                        for c in 0..64 {
                            window[c * 51 + t] = frame[c];
                        }
                    }

                    // Conv1
                    let mut y1 = vec![0.0f32; 11 * 48];
                    for ki in 0..11 {
                        let t = ki * 4;
                        for o in 0..48 {
                            let mut acc = self.conv1_b[o];
                            for c in 0..64 {
                                for k in 0..11 {
                                    acc += window[c * 51 + t + k] * self.conv1_w[(o * 64 + c) * 11 + k];
                                }
                            }
                            y1[ki * 48 + o] = acc.tanh();
                        }
                    }

                    // Conv2
                    let mut y2 = vec![0.0f32; 48];
                    for o in 0..48 {
                        let mut acc = self.conv2_b[o];
                        for c in 0..48 {
                            for k in 0..11 {
                                acc += y1[k * 48 + c] * self.conv2_w[(o * 48 + c) * 11 + k];
                            }
                        }
                        y2[o] = acc.tanh();
                    }

                    // FC1
                    let mut fc1_out = vec![0.0f32; 32];
                    for o in 0..32 {
                        let mut acc = self.fc1_b[o];
                        for c in 0..48 {
                            acc += y2[c] * self.fc1_w[o * 48 + c];
                        }
                        fc1_out[o] = acc.tanh();
                    }

                    // FC2
                    let mut acc = self.fc2_b;
                    for c in 0..32 {
                        acc += fc1_out[c] * self.fc2_w[c];
                    }
                    let p = 1.0 / (1.0 + (-acc).exp());
                    Some(p)
                }
            })
            .collect()
    }
}
