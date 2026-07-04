//! ola_buffer.rs — Generic Overlap-Add Ring Buffer
//! Reusable DSP primitive for seamless chunk stitching with perfect reconstruction.

pub struct OlaBuffer {
    window_size: usize,
    hop_size: usize,
    hops_per_window: usize,
    window: Vec<f32>,
    ring_buf: Vec<f32>, // accumulator, ΟΧΙ window_sum_buf πια
    norm_factor: f32,   // προϋπολογισμένη σταθερά
    ring_idx: usize,
    filled_hops: usize,
}

impl OlaBuffer {
    /// Δημιουργεί νέο OLA buffer.
    /// Εσωτερικά δημιουργεί το Periodic Hann window (ίδια σύμβαση με stft/spectral.rs).
    pub fn new(window_size: usize, hop_size: usize) -> Self {
        let window: Vec<f32> = (0..window_size)
            .map(|n| {
                let theta = 2.0_f32 * core::f32::consts::PI * n as f32 / window_size as f32;
                0.5_f32 - 0.5_f32 * libm::cosf(theta)
            })
            .collect();
        Self::with_custom_window(window_size, hop_size, window)
    }

    pub fn with_custom_window(window_size: usize, hop_size: usize, window: Vec<f32>) -> Self {
        assert_eq!(window.len(), window_size);
        assert!(
            window_size.is_multiple_of(hop_size),
            "window_size must be a multiple of hop_size"
        );

        let hops_per_window = window_size / hop_size;

        let mut first_sum: Option<f32> = None;

        for phase in 0..hop_size {
            let mut sum = 0.0_f32;
            for h in 0..hops_per_window {
                let idx = h * hop_size + phase;
                if idx < window_size {
                    sum += window[idx] * window[idx];
                }
            }
            if let Some(expected) = first_sum {
                assert!(
                    (sum - expected).abs() < 1e-5,
                    "Window does not satisfy COLA! phase {} sum: {}, expected: {}",
                    phase,
                    sum,
                    expected
                );
            } else {
                first_sum = Some(sum);
            }
        }

        let cola_sum = first_sum.expect("hop_size > 0");
        assert!(cola_sum > 1e-6, "COLA sum too small to normalize");
        let norm_factor = 1.0_f32 / cola_sum;

        Self {
            window_size,
            hop_size,
            hops_per_window,
            window,
            ring_buf: vec![0.0_f32; window_size],
            norm_factor,
            ring_idx: 0,
            filled_hops: 0,
        }
    }

    /// CONTRACT: `chunk` must already be windowed with the SAME window
    /// passed to `new()`/`with_custom_window()` (symmetric WOLA analysis window).
    /// Passing a raw or differently-windowed chunk silently produces incorrect amplitude
    /// — see `perfect_reconstruction_fails_on_caller_window_mismatch` test.
    pub fn feed(&mut self, chunk: &[f32]) -> Option<Vec<f32>> {
        assert_eq!(
            chunk.len(),
            self.window_size,
            "Chunk size must equal window_size"
        );

        // 1. Εφαρμογή synthesis window (w[i] * chunk[i]) και accumulator update
        for i in 0..self.window_size {
            let val = chunk[i] * self.window[i];
            let buf_idx = (self.ring_idx + i) % self.window_size;
            self.ring_buf[buf_idx] += val;
        }

        // 2. Εξαγωγή του έτοιμου (παλαιότερου) hop
        let mut out = vec![0.0_f32; self.hop_size];
        for i in 0..self.hop_size {
            let buf_idx = (self.ring_idx + i) % self.window_size;
            out[i] = self.ring_buf[buf_idx] * self.norm_factor;
            self.ring_buf[buf_idx] = 0.0_f32; // Clear ΠΑΝΤΑ
        }

        let output = if self.filled_hops >= self.hops_per_window - 1 {
            Some(out)
        } else {
            None
        };

        self.ring_idx = (self.ring_idx + self.hop_size) % self.window_size;
        self.filled_hops += 1;

        output
    }

    /// Αδειάζει τα υπολειπόμενα hops στο τέλος του stream.
    pub fn flush(&mut self) -> Vec<Vec<f32>> {
        let mut flushed = Vec::new();
        let remaining = if self.filled_hops < self.hops_per_window {
            self.filled_hops
        } else {
            self.hops_per_window - 1
        };

        for _ in 0..remaining {
            let mut out = vec![0.0_f32; self.hop_size];
            for i in 0..self.hop_size {
                let buf_idx = (self.ring_idx + i) % self.window_size;
                out[i] = self.ring_buf[buf_idx] * self.norm_factor;
                self.ring_buf[buf_idx] = 0.0_f32;
            }
            flushed.push(out);
            self.ring_idx = (self.ring_idx + self.hop_size) % self.window_size;
        }
        flushed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_reconstruction_sine_wave() {
        let window_size = 2048;
        let hop_size = 512;
        let mut ola = OlaBuffer::new(window_size, hop_size);

        let n_samples = 20480;
        let signal: Vec<f32> = (0..n_samples)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 1000.0 * i as f32 / 48000.0))
            .collect();

        let mut output = Vec::new();

        let mut pos = 0;
        while pos + window_size <= signal.len() {
            let chunk = &signal[pos..pos + window_size];

            let windowed_chunk: Vec<f32> =
                chunk.iter().zip(&ola.window).map(|(x, w)| x * w).collect();

            if let Some(mut reconstructed_hop) = ola.feed(&windowed_chunk) {
                output.append(&mut reconstructed_hop);
            }
            pos += hop_size;
        }

        // Only compare the fully reconstructed steady-state hops (from feed).
        // The flush tail is a fade-out (lacks future overlaps) and will naturally be attenuated.
        let compare_len = output.len();

        let flushed = ola.flush();
        assert_eq!(
            flushed.len(),
            3,
            "Flush must return exactly hops_per_window - 1 hops"
        );
        for mut hop in flushed {
            assert_eq!(hop.len(), hop_size, "Flushed hop must have correct length");
            for &sample in &hop {
                assert!(sample.is_finite(), "Flushed sample must be finite");
            }
            output.append(&mut hop);
        }

        let delay = window_size - hop_size;
        let mut sq_err = 0.0;
        for i in 0..compare_len {
            let orig_sample = signal[delay + i];
            let out_sample = output[i];
            sq_err += (out_sample - orig_sample).powi(2);
        }
        let rms_err = libm::sqrtf(sq_err / compare_len.max(1) as f32);

        // ΜΑΘΗΜΑΤΙΚΗ ΑΠΟΔΕΙΞΗ WOLA COLA-CONSTANT (Hann², 75% overlap):
        // N=8, hop=2 παράδειγμα (ίδια αναλογία με 2048/512):
        // w[n] = 0.5 - 0.5*cos(2πn/8), n=0..7
        // w²: [0.0, 0.0215, 0.25, 0.7286, 1.0, 0.7286, 0.25, 0.0215]
        // Άθροισμα 4 επικαλύψεων ανά phase (κυκλικά, βήμα hop=2):
        //   phase 0: w²[0]+w²[2]+w²[4]+w²[6] = 0.0+0.25+1.0+0.25 = 1.5
        //   phase 1: w²[1]+w²[3]+w²[5]+w²[7] = 0.0215+0.7286+0.7286+0.0215 = 1.5
        // Σταθερό σε όλα τα phases → COLA ικανοποιείται, norm_factor=1/1.5.

        assert!(
            rms_err < 1e-6,
            "Reconstruction failed! RMS Error: {}",
            rms_err
        );
    }

    #[test]
    #[should_panic(expected = "Reconstruction failed")]
    fn perfect_reconstruction_fails_on_caller_window_mismatch() {
        let window_size = 2048;
        let hop_size = 512;

        let mut ola = OlaBuffer::new(window_size, hop_size);

        let signal: Vec<f32> = (0..20480)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 1000.0 * i as f32 / 48000.0))
            .collect();

        let mut output = Vec::new();
        let mut pos = 0;
        while pos + window_size <= signal.len() {
            let raw_chunk = &signal[pos..pos + window_size];

            // ΑΡΝΗΤΙΚΟΣ ΕΛΕΓΧΟΣ: Εξαπατούμε το API περνώντας raw chunk.
            // Το feed() εφαρμόζει synthesis Hann window, οπότε το συνολικό βάρος = απλό Hann (ΟΧΙ Hann²).
            // Το COLA sum του απλού Hann σε 75% overlap είναι 2.0.
            // Αφού το norm_factor του OlaBuffer έχει υπολογιστεί για Hann² (1.5), η έξοδος
            // πολλαπλασιάζεται με 1/1.5.
            // Το amplitude της εξόδου θα είναι 2.0 / 1.5 ≈ 1.333×,
            // άρα το RMS error θα εκτοξευτεί και το tight 1e-6 tolerance θα αποτύχει εγγυημένα!
            if let Some(mut reconstructed_hop) = ola.feed(raw_chunk) {
                output.append(&mut reconstructed_hop);
            }
            pos += hop_size;
        }

        let delay = window_size - hop_size;
        let mut sq_err = 0.0;
        let compare_len = output.len();
        for i in 0..compare_len {
            let orig_sample = signal[delay + i];
            let out_sample = output[i];
            sq_err += (out_sample - orig_sample).powi(2);
        }
        let rms_err = libm::sqrtf(sq_err / compare_len.max(1) as f32);

        assert!(
            rms_err < 1e-6,
            "Reconstruction failed! RMS Error: {}",
            rms_err
        );
    }
}
