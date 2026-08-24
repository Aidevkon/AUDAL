//! ΜΕΤΡΗΣΗ 2026-08-24 — η κατανομή ενός ΠΡΑΓΜΑΤΙΚΟΥ, εμπορικά
//! εκδομένου βιβλίου (Bill Gates, "Source Code", 17 mp3, Random House
//! Audio, LAME3.100 VBR — βλ. Step 0 ffprobe).
//!
//! ΓΙΑΤΙ: πάμε να χτίσουμε όψη απόκλισης χωρίς να ξέρουμε τι είναι
//! ΦΥΣΙΟΛΟΓΙΚΟ σε πραγματικό, περασμένο-από-QC υλικό.
//!
//! Καλεί τη ΔΙΚΙΑ ΜΑΣ AcxCheckAnalyzer (sp314-dsp) και τον ΠΡΑΓΜΑΤΙΚΟ
//! LazyAudioReader (m0d) — όχι re-implementation. Ροή streaming
//! (fill_buffer σε chunks) ώστε 10+ ώρες ήχου να μη χρειάζονται
//! ολόκληρο-αρχείο-στη-RAM.
//!
//! ΥΛΙΚΟ: 17 αρχεία στον δίσκο, ΟΧΙ 13 (κενό στο track 15 — βλ. Step 0
//! recon, αναφέρθηκε στον χρήστη, απάντηση: προχώρα με ό,τι υπάρχει).
//! ΣΩΜΑ = όλα τα παρόντα chapter-numbered αρχεία (05 και πάνω), ΟΧΙ
//! literal "05-13" — αλλιώς θα έλειπαν 4 πραγματικά κεφάλαια.
//!
//! usage: real_book_distribution <dataset_dir> <out_csv>

use m0d::dsp::lazy_reader::LazyAudioReader;
use rustfft::{num_complex::Complex, FftPlanner};
use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use std::io::Write;
use std::path::Path;

const LTASS_CFS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];
const LTASS_NAMES: [&str; 8] = [
    "Sub", "Bass", "LowMid", "MidLow", "MidHigh", "HighMid", "Presence", "Air",
];
const FFT_SIZE: usize = 4096;
const HOP: usize = 2048;
const ACTIVE_BLOCK_MS: usize = 100;
const ACTIVE_THRESH_DB: f32 = -60.0;
const CHUNK_FRAMES: usize = 65536;

fn stamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60
    )
}

fn hann_window(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 - 0.5 * libm::cosf(2.0 * core::f32::consts::PI * i as f32 / (n as f32 - 1.0)))
        .collect()
}

/// 100ms-block active fraction. Deliberately UN-filtered (raw mono, no
/// HP10) and DECLARED separate from AcxCheckAnalyzer's noise-floor path:
/// this is an activity ratio, not a floor measurement.
struct ActiveFracAccum {
    block_size: usize,
    sum_sq: f64,
    count: usize,
    total_blocks: u64,
    active_blocks: u64,
}
impl ActiveFracAccum {
    fn new(sr: u32) -> Self {
        Self {
            block_size: (sr as usize * ACTIVE_BLOCK_MS) / 1000,
            sum_sq: 0.0,
            count: 0,
            total_blocks: 0,
            active_blocks: 0,
        }
    }
    fn feed(&mut self, mono: &[f32]) {
        for &s in mono {
            self.sum_sq += (s as f64) * (s as f64);
            self.count += 1;
            if self.count == self.block_size {
                let rms = (self.sum_sq / self.block_size as f64).sqrt();
                let db = if rms > 1e-10 { 20.0 * rms.log10() } else { -200.0 };
                self.total_blocks += 1;
                if db > ACTIVE_THRESH_DB as f64 {
                    self.active_blocks += 1;
                }
                self.sum_sq = 0.0;
                self.count = 0;
            }
        }
    }
    fn active_frac(&self) -> f32 {
        if self.total_blocks == 0 {
            0.0
        } else {
            self.active_blocks as f32 / self.total_blocks as f32
        }
    }
}

/// Extracts FFT_SIZE-long, HOP-spaced frames out of a stream of chunks
/// that don't align to frame boundaries.
struct FrameSource {
    carry: Vec<f32>,
}
impl FrameSource {
    fn new() -> Self {
        Self { carry: Vec::new() }
    }
    fn push_and_extract(&mut self, chunk: &[f32]) -> Vec<Vec<f32>> {
        self.carry.extend_from_slice(chunk);
        let mut frames = Vec::new();
        let mut start = 0usize;
        while start + FFT_SIZE <= self.carry.len() {
            frames.push(self.carry[start..start + FFT_SIZE].to_vec());
            start += HOP;
        }
        self.carry.drain(0..start);
        frames
    }
}

/// 8-band LTASS-center profile (Q=0.707 -> ~octave edges, same centers
/// as aether-bridge REF_CFS / dsp/mod.rs LTASS_CFS) + the mic-distance
/// proxy (80-250 Hz minus 1-4 kHz), all from ONE STFT pass.
struct SpectralAccum {
    window: Vec<f32>,
    planner: FftPlanner<f32>,
    bin_hz: f32,
    band_edges: [(f32, f32); 8],
    band_energy: [f64; 8],
    mic_low_energy: f64,
    mic_mid_energy: f64,
    n_frames: u64,
}
impl SpectralAccum {
    fn new(sr: u32) -> Self {
        let bin_hz = sr as f32 / FFT_SIZE as f32;
        let band_edges =
            LTASS_CFS.map(|cf| (cf / core::f32::consts::SQRT_2, cf * core::f32::consts::SQRT_2));
        Self {
            window: hann_window(FFT_SIZE),
            planner: FftPlanner::new(),
            bin_hz,
            band_edges,
            band_energy: [0.0; 8],
            mic_low_energy: 0.0,
            mic_mid_energy: 0.0,
            n_frames: 0,
        }
    }
    fn process_frame(&mut self, frame: &[f32]) {
        let fft = self.planner.plan_fft_forward(FFT_SIZE);
        let mut buf: Vec<Complex<f32>> = (0..FFT_SIZE)
            .map(|j| Complex::new(frame[j] * self.window[j], 0.0))
            .collect();
        fft.process(&mut buf);
        for bin in 0..FFT_SIZE / 2 {
            let f = bin as f32 * self.bin_hz;
            let mag2 = (buf[bin].re * buf[bin].re + buf[bin].im * buf[bin].im) as f64;
            for (i, (lo, hi)) in self.band_edges.iter().enumerate() {
                if f >= *lo && f < *hi {
                    self.band_energy[i] += mag2;
                }
            }
            if (80.0..250.0).contains(&f) {
                self.mic_low_energy += mag2;
            }
            if (1000.0..4000.0).contains(&f) {
                self.mic_mid_energy += mag2;
            }
        }
        self.n_frames += 1;
    }
    fn finish(&self) -> ([f32; 8], f32) {
        let mut band_db = [0.0f32; 8];
        for i in 0..8 {
            let avg = self.band_energy[i] / self.n_frames.max(1) as f64;
            band_db[i] = if avg > 1e-20 {
                10.0 * avg.log10() as f32
            } else {
                -200.0
            };
        }
        let proxy_db = if self.mic_mid_energy > 1e-20 && self.mic_low_energy > 0.0 {
            10.0 * (self.mic_low_energy / self.mic_mid_energy).log10() as f32
        } else {
            f32::NAN
        };
        (band_db, proxy_db)
    }
}

struct FileResult {
    filename: String,
    track_num: u32,
    decoded_sr: u32,
    decoded_channels: usize,
    n_mono_samples: u64,
    duration_s: f64,
    rms_db: f32,
    sample_peak_db: f32,
    noise_floor_db: Option<f32>,
    crest_db: f32,
    active_frac: f32,
    mic_proxy_db: f32,
    band_db: [f32; 8],
    decode_wall_s: f64,
}

fn process_file(path: &Path, filename: &str, track_num: u32) -> Result<FileResult, String> {
    let t0 = std::time::Instant::now();
    let mut reader = LazyAudioReader::open(path).map_err(|e| format!("{e:?}"))?;
    let sr = reader.sample_rate();
    let ch = reader.channels();

    let mut acx = AcxCheckAnalyzer::new(sr);
    let mut active = ActiveFracAccum::new(sr);
    let mut spectral = SpectralAccum::new(sr);
    let mut frames_src = FrameSource::new();
    let mut n_mono: u64 = 0;
    let mut last_progress_s: f64 = 0.0;

    let mut buf = vec![0.0f32; CHUNK_FRAMES * ch];
    loop {
        let got = reader.fill_buffer(&mut buf).map_err(|e| format!("{e:?}"))?;
        if got == 0 {
            break;
        }
        let mut mono = Vec::with_capacity(got);
        for f in 0..got {
            let mut sum = 0.0f32;
            for c in 0..ch {
                sum += buf[f * ch + c];
            }
            mono.push(sum / ch as f32);
        }
        acx.feed_chunk(&mono);
        active.feed(&mono);
        for frame in frames_src.push_and_extract(&mono) {
            spectral.process_frame(&frame);
        }
        n_mono += mono.len() as u64;

        let elapsed_audio_s = n_mono as f64 / sr as f64;
        if elapsed_audio_s - last_progress_s > 600.0 {
            eprintln!(
                "  [{}]   ...{} : {:.0}s ήχου επεξεργάστηκε ({:.0}min)",
                stamp(),
                filename,
                elapsed_audio_s,
                elapsed_audio_s / 60.0
            );
            let _ = std::io::stderr().flush();
            last_progress_s = elapsed_audio_s;
        }
    }

    let report = acx.finish();
    let (band_db, mic_proxy_db) = spectral.finish();
    let crest_db = report.sample_peak_db - report.rms_db;
    let duration_s = n_mono as f64 / sr as f64;

    Ok(FileResult {
        filename: filename.to_string(),
        track_num,
        decoded_sr: sr,
        decoded_channels: ch,
        n_mono_samples: n_mono,
        duration_s,
        rms_db: report.rms_db,
        sample_peak_db: report.sample_peak_db,
        noise_floor_db: report.noise_floor_db,
        crest_db,
        active_frac: active.active_frac(),
        mic_proxy_db,
        band_db,
        decode_wall_s: t0.elapsed().as_secs_f64(),
    })
}

fn parse_track_num(filename: &str) -> Option<u32> {
    // "Source Code - 05 - Chapter One - Trey.mp3" -> 05
    filename.split(" - ").nth(1)?.trim().parse().ok()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 2, "usage: real_book_distribution <dataset_dir> <out_csv>");
    let dataset_dir = std::path::PathBuf::from(&args[0]);
    let out_csv_path = std::path::PathBuf::from(&args[1]);

    let mut entries: Vec<(std::path::PathBuf, String, u32)> = std::fs::read_dir(&dataset_dir)
        .expect("read_dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("mp3"))
        .filter_map(|p| {
            let filename = p.file_name()?.to_str()?.to_string();
            let track_num = parse_track_num(&filename)?;
            Some((p, filename, track_num))
        })
        .collect();
    entries.sort_by_key(|(_, _, n)| *n);

    eprintln!(
        "[{}] ΞΕΚΙΝΩ — {} αρχεία βρέθηκαν, ταξινομημένα κατά track number",
        stamp(),
        entries.len()
    );
    for (_, fname, n) in &entries {
        eprintln!("  #{n:02} {fname}");
    }

    let mut out = std::fs::File::create(&out_csv_path).expect("create out_csv");
    writeln!(
        out,
        "track_num,filename,decoded_sr,decoded_channels,n_mono_samples,duration_s,rms_db,sample_peak_db,noise_floor_db,crest_db,active_frac,mic_proxy_db,{}",
        LTASS_NAMES.iter().map(|n| format!("band_{n}_db")).collect::<Vec<_>>().join(",")
    ).unwrap();
    out.flush().unwrap();

    for (path, filename, track_num) in &entries {
        let t0 = std::time::Instant::now();
        eprintln!("[{}] ΞΕΚΙΝΩ #{track_num:02} {filename}", stamp());
        let _ = std::io::stderr().flush();

        match process_file(path, filename, *track_num) {
            Ok(r) => {
                let nf_str = r
                    .noise_floor_db
                    .map(|v| format!("{v:.3}"))
                    .unwrap_or_else(|| "NA".to_string());
                let bands_str = r
                    .band_db
                    .iter()
                    .map(|b| format!("{b:.3}"))
                    .collect::<Vec<_>>()
                    .join(",");
                writeln!(
                    out,
                    "{},{},{},{},{},{:.3},{:.3},{:.3},{},{:.3},{:.4},{:.3},{}",
                    r.track_num,
                    r.filename,
                    r.decoded_sr,
                    r.decoded_channels,
                    r.n_mono_samples,
                    r.duration_s,
                    r.rms_db,
                    r.sample_peak_db,
                    nf_str,
                    r.crest_db,
                    r.active_frac,
                    r.mic_proxy_db,
                    bands_str
                )
                .unwrap();
                out.flush().unwrap();

                eprintln!(
                    "[{}] ΤΕΛΟΣ #{track_num:02} {filename} — dur={:.0}s rms={:.2} peak={:.2} floor={} crest={:.2} active={:.3} mic_proxy={:.2} wall={:.1}s",
                    stamp(),
                    r.duration_s,
                    r.rms_db,
                    r.sample_peak_db,
                    nf_str,
                    r.crest_db,
                    r.active_frac,
                    r.mic_proxy_db,
                    t0.elapsed().as_secs_f64()
                );
            }
            Err(e) => {
                writeln!(out, "{track_num},{filename},FAILED: {e}").unwrap();
                out.flush().unwrap();
                eprintln!("[{}] ΑΠΕΤΥΧΕ #{track_num:02} {filename}: {e}", stamp());
            }
        }
        let _ = std::io::stderr().flush();
    }

    eprintln!("[{}] ΟΛΟΚΛΗΡΩΘΗΚΕ -> {}", stamp(), out_csv_path.display());
}
