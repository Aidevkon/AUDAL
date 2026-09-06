//! lame-sys build.rs — LGPL-compliant dynamic linking to system libmp3lame.
//!
//! Authority: docs/licenses/LAME-LGPL-NOTICE.md §1 · Creator OS Constitution v2.6 §07.3
//!
//! Linking strategy:
//!   1. Try pkg-config (prefers libmp3lame-dev package).
//!   2. Fall back to direct rustc-link-lib=dylib=mp3lame (works when .so.0 is
//!      present but libmp3lame-dev is not installed — common on runtime-only setups).
//!
//! NEVER emits `rustc-link-lib=static=mp3lame` — that would be an LGPL violation.

fn main() {
    // Skip on docs.rs
    if std::env::var("DOCS_RS").map(|v| v == "1").unwrap_or(false) {
        return;
    }

    // Try pkg-config first (requires libmp3lame-dev, provides .pc file)
    let found_via_pkg = pkg_config::Config::new()
        .atleast_version("3.100")
        .probe("mp3lame")
        .is_ok();

    if !found_via_pkg {
        // Fallback: system .so is present but no .pc file.
        // Direct dynamic link — LGPL compliant (replaceability preserved).
        // Verified: /lib/x86_64-linux-gnu/libmp3lame.so.0 exists.
        println!("cargo:warning=lame-sys: pkg-config mp3lame not found, falling back to direct dynamic link");
        println!("cargo:rustc-link-lib=dylib=mp3lame");
    }
    // If pkg-config succeeded, it already emitted the correct link directives.

    // IMPORTANT: never emit rustc-link-lib=static=mp3lame here.
}
