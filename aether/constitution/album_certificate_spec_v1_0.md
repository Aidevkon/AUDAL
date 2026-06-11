# Aether Black: Album Certificate Specification (v1.0)

## 1. Core Philosophy

Traditional DAWs treat album mastering as a fragile, monolithic session file (e.g., `.sesx`, `.ptx`). These files are heavily coupled to specific environments, plugin versions, and filesystem structures. When a mastering engineer attempts to restore an album session six months later, missing plugins, updated dependencies, or lost automation curves frequently result in an inability to reproduce the original state exactly. This destroys verifiable quality and forces tedious, manual re-work.

CreatorOS introduces the **AlbumCertificate**: a cryptographic "Mastering Passport." Instead of saving a bloated, environment-dependent DAW session, CreatorOS computes a deterministic, zero-allocation cryptographic proof of the album's exact state. The `AlbumCertificate` acts as an immutable blueprint. It guarantees that any future reconstruction of the album—or any subsequent non-destructive edits to individual tracks—will perfectly align with the original dynamic relationships, Anchor LUFS, and `EarFatigue` mitigation parameters, all without requiring a full re-analysis of the album. State Recovery becomes a mathematical certainty, not a fragile reconstruction effort.

## 2. The Schema

The `AlbumCertificate` is represented as a single `JSON` payload generated from the strict Rust structure below. It aggregates `N` track `StoredBlob`s into a single cohesive entity.

```rust
use serde::{Serialize, Deserialize};

/// Single cryptographic certificate for an entire album.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumCertificate {
    /// Unique identifier for the album session
    pub album_id: String,
    
    /// Total number of tracks in the album (N)
    pub track_count: usize,
    
    /// The index [0, N-1] of the track acting as the dynamic anchor
    pub anchor_track_idx: usize,
    
    /// The integrated LUFS of the anchor track (baseline for relative offsets)
    pub anchor_lufs: f32,
    
    /// Cryptographic SHA-256 hash of all individual track input_hashes
    pub album_hash: String,
    
    /// Ordered list of `StoredBlob` UUIDs corresponding to each track
    pub track_blob_ids: Vec<String>,
    
    /// Ordered list of integrated LUFS values for all N tracks
    pub track_lufs: Vec<f32>,
    
    /// Ordered boolean map indicating if EarFatigue TTS recovery was applied
    pub ear_fatigue_applied: Vec<bool>,
    
    /// ISO 8601 UTC timestamp of certificate generation
    pub created_at: String,
    
    /// Exact version of the DSP mastering pipeline used (e.g., "0.4.0")
    pub pipeline_version: String,
}
```

## 3. The "Hash of Hashes" Cryptography

To satisfy the invariant **INV-AB-1** (*same inputs → same output plan, always*), the `album_hash` must serve as an absolute, deterministic proof of the album's source material and sequence.

The `album_hash` is computed as a SHA-256 "Hash of Hashes":
1. Each individual track is hashed upon entry into the pipeline, yielding an `input_hash` (e.g., via Blake3).
2. The `AlbumConductor` orchestrator sequentially feeds these `input_hash` bytes into a new SHA-256 digest in exact track order.
3. The resulting 256-bit hash becomes the `album_hash`.

$$ \text{AlbumHash} = \text{SHA256}(\text{input\_hash}_0 \parallel \text{input\_hash}_1 \parallel \dots \parallel \text{input\_hash}_{N-1}) $$

### Enforcing INV-AB-1
By cryptographically binding the sequence and content of all tracks, the `album_hash` ensures that if a single sample changes in Track 3, or if the track order is modified, the `album_hash` immediately invalidates. This provides total assurance that the `anchor_lufs` and `ear_fatigue_applied` map stored in the certificate correspond exactly to the provided source audio sequence.

## 4. State Recovery Workflow

The `AlbumCertificate` decouples inter-track dependencies. This enables highly efficient partial re-renders. Consider a scenario where a user returns six months later to modify ONLY Track 2 of a 10-track album (e.g., a minor vocal EQ adjustment). 

Instead of re-analyzing the entire album, the pipeline leverages the `AlbumCertificate` to perform a targeted update:

1. **Verify Originality**: The system checks the `input_hash` of all unmodified tracks against the `track_blob_ids` to ensure no silent corruption occurred.
2. **Anchor Preservation**: The orchestrator reads the `anchor_track_idx` and `anchor_lufs` from the certificate. Even if Track 2's LUFS changes slightly due to the EQ edit, the original album anchor remains intact, preserving the global macro-dynamic arc.
3. **Offset Recomputation**: The system re-analyzes only the new Track 2. It calculates Track 2's new offset relative to the locked `anchor_lufs`.
4. **Fatigue & Morph Application**: The orchestrator reads `ear_fatigue_applied`. If Track 1 triggered fatigue, Track 2 still receives its correct TTS recovery parameters without requiring a re-analysis of Track 1. Similarly, MorphCurve states between Track 1 -> 2 and Track 2 -> 3 are perfectly re-calculated based on the known boundary states.
5. **Certificate Re-issuance**: A new `AlbumCertificate` is issued with an updated `album_hash` and a new `StoredBlob` ID for Track 2, maintaining an immutable ledger of mastering revisions.

By caching the collective state in the `AlbumCertificate`, the system achieves true zero-allocation recovery, bypassing expensive global cohesion pre-passes while guaranteeing perfect album continuity.
