use crate::app_state::AppState;
use crate::blob_store::{
    StoredBlob, StoredLoudness, StoredProvenance, StoredQuality, StoredSpatial,
};
use crate::db::schema::Track;
use crate::dsp::signal_health::DeadAirSummary;
use crate::handlers::export::export_mp3_acx;
use axum::{
    extract::{Path, State},
    Json,
};
use lineos_types::audio::ManagedPcm;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct DeliverRequest {
    pub output_dir: Option<String>,
    pub book_title: String,
    pub entries: Vec<DeliverEntry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeliverEntry {
    pub track_id: String,
    pub index: u32,
    pub title: String,
    pub role: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PlanEntry {
    pub index: u32,
    pub title: String,
    pub role: String,
    pub filename: String,
    pub duration_ms: u64,
    pub exists: bool,
    #[serde(skip)]
    pub track_id: String,
    #[serde(skip)]
    pub audio_path: String,
}

#[derive(Debug, Serialize)]
pub struct DeliveryPlan {
    pub book_title_sanitized: String,
    pub entries: Vec<PlanEntry>,
}

#[derive(Debug, Serialize)]
pub struct DeliverResponse {
    pub book_dir: Option<String>,
    pub files: Vec<String>,
    pub manifest_path: Option<String>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// Plan endpoint only: the per-entry preview (index, title, role,
    /// filename, duration, exists). None on actual delivery responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<PlanEntry>>,
}

pub fn sanitize_title(title: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = false;
    for c in title.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
            last_was_dash = false;
        } else if c == ' ' && !last_was_dash {
            out.push('_');
            last_was_dash = true;
        }
    }
    // collapse consecutive underscores and dashes
    let mut collapsed = String::new();
    let mut last_char = '\0';
    for c in out.chars() {
        if (c == '_' || c == '-') && c == last_char {
            continue;
        }
        collapsed.push(c);
        last_char = c;
    }
    collapsed.trim_matches('_').to_string()
}

pub fn validate_and_plan(
    req: &DeliverRequest,
    db_tracks: &[Track],
) -> Result<DeliveryPlan, Vec<String>> {
    let mut errors = Vec::new();

    let safe_book_title = sanitize_title(&req.book_title);
    if safe_book_title.is_empty() {
        errors.push("book_title is empty after sanitization".into());
    }

    if req.entries.is_empty() {
        errors.push("no entries provided".into());
        return Err(errors);
    }

    let mut sorted_entries = req.entries.clone();
    sorted_entries.sort_by_key(|e| e.index);

    let max_index = sorted_entries.last().unwrap().index;
    let pad_width = if max_index > 99 { 3 } else { 2 };

    let mut seen_indices = HashSet::new();
    let mut has_opening = false;
    let mut has_closing = false;
    let mut has_retail = false;
    let mut chapter_count = 0;

    let track_map: HashMap<&str, &Track> = db_tracks
        .iter()
        .filter_map(|t| t.id.as_deref().map(|id| (id, t)))
        .collect();

    let mut plan_entries = Vec::new();

    for (i, entry) in sorted_entries.iter().enumerate() {
        let expected_index = (i + 1) as u32;
        if entry.index != expected_index {
            errors.push(format!(
                "indices must be contiguous starting at 1. Expected {}, found {}",
                expected_index, entry.index
            ));
        }
        if !seen_indices.insert(entry.index) {
            errors.push(format!("duplicate index {}", entry.index));
        }

        match entry.role.as_str() {
            "opening_credits" => {
                if has_opening {
                    errors.push("at most one opening_credits allowed".into());
                }
                has_opening = true;
            }
            "closing_credits" => {
                if has_closing {
                    errors.push("at most one closing_credits allowed".into());
                }
                has_closing = true;
            }
            "retail_sample" => {
                if has_retail {
                    errors.push("at most one retail_sample allowed".into());
                }
                has_retail = true;
            }
            "chapter" => {
                chapter_count += 1;
            }
            other => errors.push(format!("invalid role '{}'", other)),
        }

        let safe_title = sanitize_title(&entry.title);
        if safe_title.is_empty() {
            errors.push(format!(
                "title for index {} is empty after sanitization",
                entry.index
            ));
        }

        let track = track_map.get(entry.track_id.as_str());
        if let Some(t) = track {
            if t.duration_ms > 120 * 60 * 1000 {
                errors.push(format!(
                    "track_id {} exceeds 120min limit - split at render time",
                    entry.track_id
                ));
            }
            let filename = format!(
                "{:0width$}_{}.mp3",
                entry.index,
                safe_title,
                width = pad_width
            );
            let exists = std::path::Path::new(&t.audio_path).exists();

            plan_entries.push(PlanEntry {
                index: entry.index,
                title: entry.title.clone(),
                role: entry.role.clone(),
                filename,
                duration_ms: t.duration_ms,
                exists,
                track_id: entry.track_id.clone(),
                audio_path: t.audio_path.clone(),
            });
        } else {
            errors.push(format!("track_id {} not found in project", entry.track_id));
        }
    }

    if chapter_count == 0 {
        errors.push("at least one chapter is required".into());
    }

    if !errors.is_empty() {
        // deduplicate errors keeping order
        let mut unique_errors = Vec::new();
        let mut seen = HashSet::new();
        for e in errors {
            if seen.insert(e.clone()) {
                unique_errors.push(e);
            }
        }
        Err(unique_errors)
    } else {
        Ok(DeliveryPlan {
            book_title_sanitized: safe_book_title,
            entries: plan_entries,
        })
    }
}

pub async fn post_deliver_plan(
    State(app): State<AppState>,
    Path(project_id): Path<String>,
    Json(req): Json<DeliverRequest>,
) -> Json<DeliverResponse> {
    let mut response = match app
        .db
        .query("SELECT * FROM tracks WHERE project_id = $id")
        .bind(("id", project_id.clone()))
        .await
    {
        Ok(res) => res,
        Err(e) => {
            return Json(DeliverResponse {
                book_dir: None,
                files: vec![],
                manifest_path: None,
                warnings: vec![],
                errors: vec![e.to_string()],
                entries: None,
            })
        }
    };
    let raw_results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
    let db_tracks: Vec<Track> = raw_results
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    match validate_and_plan(&req, &db_tracks) {
        Ok(plan) => {
            let files = plan.entries.iter().map(|e| e.filename.clone()).collect();
            Json(DeliverResponse {
                book_dir: None,
                files,
                manifest_path: None,
                warnings: vec![],
                errors: vec![],
                entries: Some(plan.entries),
            })
        }
        Err(errors) => Json(DeliverResponse {
            book_dir: None,
            files: vec![],
            manifest_path: None,
            warnings: vec![],
            errors,
            entries: None,
        }),
    }
}

#[derive(Serialize)]
pub struct ManifestEntry {
    pub index: u32,
    pub title: String,
    pub role: String,
    pub filename: String,
    pub duration_ms: u64,
    pub sample_peak_db: f32,
    pub rms_db: f32,
    pub noise_floor_db: f32,
    pub passes_acx: bool,
}

#[derive(Serialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub book_title: String,
    pub generated_at: String,
    pub entries: Vec<ManifestEntry>,
    pub rms_spread_db: f32,
    pub warnings: Vec<String>,
}

fn build_minimal_blob(audio_path: &str) -> StoredBlob {
    StoredBlob {
        id: "delivery".into(),
        version: "1.0".into(),
        blob_type: "audio".into(),
        created_at: "".into(),
        input_hash: "".into(),
        seed: 1,
        pipeline_version: "".into(),
        preset_id: "acx".into(),
        loudness: StoredLoudness::default(),
        quality: StoredQuality::default(),
        provenance: StoredProvenance::default(),
        spatial: StoredSpatial::default(),
        schema_version: 1,
        aether_cert: None,
        aether_persona: None,
        aether_config: None,
        stem_fingerprints: None,
        qr_base64: None,
        pcm_blake3: None,
        cert_signature: None,
        processing_timeline: vec![],
        dead_air: DeadAirSummary::default(),
        audio_path: Arc::new(ManagedPcm::new(std::path::PathBuf::from(audio_path))),
        sample_rate: 48000,
        channels: 2,
        num_frames: 0,
    }
}

pub fn run_deliver_core(
    req: &DeliverRequest,
    plan: DeliveryPlan,
) -> Result<DeliverResponse, Vec<String>> {
    let out_dir_str = match &req.output_dir {
        Some(d) => d,
        None => return Err(vec!["output_dir is required for deliver".into()]),
    };

    let book_dir = std::path::PathBuf::from(out_dir_str).join(&plan.book_title_sanitized);
    if let Err(e) = std::fs::create_dir_all(&book_dir) {
        return Err(vec![format!("failed to create book dir: {}", e)]);
    }

    let mut manifest_entries = Vec::new();
    let mut warnings = Vec::new();
    let mut files = Vec::new();

    for plan_entry in &plan.entries {
        if !plan_entry.exists {
            return Err(vec![format!(
                "PCM file for track {} does not exist at {}",
                plan_entry.track_id, plan_entry.audio_path
            )]);
        }

        let blob = build_minimal_blob(&plan_entry.audio_path);
        let final_path = book_dir.join(&plan_entry.filename);

        let report = match export_mp3_acx(&blob, &final_path) {
            Ok(r) => r,
            Err(e) => {
                return Err(vec![format!(
                    "export_mp3_acx failed for track {}: {}",
                    plan_entry.track_id, e
                )])
            }
        };

        files.push(plan_entry.filename.clone());
        manifest_entries.push(ManifestEntry {
            index: plan_entry.index,
            title: plan_entry.title.clone(),
            role: plan_entry.role.clone(),
            filename: plan_entry.filename.clone(),
            duration_ms: plan_entry.duration_ms,
            sample_peak_db: report.sample_peak_db,
            rms_db: report.rms_db,
            noise_floor_db: report.noise_floor_db.unwrap_or_default(),
            passes_acx: report.passes_acx(),
        });
    }

    let mut min_rms = f32::MAX;
    let mut max_rms = f32::MIN;
    for e in &manifest_entries {
        if e.role == "chapter" {
            if e.rms_db < min_rms {
                min_rms = e.rms_db;
            }
            if e.rms_db > max_rms {
                max_rms = e.rms_db;
            }
        }
    }

    let rms_spread = if min_rms <= max_rms {
        max_rms - min_rms
    } else {
        0.0
    };
    if rms_spread > 4.0 {
        warnings.push(format!(
            "RMS spread across chapters is {:.1} dB (> 4.0 dB)",
            rms_spread
        ));
    }

    let roles: HashSet<&str> = manifest_entries.iter().map(|e| e.role.as_str()).collect();
    if !roles.contains("opening_credits") {
        warnings.push("missing opening_credits".into());
    }
    if !roles.contains("closing_credits") {
        warnings.push("missing closing_credits".into());
    }
    if !roles.contains("retail_sample") {
        warnings.push("missing retail_sample".into());
    }

    let manifest = Manifest {
        schema_version: 1,
        book_title: req.book_title.clone(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        entries: manifest_entries,
        rms_spread_db: rms_spread,
        warnings: warnings.clone(),
    };

    let manifest_path = book_dir.join("manifest.json");
    if let Ok(json) = serde_json::to_string_pretty(&manifest) {
        let _ = std::fs::write(&manifest_path, json);
    }

    Ok(DeliverResponse {
        book_dir: Some(book_dir.to_string_lossy().into_owned()),
        files,
        manifest_path: Some(manifest_path.to_string_lossy().into_owned()),
        warnings,
        errors: vec![],
        entries: None,
    })
}

pub async fn post_deliver(
    State(app): State<AppState>,
    Path(project_id): Path<String>,
    Json(req): Json<DeliverRequest>,
) -> Json<DeliverResponse> {
    let mut response = match app
        .db
        .query("SELECT * FROM tracks WHERE project_id = $id")
        .bind(("id", project_id.clone()))
        .await
    {
        Ok(res) => res,
        Err(e) => {
            return Json(DeliverResponse {
                book_dir: None,
                files: vec![],
                manifest_path: None,
                warnings: vec![],
                errors: vec![e.to_string()],
                entries: None,
            })
        }
    };
    let raw_results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
    let db_tracks: Vec<Track> = raw_results
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    match validate_and_plan(&req, &db_tracks) {
        Ok(plan) => {
            // blocking IO in spawn_blocking
            let result = tokio::task::spawn_blocking(move || run_deliver_core(&req, plan))
                .await
                .unwrap_or_else(|e| Err(vec![e.to_string()]));

            match result {
                Ok(resp) => Json(resp),
                Err(errors) => Json(DeliverResponse {
                    book_dir: None,
                    files: vec![],
                    manifest_path: None,
                    warnings: vec![],
                    errors,
                    entries: None,
                }),
            }
        }
        Err(errors) => Json(DeliverResponse {
            book_dir: None,
            files: vec![],
            manifest_path: None,
            warnings: vec![],
            errors,
            entries: None,
        }),
    }
}
