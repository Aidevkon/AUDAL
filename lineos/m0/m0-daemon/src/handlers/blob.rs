//! GET /blob/:id — fetch Golden Blob as JSON.
//! Authority: Phase 6 task-decomposition P6-003
//!
//! Returns StoredBlob as JSON. 404 if not found.
//! FORBIDDEN: Returning raw audio bytes (audio stays in M0 storage).
//! FORBIDDEN: serde_json::Value in response type.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::app_state::AppState;
use crate::blob_store::{StoredBlobV2, BlobVariant};
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RehydrateError {
    NotFound,
    Io(String),
    Corrupt(String),
}

/// §Π — Η ΑΛΥΣΙΔΑ ΑΝΑΣΤΑΣΗΣ:
/// ```text
///   RAM hit                              το φθηνό μονοπάτι
///     ↓ miss
///   δίσκος  <masters>/*/<blob_id>.json   ΤΟ ΚΟΙΝΟ μονοπάτι
///     ↓ Ok(None)
///   DB      blob_path                    ΤΕΛΕΥΤΑΙΟ καταφύγιο
///     ↓ miss
///   NotFound
/// ```
pub async fn get_or_rehydrate(
    state: &AppState,
    id: &str,
) -> Result<StoredBlobV2, RehydrateError> {
    // 1. RAM
    if let Some(blob) = state.blob_store.get(id) {
        return Ok(blob);
    }

    let masters_root = state.config.masters_path.clone();

    // 2. ΔΙΣΚΟΣ
    match crate::blob_store::find_sidecar(&masters_root, id) {
        Err(e) => {
            tracing::error!(blob_id = %id, "§Π: cannot search for certificate: {e}");
            return Err(RehydrateError::Io(e.to_string()));
        }
        Ok(Some(path)) => return load_and_cache(state, id, &path),
        Ok(None) => {}
    }

    // 3. DB — ΤΕΛΕΥΤΑΙΟ καταφύγιο
    let sql = "SELECT blob_path FROM tracks WHERE blob_id = $blob_id LIMIT 1";
    let db_path: Option<String> = match state.db.query(sql).bind(("blob_id", id.to_string())).await {
        Ok(mut r) => match r.take::<Option<String>>((0, "blob_path")) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(blob_id = %id, "§Π: db blob_path unreadable: {e}");
                return Err(RehydrateError::Corrupt(e.to_string()));
            }
        },
        Err(e) => {
            tracing::error!(blob_id = %id, "§Π: db lookup failed: {e}");
            return Err(RehydrateError::Io(e.to_string()));
        }
    };

    match db_path {
        Some(p) => load_and_cache(state, id, std::path::Path::new(&p)),
        // 4. Πουθενά. ΑΥΤΟ είναι το θεμιτό NotFound.
        None => Err(RehydrateError::NotFound),
    }
}

/// GET /blob/:id — return Golden Blob metrics as JSON.
/// Audio bytes are NOT returned — Cockpit receives metrics only.
pub async fn get_blob(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BlobResponse>, StatusCode> {
    match get_or_rehydrate(&state, &id).await {
        Ok(blob) => Ok(Json(blob.into())),
        Err(RehydrateError::NotFound) => Err(StatusCode::NOT_FOUND),
        Err(RehydrateError::Io(_)) | Err(RehydrateError::Corrupt(_)) => {
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Διαβάζει, επαληθεύει ΤΑΥΤΟΤΗΤΑ, και ΓΕΜΙΖΕΙ ΤΟ RAM ώστε το επόμενο
/// hit να είναι φθηνό.
fn load_and_cache(
    state: &AppState,
    id: &str,
    path: &std::path::Path,
) -> Result<StoredBlobV2, RehydrateError> {
    match crate::blob_store::read_sidecar(path) {
        Ok(blob) => {
            if blob.core.id != id {
                tracing::error!(
                    requested = %id,
                    found = %blob.core.id,
                    path = %path.display(),
                    "§Π: certificate identity mismatch — the file answers a different question"
                );
                return Err(RehydrateError::Corrupt(format!(
                    "certificate identity mismatch: requested {id}, found {}",
                    blob.core.id
                )));
            }
            state.blob_store.insert(blob.clone());
            tracing::info!(
                blob_id = %id,
                path = %path.display(),
                "§Π: certificate restored from disk"
            );
            Ok(blob)
        }
        Err(e) => {
            tracing::error!(
                blob_id = %id,
                path = %path.display(),
                "§Π: certificate unreadable: {e}"
            );
            Err(RehydrateError::Io(e.to_string()))
        }
    }
}

/// Το seed σειριοποιείται ως string, όχι αριθμός.
/// ΜΕΤΑΚΙΝΗΘΗΚΕ από το blob_store στο 7β: έγινε pub
/// στο 7α.3 για να τη δανειστεί το DTO, και μόλις
/// έφυγε ο StoredBlob έμεινε δημόσια συνάρτηση σε
/// module που δεν τη χρησιμοποιεί — διαρροή προς την
/// ΑΝΤΙΘΕΤΗ κατεύθυνση από αυτήν που χτίσαμε.
/// Ζει δίπλα στον μοναδικό της χρήστη.
fn serialize_u64_as_string<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}

/// Το σχήμα που βλέπει ο κόσμος. ΣΤΑΘΕΡΟ.
///
/// ΓΙΑΤΙ ΟΧΙ StoredBlobV2 απευθείας: θα σειριοποιούνταν
/// ως {"core":{...},"variant":{...}} με το enum να
/// προσθέτει άλλο επίπεδο. Το JSON συγχωρεί ΠΡΟΣΘΗΚΗ
/// πεδίων και τίποτα άλλο.
///
/// Ο εσωτερικός τύπος είναι ελεύθερος να αλλάξει.
/// Αυτό είναι ΣΥΜΒΑΣΗ. northstar §Σ.
///
/// ΒΑΣΗ: το ΣΗΜΕΡΙΝΟ JSON, ΟΧΙ τα πεδία του struct —
/// δεν είναι το ίδιο, λόγω των #[serde(skip)].
#[derive(Serialize)]
pub struct BlobResponse {
    // 1. ΑΝΤΙΓΡΑΦΗ (GoldenBlob v1 fields)
    pub id: String,
    pub version: String,
    #[serde(rename = "type")]
    pub blob_type: String,
    pub created_at: String,
    pub input_hash: String,
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub seed: u64,
    pub pipeline_version: String,
    pub preset_id: String,

    pub loudness: Option<crate::blob_store::StoredLoudness>,
    pub quality: Option<crate::blob_store::StoredQuality>,
    pub provenance: Option<crate::blob_store::StoredProvenance>,
    pub spatial: Option<crate::blob_store::StoredSpatial>,
    
    pub schema_version: u32,
    pub aether_cert: Option<String>,
    pub aether_persona: Option<String>,
    pub aether_config: Option<String>,

    pub stem_fingerprints: Option<crate::blob_store::StemFingerprints>,
    pub qr_base64: Option<String>,
    pub pcm_blake3: Option<String>,
    pub cert_signature: Option<String>,
    pub processing_timeline: Option<Vec<crate::blob_store::StageRecord>>,
    pub dead_air: Option<crate::dsp::signal_health::DeadAirSummary>,

    // 2. ΕΠΕΚΤΑΣΗ — τα τρία τεχνικά
    pub sample_rate: u32,
    pub channels: u16,
    pub num_frames: usize,

    // 3. ΝΕΑ — η ρητή δήλωση
    pub certified: bool,
    pub uncertified_reason: Option<&'static str>,
}

impl From<StoredBlobV2> for BlobResponse {
    fn from(v2: StoredBlobV2) -> Self {
        let (certified, uncertified_reason) = match &v2.variant {
            BlobVariant::Certified { .. } => (true, None),
            BlobVariant::Uncertified { reason } => {
                tracing::debug!(
                    blob_id = %v2.core.id,
                    internal_reason = ?reason,
                    "blob served as uncertified"
                );
                (false, Some(reason.as_public_str()))
            }
        };

        BlobResponse {
            id: v2.core.id.clone(),
            version: v2.core.version.clone(),
            blob_type: v2.core.blob_type.clone(),
            created_at: v2.core.created_at.clone(),
            input_hash: v2.core.input_hash.clone(),
            seed: v2.core.seed,
            pipeline_version: v2.core.pipeline_version.clone(),
            preset_id: v2.core.preset_id.clone(),
            
            schema_version: v2.core.schema_version,
            pcm_blake3: v2.core.pcm_blake3.clone(),
            cert_signature: v2.core.cert_signature.clone(),
            
            sample_rate: v2.core.sample_rate,
            channels: v2.core.channels,
            num_frames: v2.core.num_frames,
            
            certified,
            uncertified_reason,

            loudness: v2.loudness().cloned(),
            quality: v2.quality().cloned(),
            provenance: v2.provenance().cloned(),
            spatial: v2.spatial().cloned(),
            
            aether_cert: v2.aether_cert().map(|s| s.to_string()),
            aether_persona: v2.aether_persona().map(|s| s.to_string()),
            aether_config: v2.aether_config().map(|s| s.to_string()),
            
            stem_fingerprints: v2.stem_fingerprints().cloned(),
            qr_base64: v2.qr_base64().map(|s| s.to_string()),
            
            processing_timeline: match &v2.variant {
                BlobVariant::Certified { processing_timeline, .. } => Some(processing_timeline.clone()),
                BlobVariant::Uncertified { .. } => None,
            },
            
            dead_air: match &v2.variant {
                BlobVariant::Certified { dead_air, .. } => Some(dead_air.clone()),
                BlobVariant::Uncertified { .. } => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::blob_store::{
        BlobStore, StoredBlobCore, BlobVariant, StoredLoudness, StoredProvenance, StoredQuality, StoredSpatial, UncertifiedReason, StoredBlobV2
    };
    use super::BlobResponse;


    #[test]
    fn test_blob_store_returns_none_for_unknown_id() {
        let store = BlobStore::new();
        assert!(store.get("unknown-id").is_none());
    }

    #[test]
    fn blob_response_shape_is_stable() {
        // 1. StoredBlobV2 με Certified variant
        let v2 = StoredBlobV2 {
            core: StoredBlobCore {
                id: "test-id".into(),
                version: "1.0".into(),
                blob_type: "audio".into(),
                created_at: "2026-08-11T00:00:00Z".into(),
                input_hash: "hash".into(),
                seed: 42,
                pipeline_version: "v1".into(),
                preset_id: "preset".into(),
                schema_version: 1,
                pcm_blake3: Some("blake3".into()),
                cert_signature: Some("sig".into()),
                audio_path: Default::default(),
                sample_rate: 48000,
                channels: 2,
                num_frames: 48000,
            },
            variant: BlobVariant::Certified {
                loudness: StoredLoudness::default(),
                quality: StoredQuality {
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
                provenance: StoredProvenance {
                    engine_id: "".into(),
                    engine_version: "".into(),
                    processing_time_ms: 0,
                    host_os: "".into(),
                    created_by: "".into(),
                    aether_enriched: false,
                    aether_devices: vec![],
                },
                spatial: StoredSpatial::default(),
                stem_fingerprints: None,
                processing_timeline: vec![],
                dead_air: Default::default(),
                aether_cert: None,
                aether_persona: None,
                aether_config: None,
                qr_base64: None,
            },
        };

        // 2. .into() → BlobResponse → serde_json::to_value
        let response: BlobResponse = v2.into();
        let value = serde_json::to_value(&response).unwrap();

        // 3. assert ΚΑΘΕ πεδίο υπάρχει, ΟΝΟΜΑΣΤΙΚΑ
        assert!(value.get("id").is_some());
        assert!(value.get("version").is_some());
        assert!(value.get("type").is_some());
        assert!(value.get("created_at").is_some());
        assert!(value.get("input_hash").is_some());
        assert!(value.get("seed").is_some());
        assert!(value.get("pipeline_version").is_some());
        assert!(value.get("preset_id").is_some());
        
        assert!(value.get("loudness").is_some());
        assert!(value.get("quality").is_some());
        assert!(value.get("provenance").is_some());
        assert!(value.get("spatial").is_some());
        
        assert!(value.get("schema_version").is_some());
        assert!(value.get("aether_cert").is_some());
        assert!(value.get("aether_persona").is_some());
        assert!(value.get("aether_config").is_some());
        
        assert!(value.get("stem_fingerprints").is_some());
        assert!(value.get("qr_base64").is_some());
        assert!(value.get("pcm_blake3").is_some());
        assert!(value.get("cert_signature").is_some());
        
        assert!(value.get("processing_timeline").is_some());
        assert!(value.get("dead_air").is_some());
        
        assert!(value.get("sample_rate").is_some());
        assert!(value.get("channels").is_some());
        assert!(value.get("num_frames").is_some());

        assert!(value.get("certified").is_some());
        assert!(value.get("uncertified_reason").is_some());

        // 4. certified == true, uncertified_reason == null
        assert_eq!(value["certified"], true);
        assert!(value["uncertified_reason"].is_null());

        // 5. ΔΕΝ υπάρχει κλειδί "audio_path"
        assert!(value.get("audio_path").is_none());

        // 6. ΔΕΝ υπάρχει κλειδί "core" ΟΥΤΕ "variant"
        assert!(value.get("core").is_none());
        assert!(value.get("variant").is_none());
    }

    #[test]
    fn blob_response_uncertified_declares_itself() {
        let v2 = StoredBlobV2 {
            core: StoredBlobCore {
                id: "test-id".into(),
                version: "1.0".into(),
                blob_type: "audio".into(),
                created_at: "2026-08-11T00:00:00Z".into(),
                input_hash: "hash".into(),
                seed: 42,
                pipeline_version: "v1".into(),
                preset_id: "preset".into(),
                schema_version: 1,
                pcm_blake3: None,
                cert_signature: None,
                audio_path: Default::default(),
                sample_rate: 48000,
                channels: 2,
                num_frames: 48000,
            },
            variant: BlobVariant::Uncertified {
                reason: UncertifiedReason::TransportOnlyNotASource,
            },
        };

        let response: BlobResponse = v2.into();
        let value = serde_json::to_value(&response).unwrap();

        assert_eq!(value["certified"], false);
        assert_eq!(value["uncertified_reason"], "not_a_certificate");
        assert!(value["loudness"].is_null());
        assert!(value["quality"].is_null());
    }
}
