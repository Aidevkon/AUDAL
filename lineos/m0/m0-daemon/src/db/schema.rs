//! schema.rs — SurrealDB table definitions.
//! Projects, Tracks, Sessions for Creator OS.

use serde::{Deserialize, Serialize};

/// A mastering project (album or single).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id:          Option<String>,
    pub name:        String,
    pub created_at:  String,
    pub flavour_id:  String,
    pub track_count: usize,
}

/// A single mastered track within a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id:          Option<String>,
    pub project_id:  String,
    pub audio_path:  String,
    pub blob_id:     String,
    pub lufs:        f32,
    pub true_peak:   f32,
    pub flavour_id:  String,
    pub created_at:  String,
    pub duration_ms: u64,
}

/// A mastering session (single run of the pipeline).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id:          Option<String>,
    pub project_id:  String,
    pub track_id:    String,
    pub blob_id:     String,
    pub preset_id:   String,
    pub lufs:        f32,
    pub created_at:  String,
    pub duration_ms: u64,
}

/// Initialize all tables in SurrealDB.
pub async fn migrate(db: &super::DbConn) -> Result<(), surrealdb::Error> {
    // Define tables with schema
    db.query("
        DEFINE TABLE projects SCHEMAFULL;
        DEFINE FIELD name        ON projects TYPE string;
        DEFINE FIELD created_at  ON projects TYPE string;
        DEFINE FIELD flavour_id  ON projects TYPE string;
        DEFINE FIELD track_count ON projects TYPE int;

        DEFINE TABLE tracks SCHEMAFULL;
        DEFINE FIELD project_id  ON tracks TYPE string;
        DEFINE FIELD audio_path  ON tracks TYPE string;
        DEFINE FIELD blob_id     ON tracks TYPE string;
        DEFINE FIELD lufs        ON tracks TYPE float;
        DEFINE FIELD true_peak   ON tracks TYPE float;
        DEFINE FIELD flavour_id  ON tracks TYPE string;
        DEFINE FIELD created_at  ON tracks TYPE string;
        DEFINE FIELD duration_ms ON tracks TYPE int;

        DEFINE TABLE sessions SCHEMAFULL;
        DEFINE FIELD project_id  ON sessions TYPE string;
        DEFINE FIELD track_id    ON sessions TYPE string;
        DEFINE FIELD blob_id     ON sessions TYPE string;
        DEFINE FIELD preset_id   ON sessions TYPE string;
        DEFINE FIELD lufs        ON sessions TYPE float;
        DEFINE FIELD created_at  ON sessions TYPE string;
        DEFINE FIELD duration_ms ON sessions TYPE int;
    ").await?;
    Ok(())
}
