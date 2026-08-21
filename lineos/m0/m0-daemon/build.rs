//! Εμφυτεύει το git hash του HEAD στο binary, ως GIT_HASH.
//!
//! ΓΙΑΤΙ ΥΠΑΡΧΕΙ: το certificate sidecar δηλώνει ΠΟΙΟΣ έγραψε την
//! απόδειξη. Το runtime δεν ξέρει git — ο build ξέρει.
//!
//! ΟΡΑΤΟ FALLBACK: αν το build γίνει εκτός repo (tarball, docker
//! COPY χωρίς .git), η τιμή είναι "unknown" — ΟΧΙ κενό. Ένα κενό
//! πεδίο διαβάζεται ως «δεν χρειαζόταν»· το "unknown" διαβάζεται
//! ως «δεν ξέραμε», που είναι η αλήθεια.
//!
//! ⚠ §Ρ: το GIT_HASH είναι σταθερά μέσα στο binary, άρα τα bytes
//! του binary αλλάζουν σε ΚΑΘΕ commit. Αυτό είναι το ζητούμενο για
//! provenance και ΔΕΝ αγγίζει το INV-DET-1, που συγκρίνει δύο
//! renders από το ΙΔΙΟ binary. Το «bit-exact across commits» δεν
//! ήταν ποτέ υπόσχεση.

fn main() {
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let rustc_ver = std::process::Command::new("rustc")
        .arg("-V")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let target_triple = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    let target_arch =
        std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "unknown".to_string());
    let target_os =
        std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "unknown".to_string());
    let target_env =
        std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_else(|_| "unknown".to_string());

    // Declared build profile parameters (Cargo.toml §Ρ)
    let opt_level = std::env::var("OPT_LEVEL").unwrap_or_else(|_| "unknown".to_string());
    let codegen_units =
        std::env::var("CODEGEN_UNITS").unwrap_or_else(|_| "unknown".to_string());

    let target_cpu = parse_target_cpu();

    println!("cargo:warning=TARGET_CPU={target_cpu}");
    println!("cargo:rustc-env=GIT_HASH={hash}");
    println!("cargo:rustc-env=RUSTC_VERSION={rustc_ver}");
    println!("cargo:rustc-env=TARGET_TRIPLE={target_triple}");
    println!("cargo:rustc-env=TARGET_ARCH={target_arch}");
    println!("cargo:rustc-env=TARGET_OS={target_os}");
    println!("cargo:rustc-env=TARGET_ENV={target_env}");
    println!("cargo:rustc-env=TARGET_CPU={target_cpu}");
    println!("cargo:rustc-env=PROFILE_OPT_LEVEL={opt_level}");
    println!("cargo:rustc-env=PROFILE_CODEGEN_UNITS={codegen_units}");

    println!("cargo:rerun-if-changed=../../../.git/HEAD");

    if let Ok(head_content) = std::fs::read_to_string("../../../.git/HEAD") {
        if let Some(ref_path) = head_content.trim().strip_prefix("ref: ") {
            println!("cargo:rerun-if-changed=../../../.git/{ref_path}");
        }
    }
}

fn parse_target_cpu() -> String {
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    println!("cargo:rerun-if-env-changed=RUSTFLAGS");

    let flags: Vec<String> = if let Ok(encoded) = std::env::var("CARGO_ENCODED_RUSTFLAGS") {
        encoded.split('\x1f').map(|s| s.to_string()).collect()
    } else if let Ok(plain) = std::env::var("RUSTFLAGS") {
        plain.split_whitespace().map(|s| s.to_string()).collect()
    } else {
        vec![]
    };

    for window in flags.windows(2) {
        if window[0] == "-C" {
            if let Some(cpu) = window[1].strip_prefix("target-cpu=") {
                return cpu.trim().to_string();
            }
        }
    }
    for flag in &flags {
        if let Some(cpu) = flag.strip_prefix("target-cpu=") {
            return cpu.trim().to_string();
        }
        if let Some(cpu) = flag.strip_prefix("-Ctarget-cpu=") {
            return cpu.trim().to_string();
        }
    }

    "default(unset)".to_string()
}
