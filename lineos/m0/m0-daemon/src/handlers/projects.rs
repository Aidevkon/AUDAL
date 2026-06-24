//! projects.rs — Project & Track CRUD handlers.
//! Persistent via SurrealDB (kv-surrealkv).
//! Bypasses v3 SDK Trait Bounds using Universal JSON Adapter.

use crate::app_state::AppState;
use crate::db::schema::{Project, Track};
use axum::extract::Path;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub flavour_id: Option<String>,
}

#[derive(Serialize)]
pub struct ProjectResponse {
    pub ok: bool,
    pub id: String,
    pub message: String,
}

/// POST /projects — create new project
pub async fn create_project(
    State(app): State<AppState>,
    Json(req): Json<CreateProjectRequest>,
) -> Json<ProjectResponse> {
    let aql = "CREATE projects SET name = $name, created_at = $created_at, flavour_id = $flavour_id, track_count = 0 RETURN *;";
    let created_at = chrono::Utc::now().to_rfc3339();
    let flavour_id = req
        .flavour_id
        .clone()
        .unwrap_or_else(|| "neutral".to_string());

    // Fix 1: Pass Owned Strings or &str, NOT &String
    let mut response = match app
        .db
        .query(aql)
        .bind(("name", req.name.clone()))
        .bind(("created_at", created_at))
        .bind(("flavour_id", flavour_id))
        .await
    {
        Ok(res) => res,
        Err(e) => {
            return Json(ProjectResponse {
                ok: false,
                id: String::new(),
                message: format!("DB Query Failed: {}", e),
            })
        }
    };

    // Fix 2: Fetch as generic serde_json::Value to bypass SurrealValue trait bounds
    let raw_result: Option<serde_json::Value> = response.take(0).unwrap_or(None);

    match raw_result {
        Some(val) => {
            // Fix 3: Deserialize safely on our side
            if let Ok(proj) = serde_json::from_value::<Project>(val) {
                let proj_id = proj.id.unwrap_or_default();
                Json(ProjectResponse {
                    ok: true,
                    id: proj_id,
                    message: format!("Project '{}' created", req.name),
                })
            } else {
                Json(ProjectResponse {
                    ok: false,
                    id: String::new(),
                    message: "Failed to parse Project JSON from DB".into(),
                })
            }
        }
        None => Json(ProjectResponse {
            ok: false,
            id: String::new(),
            message: "No record returned from DB".into(),
        }),
    }
}

/// GET /projects — list all projects
pub async fn list_projects(State(app): State<AppState>) -> Json<Vec<Project>> {
    if let Ok(mut response) = app.db.query("SELECT * FROM projects").await {
        let raw_results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let projects: Vec<Project> = raw_results
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        return Json(projects);
    }
    Json(vec![])
}

/// GET /projects/:id — get single project
pub async fn get_project(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Json<Option<Project>> {
    if let Ok(mut response) = app
        .db
        .query("SELECT * FROM type::thing('projects', $id)")
        .bind(("id", id.clone()))
        .await
    {
        let raw_result: Option<serde_json::Value> = response.take(0).unwrap_or(None);
        if let Some(val) = raw_result {
            return Json(serde_json::from_value(val).ok());
        }
    }
    Json(None)
}

/// GET /projects/:id/tracks — list tracks for project
pub async fn list_tracks(State(app): State<AppState>, Path(id): Path<String>) -> Json<Vec<Track>> {
    if let Ok(mut response) = app
        .db
        .query("SELECT * FROM tracks WHERE project_id = $id")
        .bind(("id", id.clone()))
        .await
    {
        let raw_results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let tracks: Vec<Track> = raw_results
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        return Json(tracks);
    }
    Json(vec![])
}
