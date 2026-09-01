//! ΜΕΤΡΗΣΗ: πόσα αρχεία αφήγησης κολλάνε στο clamp του LTASS;
//!
//! Ίδια αλυσίδα με το ltass_sibilance.rs: ΠΡΑΓΜΑΤΙΚΟ run_trunk_pass →
//! ΠΡΑΓΜΑΤΙΚΟΣ ReferenceProfile::load(PodcastV1) → ΠΡΑΓΜΑΤΙΚΟΣ
//! ReferenceResolver::resolve. Καμία επανυλοποίηση.
//! Δέχεται λίστα αρχείων· αποκωδικοποιεί με ffmpeg σε raw f32 48k stereo
//! (ό,τι μορφή περιμένει το trunk pass).

const CENTERS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];
const G_MAX: f32 = 6.0;

fn decode_to_raw(src: &str, dst: &std::path::Path) -> bool {
    std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i", src, "-ar", "48000", "-ac", "2", "-f", "f32le"])
        .arg(dst)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    assert!(!files.is_empty(), "usage: ltass_corpus <file>...");

    let prof = aether_bridge::reference_resolver::ReferenceProfile::load(
        aether_bridge::reference_resolver::ProfileId::PodcastV1,
    );
    let n = prof.normalization_band_count;

    println!("{:<42} {}", "αρχείο", "gains ανά ζώνη (★ = clamp)");
    let mut clamp_counts = [0usize; 8];
    let mut total = 0usize;
    let mut any_clamp = 0usize;

    for f in &files {
        // ΑΝΑ ΔΙΕΡΓΑΣΙΑ: το corpus τρέχει σε παράλληλες παρτίδες, κοινό
        // όνομα ⇒ δύο διεργασίες γράφουν/διαβάζουν το ίδιο dump.
        let dump = std::env::temp_dir()
            .join(format!("ltass_corpus_probe_{}.pcm", std::process::id()));
        if !decode_to_raw(f, &dump) {
            println!("{:<42} DECODE FAILED", short(f));
            continue;
        }
        let trunk = match sp314_orchestrator::trunk_pass::run_trunk_pass(&dump, false) {
            Ok(t) => t,
            Err(e) => {
                println!("{:<42} TRUNK FAILED: {e}", short(f));
                continue;
            }
        };
        let raw = trunk.spectral_profile_db;
        let mean: f32 = raw[..n].iter().sum::<f32>() / n as f32;
        let normalized: [f32; 8] = std::array::from_fn(|k| raw[k] - mean);
        let g = aether_bridge::reference_resolver::ReferenceResolver::resolve(&normalized, &prof);

        total += 1;
        let mut line = String::new();
        let mut hit = false;
        for k in 0..8 {
            let clamped = (g[k].abs() - G_MAX).abs() < 1e-3;
            if clamped {
                clamp_counts[k] += 1;
                hit = true;
            }
            line.push_str(&format!("{:>+7.2}{}", g[k], if clamped { "★" } else { " " }));
        }
        if hit {
            any_clamp += 1;
        }
        println!("{:<42} {}", short(f), line);
    }

    println!("\n=== ΣΥΝΟΨΗ ({total} αρχεία) ===");
    println!("  αρχεία με ΤΟΥΛΑΧΙΣΤΟΝ ένα clamp: {any_clamp}/{total}");
    println!("  ανά ζώνη:");
    for k in 0..8 {
        println!(
            "    [{}] {:>6.0} Hz : {}/{}",
            k, CENTERS[k], clamp_counts[k], total
        );
    }
}

fn short(p: &str) -> String {
    let b = std::path::Path::new(p)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if b.len() > 40 { b[..40].to_string() } else { b }
}
