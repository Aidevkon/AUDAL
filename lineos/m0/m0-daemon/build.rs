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

    println!("cargo:rustc-env=GIT_HASH={hash}");
    println!("cargo:rerun-if-changed=../../../.git/HEAD");
}
