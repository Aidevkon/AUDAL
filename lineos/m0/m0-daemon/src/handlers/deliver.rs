//! `run_deliver_core` and its own value types moved to conformance
//! 21/09 (F-137/PRD §6.1). `sanitize_title`/`validate_and_plan` stay
//! here — they take `crate::db::schema::Track`, and run_deliver_core
//! itself never calls either. `apply_sidecar_updates`/
//! `write_manifest_file`/the axum handlers stay too (blob_store's
//! find_sidecar/write_sidecar, F-129, or axum itself).

use crate::app_state::AppState;
use crate::blob_store::BlobVariant;
use crate::db::schema::Track;
pub use conformance::deliver::{
    DeliverEntry, DeliverRequest, DeliverResponse, DeliveryPlan, Manifest, PlanEntry,
    SidecarUpdate,
};
pub use conformance::deliver::run_deliver_core;
use axum::{
    extract::{Path, State},
    Json,
};
use std::collections::{HashMap, HashSet};

/// Looks up and rewrites each pending sidecar from `resp.sidecar_updates`,
/// appending the same warning text `run_deliver_core` used to produce
/// inline. Storage-layer read (`find_sidecar`) and write (`write_sidecar`)
/// both live here now, outside the core.
pub fn apply_sidecar_updates(resp: &mut DeliverResponse, masters_dir: &str, project_id: &str) {
    for update in &resp.sidecar_updates {
        match crate::blob_store::find_sidecar(masters_dir, &update.blob_id) {
            Ok(Some(sidecar_path)) => {
                if let BlobVariant::Certified { .. } = &update.updated_blob.variant {
                    let master_flac = sidecar_path.with_extension("flac");
                    if let Err(e) = crate::blob_store::write_sidecar(
                        masters_dir,
                        project_id,
                        &update.updated_blob,
                        &master_flac,
                    ) {
                        resp.warnings.push(format!(
                            "output-acx: sidecar rewrite failed for track {}: {e}",
                            update.track_id
                        ));
                    }
                }
            }
            Ok(None) => resp.warnings.push(format!(
                "output-acx: no existing sidecar found for track {} — certificate rewrite skipped",
                update.track_id
            )),
            Err(e) => resp.warnings.push(format!(
                "output-acx: sidecar lookup failed for track {}: {e}",
                update.track_id
            )),
        }
    }
}

/// Writes `manifest.json` from the value `run_deliver_core` returned,
/// folding in whatever warnings the caller (e.g. `apply_sidecar_updates`)
/// added since. Call after any warning-producing step, not before.
pub fn write_manifest_file(resp: &mut DeliverResponse) {
    let warnings = resp.warnings.clone();
    if let (Some(manifest), Some(path)) = (&mut resp.manifest, &resp.manifest_path) {
        manifest.warnings = warnings;
        if let Ok(json) = serde_json::to_string_pretty(manifest) {
            let _ = std::fs::write(path, json);
        }
    }
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
                resolved_blob: None,
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
                manifest: None,
                sidecar_updates: vec![],
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
                manifest: None,
                sidecar_updates: vec![],
            })
        }
        Err(errors) => Json(DeliverResponse {
            book_dir: None,
            files: vec![],
            manifest_path: None,
            warnings: vec![],
            errors,
            entries: None,
            manifest: None,
            sidecar_updates: vec![],
        }),
    }
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
                manifest: None,
                sidecar_updates: vec![],
            })
        }
    };
    let raw_results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
    let db_tracks: Vec<Track> = raw_results
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    match validate_and_plan(&req, &db_tracks) {
        Ok(mut plan) => {
            // Rehydrate real blobs before spawn_blocking
            for entry in &mut plan.entries {
                match crate::handlers::blob::get_or_rehydrate(&app, &entry.track_id).await {
                    Ok(blob) => {
                        if blob.is_certified() {
                            entry.resolved_blob = Some(blob);
                        } else {
                            tracing::warn!(
                                track_id = %entry.track_id,
                                reason = ?blob.variant,
                                "Deliver rehydration fallback: blob is uncertified, using minimal blob fallback"
                            );
                            entry.resolved_blob = None;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            track_id = %entry.track_id,
                            error = ?e,
                            "Deliver rehydration fallback: blob rehydrate failed, using minimal blob fallback"
                        );
                        entry.resolved_blob = None;
                    }
                }
            }

            // blocking IO in spawn_blocking
            let masters_dir = app.config.masters_path.clone();
            let project_id_for_core = project_id.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut resp = run_deliver_core(&req, plan)?;
                apply_sidecar_updates(&mut resp, &masters_dir, &project_id_for_core);
                write_manifest_file(&mut resp);
                Ok(resp)
            })
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
                    manifest: None,
                    sidecar_updates: vec![],
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
            manifest: None,
            sidecar_updates: vec![],
        }),
    }
}
