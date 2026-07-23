#![allow(clippy::needless_range_loop)]
use crate::dsp::signal_health::SignalHealthMonitor;
use blake3::Hasher as Blake3Hasher;
use rubato::{Resampler, SincFixedIn};
use sha2::{Digest, Sha256};

pub const TARGET_SR: u32 = 48_000;
pub const SINC_LEN: usize = 256;
pub const SINC_OVERSAMPLE: usize = 256;
pub const RESAMPLE_CHUNK_FRAMES: usize = 1024;
pub const MAX_DURATION_SECS: u64 = 720;
pub const READ_FRAMES: usize = 4096;

#[inline(always)]
pub fn sanitize_sample(s: f32) -> f32 {
    if s.is_nan() || s.is_infinite() {
        0.0
    } else {
        s.clamp(-1.0, 1.0)
    }
}

pub struct StandardizedStreamCore<const N: usize> {
    pub reader: crate::dsp::lazy_reader::LazyAudioReader,
    pub resampler: Option<SincFixedIn<f32>>,
    pub in_ch: [Vec<f32>; N],
    pub out_ring: std::collections::VecDeque<f32>,
    pub read_buf: Vec<f32>,
    pub reader_eof: bool,
    pub flushed: bool,
    pub blake3: Blake3Hasher,
    pub sha256: Sha256,
    pub health: Option<SignalHealthMonitor>,
    pub est_total_frames: Option<u64>,
    pub expected_output_frames: Option<u64>,
}

impl<const N: usize> StandardizedStreamCore<N> {
    pub fn new(
        reader: crate::dsp::lazy_reader::LazyAudioReader,
        resampler: Option<SincFixedIn<f32>>,
        est_total_frames: Option<u64>,
        expected_output_frames: Option<u64>,
        health: Option<SignalHealthMonitor>,
    ) -> Self {
        const EMPTY_VEC: Vec<f32> = Vec::new();
        let mut in_ch = [EMPTY_VEC; N];
        for v in in_ch.iter_mut() {
            *v = Vec::with_capacity(RESAMPLE_CHUNK_FRAMES * 2);
        }
        Self {
            reader,
            resampler,
            in_ch,
            out_ring: std::collections::VecDeque::new(),
            read_buf: Vec::new(),
            reader_eof: false,
            flushed: false,
            blake3: Blake3Hasher::new(),
            sha256: Sha256::new(),
            health,
            est_total_frames,
            expected_output_frames,
        }
    }

    pub fn push_input_frame(&mut self, frame: [f32; N]) {
        for ch in 0..N {
            self.in_ch[ch].push(frame[ch]);
        }
    }

    pub fn process_one_block(&mut self, pad: bool) -> Result<(), String> {
        let have = self.in_ch[0].len();
        if have == 0 {
            return Ok(());
        }
        let take = have.min(RESAMPLE_CHUNK_FRAMES);

        let mut wave_in = vec![vec![]; N];
        if take == RESAMPLE_CHUNK_FRAMES {
            for ch in 0..N {
                wave_in[ch] = self.in_ch[ch][..RESAMPLE_CHUNK_FRAMES].to_vec();
            }
        } else if pad {
            for ch in 0..N {
                let mut v = self.in_ch[ch][..take].to_vec();
                if self.resampler.is_some() {
                    v.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
                }
                wave_in[ch] = v;
            }
        } else {
            return Ok(());
        };

        for ch in 0..N {
            self.in_ch[ch].drain(..take);
        }

        if let Some(rs_mut) = self.resampler.as_mut() {
            let wave_out = rs_mut
                .process(&wave_in, None)
                .map_err(|e| format!("resample: {e}"))?;

            let out_arr: [Vec<f32>; N] = std::array::from_fn(|i| wave_out[i].clone());
            self.push_output(&out_arr);
        } else {
            let out_arr: [Vec<f32>; N] = std::array::from_fn(|i| wave_in[i].clone());
            self.push_output(&out_arr);
        }
        Ok(())
    }

    pub fn push_output(&mut self, wave_out: &[Vec<f32>; N]) {
        if wave_out[0].is_empty() {
            return;
        }
        let frames = wave_out[0].len();
        let mut interleaved = Vec::with_capacity(frames * N);
        for i in 0..frames {
            for ch in 0..N {
                let s = sanitize_sample(wave_out[ch][i]);
                self.blake3.update(&s.to_le_bytes());
                self.sha256.update(s.to_be_bytes());
                interleaved.push(s);
            }
        }
        if let Some(h) = self.health.as_mut() {
            h.observe(&interleaved);
        }
        self.out_ring.extend(interleaved);
    }

    pub fn flush(&mut self) -> Result<(), String> {
        if self.flushed {
            return Ok(());
        }
        // Drain loop closing F-047 regression
        while !self.in_ch[0].is_empty() {
            self.process_one_block(true)?;
        }
        if let Some(rs) = self.resampler.as_mut() {
            if let Ok(tail) = rs.process_partial::<Vec<f32>>(None, None) {
                if !tail[0].is_empty() {
                    let out_arr: [Vec<f32>; N] = std::array::from_fn(|i| tail[i].clone());
                    self.push_output(&out_arr);
                }
            }
        }
        self.flushed = true;
        Ok(())
    }

    pub fn fill_buffer<F>(
        &mut self,
        buffer: &mut [f32],
        mut pull_and_normalize: F,
    ) -> Result<usize, String>
    where
        F: FnMut(&mut Self) -> Result<(), String>,
    {
        let want_samples = buffer.len();
        while self.out_ring.len() < want_samples {
            if !self.reader_eof {
                pull_and_normalize(self)?;
                while self.in_ch[0].len() >= RESAMPLE_CHUNK_FRAMES {
                    self.process_one_block(false)?;
                }
            } else {
                self.flush()?;
                break;
            }
        }

        let n = want_samples.min(self.out_ring.len());
        for slot in buffer.iter_mut().take(n) {
            *slot = self.out_ring.pop_front().unwrap_or(0.0);
        }
        Ok(n / N)
    }

    pub fn input_hashes(&self) -> (String, String) {
        let b = self.blake3.finalize().to_hex().to_string();
        let s = format!("{:x}", self.sha256.clone().finalize());
        (b, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f047_regression_guard_flush_drains_multiple_blocks() {
        let path = "/tmp/f047_dummy.wav";
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for _ in 0..48000 * 2 {
            w.write_sample(0.0f32).unwrap();
        }
        w.finalize().unwrap();

        let reader =
            crate::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(path)).unwrap();
        let mut core = StandardizedStreamCore::<2>::new(reader, None, None, None, None);

        for _ in 0..2500 {
            core.push_input_frame([0.5, -0.5]);
        }

        core.flush().unwrap();

        // 2500 frames * 2 channels = 5000 output samples
        assert_eq!(
            core.out_ring.len(),
            5000,
            "Flush did not drain all blocks (F-047 regression)"
        );
        let _ = std::fs::remove_file(path);
    }
}
