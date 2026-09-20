//! certificate_node.rs (moved here 21/09, F-137) reads TARGET_TRIPLE/
//! TARGET_ARCH/TARGET_OS/TARGET_ENV/TARGET_CPU/RUSTC_VERSION/
//! PROFILE_OPT_LEVEL/PROFILE_CODEGEN_UNITS via env!() at compile time —
//! these are per-crate (set by each crate's own build.rs), so moving
//! the file did not carry m0-daemon's values with it. Same logic,
//! copied verbatim from m0-daemon/build.rs; GIT_HASH is not needed
//! here (StoredProvenance doesn't read it in this file).

fn main() {
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

    println!("cargo:rustc-env=RUSTC_VERSION={rustc_ver}");
    println!("cargo:rustc-env=TARGET_TRIPLE={target_triple}");
    println!("cargo:rustc-env=TARGET_ARCH={target_arch}");
    println!("cargo:rustc-env=TARGET_OS={target_os}");
    println!("cargo:rustc-env=TARGET_ENV={target_env}");
    println!("cargo:rustc-env=TARGET_CPU={target_cpu}");
    println!("cargo:rustc-env=PROFILE_OPT_LEVEL={opt_level}");
    println!("cargo:rustc-env=PROFILE_CODEGEN_UNITS={codegen_units}");
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
