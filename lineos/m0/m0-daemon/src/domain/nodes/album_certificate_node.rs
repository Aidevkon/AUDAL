//! album_certificate_node — single cryptographic proof for entire album.
//! Aggregates N track StoredBlobs into one AlbumCertificate.
//! Authority: aether-black-spec-v1_0.md AB-P7

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Ό,τι ΑΚΡΙΒΩΣ χρειάζεται το album certificate από
/// κάθε track — τίποτα παραπάνω.
///
/// ΓΙΑΤΙ ΟΧΙ StoredBlob: ο conductor δεν έχει blobs.
/// Παίρνει BatchTrackOutput (7 πεδία) μέσω async
/// καναλιού και έφτιαχνε proxies με Default για να
/// ικανοποιήσει την υπογραφή. Ο τύπος ζητούσε
/// περισσότερα απ' όσα χρησιμοποιεί.
///
/// Όταν έρθει το run_batch() (northstar §Β), αυτό
/// είναι ήδη το σωστό σχήμα — δεν πετιέται.
///
/// ΑΝΟΙΧΤΟ: το AlbumCertificate κρατάει
/// ear_fatigue_applied: Vec<bool> — ΟΤΙ εφαρμόστηκε,
/// όχι ΠΟΣΟ. Οι πραγματικοί multipliers του
/// EarFatigueDelta πετιούνται στον conductor
/// (conductor.rs:476-484), άρα ένα album ΔΕΝ
/// αναπαράγεται από το certificate του.
///
/// Το TrackContext του northstar (Δόγμα Α, ΠΛΑΙΣΙΟ)
/// θέλει τις τιμές. ΔΕΝ τις προσθέτουμε εδώ σήμερα:
/// ο conductor δεν έχει τι να βάλει, και δύο πεδία
/// που μένουν πάντα None είναι ακριβώς η παθογένεια
/// που καθαρίζουμε.
///
/// ΣΒΗΝΕΙ όταν το run_batch() μεταφέρει αυτούσιο το
/// EarFatigueDelta. northstar §Β.
#[derive(Debug, Clone)]
pub struct TrackSummary {
    pub blob_id: String,
    pub content_hash: String,
    pub integrated_lufs: f32,
}

/// Single cryptographic certificate for an entire album.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumCertificate {
    pub album_id: String,
    pub track_count: usize,
    pub anchor_track_idx: usize,
    pub anchor_lufs: f32,
    pub album_hash: String, // SHA-256 of all track hashes
    pub track_blob_ids: Vec<String>,
    pub track_lufs: Vec<f32>,
    pub ear_fatigue_applied: Vec<bool>,
    pub created_at: String,
    pub pipeline_version: String,
}

impl AlbumCertificate {
    pub fn from_tracks(
        album_id: &str,
        tracks: &[TrackSummary],
        anchor_idx: usize,
        fatigue_map: &[bool],
    ) -> Self {
        // ΑΛΛΑΓΗ ΤΙΜΗΣ 7α.2γ: ήταν sha256 πάνω στα input_hash
        // των tracks· τώρα sha256 πάνω στα pcm_blake3 τους.
        // Το album_hash δεσμεύεται πλέον στο ΠΑΡΑΓΟΜΕΝΟ audio,
        // όχι στα πηγαία αρχεία. Συνειδητή αλλαγή, όχι
        // παρενέργεια. northstar §Ρ (μορφή των hash).
        let mut hasher = Sha256::new();
        for t in tracks {
            hasher.update(t.content_hash.as_bytes());
        }
        let album_hash = format!("{:x}", hasher.finalize());

        let anchor_lufs = tracks
            .get(anchor_idx)
            .map(|t| t.integrated_lufs)
            .unwrap_or(-14.0);

        let track_lufs: Vec<f32> = tracks
            .iter()
            .map(|t| t.integrated_lufs)
            .collect();

        let track_blob_ids: Vec<String> = tracks.iter().map(|t| t.blob_id.clone()).collect();

        let ear_fatigue_applied = if fatigue_map.len() == tracks.len() {
            fatigue_map.to_vec()
        } else {
            vec![false; tracks.len()]
        };

        AlbumCertificate {
            album_id: album_id.to_string(),
            track_count: tracks.len(),
            anchor_track_idx: anchor_idx,
            anchor_lufs,
            album_hash,
            track_blob_ids,
            track_lufs,
            ear_fatigue_applied,
            created_at: chrono::Utc::now().to_rfc3339(),
            pipeline_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Write album certificate to disk as JSON.
    pub fn write_to_disk(&self, album_id: &str, certs_dir: &str) -> Option<String> {
        std::fs::create_dir_all(certs_dir).unwrap_or_default();

        let path = format!(
            "{}/album_{}.certificate.json",
            certs_dir,
            &album_id[..album_id.len().min(8)]
        );
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

    fn make_track_summary(id: &str, input_hash: &str, lufs: f32) -> TrackSummary {
        TrackSummary {
            blob_id: id.to_string(),
            content_hash: input_hash.to_string(),
            integrated_lufs: lufs,
        }
    }

    #[test]
    fn album_hash_is_deterministic() {
        let tracks = vec![
            make_track_summary("b1", "hash1", -14.0),
            make_track_summary("b2", "hash2", -8.0),
            make_track_summary("b3", "hash3", -16.0),
        ];
        let cert1 = AlbumCertificate::from_tracks("album1", &tracks, 1, &[false, false, false]);
        let cert2 = AlbumCertificate::from_tracks("album1", &tracks, 1, &[false, false, false]);
        assert_eq!(
            cert1.album_hash, cert2.album_hash,
            "INV-AB-1: same inputs → same hash"
        );
    }

    #[test]
    fn anchor_lufs_correct() {
        let tracks = vec![make_track_summary("b1", "h1", -14.0), make_track_summary("b2", "h2", -8.0)];
        let cert = AlbumCertificate::from_tracks("album1", &tracks, 1, &[false, false]);
        assert!((cert.anchor_lufs - (-8.0)).abs() < 0.001);
        assert_eq!(cert.anchor_track_idx, 1);
    }

    #[test]
    fn track_count_matches() {
        let tracks = vec![
            make_track_summary("b1", "h1", -14.0),
            make_track_summary("b2", "h2", -10.0),
            make_track_summary("b3", "h3", -18.0),
        ];
        let cert = AlbumCertificate::from_tracks("album1", &tracks, 0, &[false, true, false]);
        assert_eq!(cert.track_count, 3);
        assert_eq!(cert.ear_fatigue_applied, vec![false, true, false]);
    }
}
