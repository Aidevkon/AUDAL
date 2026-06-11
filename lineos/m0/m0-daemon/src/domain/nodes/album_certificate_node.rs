//! album_certificate_node — single cryptographic proof for entire album.
//! Aggregates N track StoredBlobs into one AlbumCertificate.
//! Authority: aether-black-spec-v1_0.md AB-P7

use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use crate::blob_store::StoredBlob;

/// Single cryptographic certificate for an entire album.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumCertificate {
    pub album_id:        String,
    pub track_count:     usize,
    pub anchor_track_idx: usize,
    pub anchor_lufs:     f32,
    pub album_hash:      String,  // SHA-256 of all track hashes
    pub track_blob_ids:  Vec<String>,
    pub track_lufs:      Vec<f32>,
    pub ear_fatigue_applied: Vec<bool>,
    pub created_at:      String,
    pub pipeline_version: String,
}

impl AlbumCertificate {
    pub fn from_tracks(
        album_id:    &str,
        blobs:       &[StoredBlob],
        anchor_idx:  usize,
        fatigue_map: &[bool],
    ) -> Self {
        // Album hash = SHA-256 of all individual track input hashes
        let mut hasher = Sha256::new();
        for blob in blobs {
            hasher.update(blob.input_hash.as_bytes());
        }
        let album_hash = format!("{:x}", hasher.finalize());

        let anchor_lufs = blobs.get(anchor_idx)
            .map(|b| b.loudness.integrated_lufs)
            .unwrap_or(-14.0);

        let track_lufs: Vec<f32> = blobs.iter()
            .map(|b| b.loudness.integrated_lufs)
            .collect();

        let track_blob_ids: Vec<String> = blobs.iter()
            .map(|b| b.id.clone())
            .collect();

        let ear_fatigue_applied = if fatigue_map.len() == blobs.len() {
            fatigue_map.to_vec()
        } else {
            vec![false; blobs.len()]
        };

        AlbumCertificate {
            album_id:             album_id.to_string(),
            track_count:          blobs.len(),
            anchor_track_idx:     anchor_idx,
            anchor_lufs,
            album_hash,
            track_blob_ids,
            track_lufs,
            ear_fatigue_applied,
            created_at:           chrono::Utc::now().to_rfc3339(),
            pipeline_version:     env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Write album certificate to disk as JSON.
    pub fn write_to_disk(&self, album_id: &str) -> Option<String> {
        let path = format!("album_{}.certificate.json", &album_id[..album_id.len().min(8)]);
        if let Ok(json) = serde_json::to_string_pretty(self) {
            std::fs::write(&path, json).ok()?;
            Some(path)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_blob(id: &str, input_hash: &str, lufs: f32) -> StoredBlob {
        StoredBlob {
            id: id.to_string(),
            version: "1.0".to_string(),
            blob_type: "audio".to_string(),
            created_at: "now".to_string(),
            input_hash: input_hash.to_string(),
            seed: 0,
            pipeline_version: "v1".to_string(),
            preset_id: "preset".to_string(),
            loudness: crate::blob_store::StoredLoudness {
                integrated_lufs: lufs,
                short_term_lufs: 0.0,
                momentary_lufs: 0.0,
                true_peak_dbtp: 0.0,
                lra: 0.0,
                k_weighted: false,
                ebu_r128_target_lufs: 0.0,
                ebu_r128_compliant: false,
                spotify_compliant: false,
                youtube_compliant: false,
                apple_music_compliant: false,
                apple_podcasts_compliant: false,
                broadcast_compliant: false,
                tidal_compliant: false,
            },
            quality: crate::blob_store::StoredQuality {
                stereo_correlation: 0.0,
                phase_coherence: 0.0,
                stereo_width: 0.0,
                dynamic_range_db: 0.0,
                rms_db: 0.0,
                spectral_centroid: 0.0,
                spectral_flatness: 0.0,
                clips_detected: 0,
                clip_free: false,
            },
            provenance: crate::blob_store::StoredProvenance {
                engine_id: "".to_string(),
                engine_version: "".to_string(),
                processing_time_ms: 0,
                host_os: "".to_string(),
                created_by: "".to_string(),
                aether_enriched: false,
                aether_devices: vec![],
            },
            schema_version: 1,
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            stem_fingerprints: None,
            qr_base64: None,
            pcm_blake3: None,
            cert_signature: None,
            processing_timeline: vec![],
            audio_path: std::path::PathBuf::new(),
            sample_rate: 48000,
            channels: 2,
            num_frames: 0,
        }
    }

    #[test]
    fn album_hash_is_deterministic() {
        let blobs = vec![
            make_blob("b1", "hash1", -14.0),
            make_blob("b2", "hash2", -8.0),
            make_blob("b3", "hash3", -16.0),
        ];
        let cert1 = AlbumCertificate::from_tracks("album1", &blobs, 1, &[false, false, false]);
        let cert2 = AlbumCertificate::from_tracks("album1", &blobs, 1, &[false, false, false]);
        assert_eq!(cert1.album_hash, cert2.album_hash,
            "INV-AB-1: same inputs → same hash");
    }

    #[test]
    fn anchor_lufs_correct() {
        let blobs = vec![
            make_blob("b1", "h1", -14.0),
            make_blob("b2", "h2", -8.0),
        ];
        let cert = AlbumCertificate::from_tracks("album1", &blobs, 1, &[false, false]);
        assert!((cert.anchor_lufs - (-8.0)).abs() < 0.001);
        assert_eq!(cert.anchor_track_idx, 1);
    }

    #[test]
    fn track_count_matches() {
        let blobs = vec![
            make_blob("b1", "h1", -14.0),
            make_blob("b2", "h2", -10.0),
            make_blob("b3", "h3", -18.0),
        ];
        let cert = AlbumCertificate::from_tracks("album1", &blobs, 0, &[false, true, false]);
        assert_eq!(cert.track_count, 3);
        assert_eq!(cert.ear_fatigue_applied, vec![false, true, false]);
    }
}
