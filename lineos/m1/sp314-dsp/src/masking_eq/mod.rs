use rustfft::{FftPlanner, num_complex::Complex};
use crate::psychoacoustic;

pub mod biquad;

pub const FFT_SIZE: usize = 1024;
pub const HOP_SIZE: usize = 512;
pub const EQ_BANDS: usize = 8;
pub const BAND_CENTER_HZ: [f32; EQ_BANDS] =
    [80.0, 160.0, 320.0, 640.0, 1250.0, 2500.0, 5000.0, 10000.0];
pub const BAND_Q: f32 = 1.414;

#[derive(Clone, Debug)]
pub struct MaskingEQConfig {
    pub target_db:      [f32; EQ_BANDS],
    pub mask_margin_db: f32,
    pub max_boost_db:   f32,
    pub target_phon:    f32,
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidMaskMargin(f32),
    InvalidMaxBoost(f32),
    InvalidTargetPhon(f32),
    InvalidSampleRate(u32),
}

pub struct MaskingAwareEQ {
    analysis_buffer: Vec<f32>,
    write_pos:        usize,
    samples_in_hop:   usize,

    fft:         std::sync::Arc<dyn rustfft::Fft<f32>>,
    hann_window: Vec<f32>,
    fft_io:      Vec<Complex<f32>>,
    fft_scratch: Vec<Complex<f32>>,

    filter_states_l: [biquad::BiquadState; EQ_BANDS],
    filter_states_r: [biquad::BiquadState; EQ_BANDS],
    current_coeffs: [biquad::BiquadCoeffs; EQ_BANDS],
    current_gain_db: [f64; EQ_BANDS],
    target_gain_db:  [f64; EQ_BANDS],
    gain_step_db:    [f64; EQ_BANDS],

    config: MaskingEQConfig,
    sample_rate: u32,
}

impl MaskingAwareEQ {
    pub fn new(config: MaskingEQConfig, sample_rate: u32) -> Result<Self, ConfigError> {
        if config.mask_margin_db < 0.0 || config.mask_margin_db > 12.0 {
            return Err(ConfigError::InvalidMaskMargin(config.mask_margin_db));
        }
        if config.max_boost_db < 0.0 || config.max_boost_db > 12.0 {
            return Err(ConfigError::InvalidMaxBoost(config.max_boost_db));
        }
        if config.target_phon < 80.0 || config.target_phon > 90.0 {
            return Err(ConfigError::InvalidTargetPhon(config.target_phon));
        }
        if sample_rate != 48000 {
            return Err(ConfigError::InvalidSampleRate(sample_rate));
        }

        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let scratch_len = fft.get_inplace_scratch_len();

        let mut hann_window = vec![0.0; FFT_SIZE];
        for (i, item) in hann_window.iter_mut().enumerate() {
            *item = 0.5 * (1.0 - libm::cosf(2.0 * core::f32::consts::PI * i as f32 / (FFT_SIZE as f32 - 1.0)));
        }

        let current_coeffs = [biquad::BiquadCoeffs::new(); EQ_BANDS];
        let current_gain_db = [0.0_f64; EQ_BANDS];
        let target_gain_db  = [0.0_f64; EQ_BANDS];
        let gain_step_db    = [0.0_f64; EQ_BANDS];
        let filter_states_l  = [biquad::BiquadState::new(); EQ_BANDS];
        let filter_states_r  = [biquad::BiquadState::new(); EQ_BANDS];

        Ok(Self {
            analysis_buffer: vec![0.0; FFT_SIZE],
            write_pos: 0,
            samples_in_hop: 0,
            fft,
            hann_window,
            fft_io: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            fft_scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            filter_states_l,
            filter_states_r,
            current_coeffs,
            current_gain_db,
            target_gain_db,
            gain_step_db,
            config,
            sample_rate,
        })
    }

    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());

        for i in 0..left.len() {
            let mid = (left[i] + right[i]) * 0.5_f32;

            self.analysis_buffer[self.write_pos] = mid;
            self.write_pos = (self.write_pos + 1) & (FFT_SIZE - 1);
            self.samples_in_hop += 1;

            if self.samples_in_hop == HOP_SIZE {
                self.current_gain_db = self.target_gain_db;
                self.run_analysis();
                self.samples_in_hop = 0;
            }

            for b in 0..EQ_BANDS {
                self.current_gain_db[b] += self.gain_step_db[b];
                self.current_coeffs[b] = biquad::rbj_bell(
                    BAND_CENTER_HZ[b] as f64,
                    self.current_gain_db[b],
                    BAND_Q as f64,
                    self.sample_rate as f64,
                );
            }

            let mut y_l = left[i];
            for b in 0..EQ_BANDS {
                y_l = biquad::process_tdf2(y_l, &self.current_coeffs[b], &mut self.filter_states_l[b]);
            }

            let mut y_r = right[i];
            for b in 0..EQ_BANDS {
                y_r = biquad::process_tdf2(y_r, &self.current_coeffs[b], &mut self.filter_states_r[b]);
            }

            left[i]  = y_l;
            right[i] = y_r;
        }
    }

    fn run_analysis(&mut self) {
        for i in 0..FFT_SIZE {
            let buf_idx = (self.write_pos + i) & (FFT_SIZE - 1);
            self.fft_io[i] = Complex::new(
                self.analysis_buffer[buf_idx] * self.hann_window[i],
                0.0
            );
        }

        self.fft.process_with_scratch(&mut self.fft_io, &mut self.fft_scratch);

        let mut spectrum_db = [0.0f32; FFT_SIZE / 2 + 1];
        for (bin, item) in spectrum_db.iter_mut().enumerate() {
            let re = self.fft_io[bin].re;
            let im = self.fft_io[bin].im;
            let mag = libm::sqrtf(re * re + im * im);
            *item = 20.0 * libm::log10f(mag / FFT_SIZE as f32 + 1e-10_f32);
        }

        let mut mask_buf = [0.0f32; FFT_SIZE / 2 + 1];
        psychoacoustic::bark::calculate_mask_per_bin(&spectrum_db, FFT_SIZE, self.sample_rate, &mut mask_buf);
        psychoacoustic::iso226::apply_equal_loudness_weighting(&mut spectrum_db, self.sample_rate, FFT_SIZE, self.config.target_phon);

        for (b, &center_hz) in BAND_CENTER_HZ.iter().enumerate() {
            let mut bin = (center_hz * FFT_SIZE as f32 / self.sample_rate as f32) as usize;
            bin = bin.min(FFT_SIZE / 2);

            let mask = mask_buf[bin];
            let target_db = self.config.target_db[b];
            
            let allowed_boost = if target_db < 0.0 {
                target_db
            } else if target_db > mask + self.config.mask_margin_db {
                libm::fminf(target_db - mask, self.config.max_boost_db)
            } else {
                0.0
            };

            let allowed_boost_f64 = allowed_boost as f64;
            self.target_gain_db[b] = allowed_boost_f64;
            self.gain_step_db[b] =
                (self.target_gain_db[b] - self.current_gain_db[b])
                / HOP_SIZE as f64;
        }
    }

    pub fn reset(&mut self) {
        self.current_gain_db = [0.0_f64; EQ_BANDS];
        self.target_gain_db  = [0.0_f64; EQ_BANDS];
        self.gain_step_db    = [0.0_f64; EQ_BANDS];
        self.current_coeffs  = [biquad::BiquadCoeffs {
            b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0
        }; EQ_BANDS];
        self.filter_states_l  = [biquad::BiquadState::new(); EQ_BANDS];
        self.filter_states_r  = [biquad::BiquadState::new(); EQ_BANDS];
        self.write_pos      = 0;
        self.samples_in_hop = 0;
        self.analysis_buffer.fill(0.0);
        self.fft_io.fill(Complex::new(0.0, 0.0));
    }
}
