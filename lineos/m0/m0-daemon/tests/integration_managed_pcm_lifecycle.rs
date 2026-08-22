//! F-050(γ) lifecycle oracle — proves ManagedPcm RAII coordination at runtime.
//!
//! Two tests:
//! 1. `eviction_drops_the_file`: eviction is the last Arc holder → file deleted.
//! 2. `shared_ref_outlives_eviction`: a second Arc (simulating xaak's kernel
//!    guard) keeps the file alive through eviction; it dies only when the
//!    clone drops — the RAII coordination proof.
//!
//! Files use unique /tmp names (m0d-test-lifecycle-{uuid}) so the startup
//! sweep's "m0d-" prefix match is irrelevant and parallel runs never collide.

use m0d::blob_store::{BlobStore, StoredBlobV2, StoredBlobCore, BlobVariant};
use std::sync::Arc;

/// BlobStore::MAX_BLOBS is private; mirror it here.
const MAX_BLOBS: usize = 16;

/// Build a minimal StoredBlob holding the given audio_path Arc and a
/// distinguishable id + created_at.
fn stub_blob_with_path(
    id: &str,
    created_at: &str,
    audio_path: Arc<lineos_types::audio::ManagedPcm>,
) -> StoredBlobV2 {
    StoredBlobV2 {
        core: StoredBlobCore {
            id: id.to_string(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: created_at.to_string(),
            input_path_hash: "aabbccdd".into(),
            input_pcm_sha256: Some("pcm-aabbccdd".into()),
            seed: 1,
            pipeline_version: "0.4.0".into(),
            schema_version: 1,
            preset_id: "spotify".into(),
            pcm_blake3: None,
            cert_signature: None,
            audio_path,
            sample_rate: 48000,
            channels: 2,
            num_frames: 48000,
        },
        variant: BlobVariant::Certified {
            loudness: Default::default(),
            quality: Default::default(),
            provenance: Default::default(),
            spatial: Default::default(),
            stem_fingerprints: None,
            processing_timeline: vec![],
            dead_air: Default::default(),
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            qr_base64: None,
        }
    }
}

/// Create a real temp file with some bytes, return (path, Arc<ManagedPcm>).
fn create_real_temp_file() -> (std::path::PathBuf, Arc<lineos_types::audio::ManagedPcm>) {
    let id = uuid::Uuid::new_v4();
    let path = std::path::PathBuf::from(format!("/tmp/m0d-test-lifecycle-{}", id));
    std::fs::write(&path, b"lifecycle-oracle-payload").expect("write temp file");
    assert!(path.exists(), "temp file must exist after write");
    let guard = Arc::new(lineos_types::audio::ManagedPcm::new(path.clone()));
    (path, guard)
}

/// Eviction is the last Arc holder → file deleted.
///
/// 1. Create a real temp file, wrap in Arc<ManagedPcm>.
/// 2. Insert into BlobStore as the oldest blob (created_at = T0).
/// 3. Insert MAX_BLOBS more stubs with ascending created_at (T1..T16)
///    to force eviction of the first.
/// 4. Assert the file no longer exists on disk.
#[test]
fn eviction_drops_the_file() {
    let (path, guard) = create_real_temp_file();
    let store = BlobStore::new();

    // Insert the target blob as the oldest (T0).
    store.insert(stub_blob_with_path("victim", "2026-01-01T00:00:00Z", guard));

    // Insert MAX_BLOBS more with newer timestamps to force eviction.
    for i in 0..MAX_BLOBS {
        let id = format!("filler-{}", i);
        let ts = format!("2026-01-01T00:00:{:02}Z", i + 1);
        store.insert(stub_blob_with_path(
            &id,
            &ts,
            Default::default(), // no real file — ManagedPcm::default()
        ));
    }

    // The victim has been evicted; its Arc was the ONLY holder → Drop fired.
    assert!(
        !path.exists(),
        "F-050 oracle: evicted blob's file must be deleted (RAII Drop)"
    );
}

/// A shared Arc (simulating xaak's kernel guard) keeps the file alive
/// through eviction; it dies only when the clone drops.
///
/// 1. Create a real temp file, wrap in Arc<ManagedPcm>.
/// 2. Clone the Arc (simulating XaakKernel holding the guard beside its mmap).
/// 3. Insert the blob into BlobStore, then force eviction.
/// 4. Assert the file STILL EXISTS — the clone's Arc keeps it alive.
/// 5. Drop the clone → assert file is gone (last ref out, file deleted).
#[test]
fn shared_ref_outlives_eviction() {
    let (path, guard) = create_real_temp_file();
    let store = BlobStore::new();

    // Simulate xaak's kernel taking its own Arc reference.
    let kernel_guard = Arc::clone(&guard);

    // Insert the target blob as the oldest (T0).
    store.insert(stub_blob_with_path("victim", "2026-01-01T00:00:00Z", guard));

    // Force eviction: insert MAX_BLOBS more with newer timestamps.
    for i in 0..MAX_BLOBS {
        let id = format!("filler-{}", i);
        let ts = format!("2026-01-01T00:00:{:02}Z", i + 1);
        store.insert(stub_blob_with_path(&id, &ts, Default::default()));
    }

    // Blob evicted from store, BUT the kernel still holds an Arc.
    // Arc refcount = 1 (kernel_guard) — file MUST still exist.
    assert!(
        path.exists(),
        "F-050 oracle: file must survive eviction when a shared Arc (xaak kernel) holds it"
    );

    // Now drop the kernel's reference — last Arc out → Drop fires → file gone.
    drop(kernel_guard);

    assert!(
        !path.exists(),
        "F-050 oracle: file must be deleted when the last Arc drops (RAII coordination proof)"
    );
}
