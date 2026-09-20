//! Genuine local helpers `execute_streaming_plan` needs from
//! m0-daemon's domain/dsp_pipeline.rs and handlers/certificate.rs —
//! moved here 21/09, verbatim, zero other caller for either origin
//! function's own name. `dsp_pipeline.rs`'s own `run_dsp_internal`
//! (staying) calls `compute_sha256_bytes`/`derive_seed` via full path.

/// SHA-256 of input bytes — returns [u8; 32].
/// Used for both `input_path_sha256` audit field and determinism `seed`.
pub fn compute_sha256_bytes(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

/// Derive u64 seed from first 8 bytes of hash (big-endian).
pub fn derive_seed(hash: &[u8; 32]) -> u64 {
    u64::from_be_bytes(hash[..8].try_into().unwrap_or([0; 8]))
}

pub fn map_flavour_to_persona(flavour_id: &str) -> &'static str {
    match flavour_id {
        "warm" => "warm_analog",
        "clean" => "clean_punch",
        "punch" => "clean_punch",
        "air" => "hybrid_hifi",
        "film" => "cinematic_wide",
        "broadcast" => "clean_punch",
        _ => "warm_analog", // default
    }
}

/// Moved with `TimelineProfiler`/`certificate_node::run` — both call
/// it, and handlers/certificate.rs (staying, per F-137) has zero
/// other caller for it, so it is not re-exported from there.
pub fn blake3_pcm(pcm: &[f32]) -> String {
    let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    blake3::hash(&bytes).to_hex().to_string()
}
