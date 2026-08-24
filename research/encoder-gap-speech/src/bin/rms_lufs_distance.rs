//! ΜΕΤΡΗΣΗ 2026-08-24 — η απόσταση RMS↔LUFS σε πραγματική αφήγηση.
//! ΓΙΑΤΙ: το mastering στοχεύει LUFS (autotune), το ACX κρίνει RMS
//! (-23..-18), και το -ζωντανό- μονοπάτι δεν στοχεύει ΠΟΤΕ το RMS
//! παράθυρο (.reports/2026-08-24-live-spectral-path.md §5).
//!
//! rms_db: AcxCheckAnalyzer (sp314-dsp, πραγματικός) πάνω σε mono
//!   downmix, ίδια σύμβαση με το real_book_distribution.rs.
//! integrated LUFS: encoder_gap_speech::ffmpeg_ebur128_on_file — ΤΟ
//!   ΙΔΙΟ όργανο που χρησιμοποίησε το F-077 γύρος 2 (lufs_round2.rs).
//!
//! usage: rms_lufs_distance <out_csv> <file1> <file2> ...

use encoder_gap_speech::ffmpeg_ebur128_on_file;
use m0d::dsp::lazy_reader::LazyAudioReader;
use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use std::io::Write;
use std::path::Path;

const ACTIVE_BLOCK_MS: usize = 100;
const ACTIVE_THRESH_DB: f32 = -60.0;

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

fn stamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    format!("{:02}:{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}

fn measure_rms_active(path: &Path) -> Result<(f32, f32, u64), String> {
    let mut reader = LazyAudioReader::open(path).map_err(|e| format!("{e:?}"))?;
    let sr = reader.sample_rate();
    let ch = reader.channels();
    let mut acx = AcxCheckAnalyzer::new(sr);
    let mut active = ActiveFracAccum::new(sr);
    let mut n_mono: u64 = 0;
    let mut buf = vec![0.0f32; 65536 * ch];
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
        n_mono += mono.len() as u64;
    }
    let report = acx.finish();
    Ok((report.rms_db, active.active_frac(), n_mono))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 2, "usage: rms_lufs_distance <out_csv> <file1> <file2> ...");
    let out_csv_path = std::path::PathBuf::from(&args[0]);
    let files = &args[1..];

    eprintln!("[{}] ΞΕΚΙΝΩ — {} αρχεία", stamp(), files.len());
    let mut out = std::fs::File::create(&out_csv_path).expect("create out_csv");
    writeln!(out, "file,rms_db,integrated_lufs,true_peak_dbtp,distance_lufs_minus_rms,active_frac,n_mono_samples").unwrap();
    out.flush().unwrap();

    for f in files {
        let t0 = std::time::Instant::now();
        let path = std::path::Path::new(f);
        let fname = path.file_name().unwrap().to_string_lossy().to_string();
        eprintln!("[{}] ΞΕΚΙΝΩ {fname}", stamp());
        let _ = std::io::stderr().flush();

        let rms_active = measure_rms_active(path);
        let lufs = ffmpeg_ebur128_on_file(&[], path);

        match (rms_active, lufs) {
            (Ok((rms_db, active_frac, n)), Ok((lufs, tp))) => {
                let distance = lufs as f32 - rms_db;
                writeln!(
                    out,
                    "{fname},{rms_db:.3},{lufs:.3},{tp:.3},{distance:.3},{active_frac:.4},{n}"
                )
                .unwrap();
                out.flush().unwrap();
                eprintln!(
                    "[{}] ΤΕΛΟΣ {fname} — rms={rms_db:.2} lufs={lufs:.2} dist={distance:.2} active={active_frac:.3} wall={:.1}s",
                    stamp(),
                    t0.elapsed().as_secs_f64()
                );
            }
            (Err(e), _) => {
                writeln!(out, "{fname},FAILED_RMS: {e},,,,,").unwrap();
                out.flush().unwrap();
                eprintln!("[{}] ΑΠΕΤΥΧΕ (rms) {fname}: {e}", stamp());
            }
            (_, Err(e)) => {
                writeln!(out, "{fname},,FAILED_LUFS: {e},,,,").unwrap();
                out.flush().unwrap();
                eprintln!("[{}] ΑΠΕΤΥΧΕ (lufs) {fname}: {e}", stamp());
            }
        }
        let _ = std::io::stderr().flush();
    }
    eprintln!("[{}] ΟΛΟΚΛΗΡΩΘΗΚΕ -> {}", stamp(), out_csv_path.display());
}
