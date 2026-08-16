//! schema.rs — SurrealDB table definitions.
//! Projects, Tracks, Sessions for Creator OS.

use serde::{Deserialize, Serialize};

/// A mastering project (album or single).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Option<String>,
    pub name: String,
    pub created_at: String,
    pub flavour_id: String,
    pub track_count: usize,
}

/// ⚠ ΤΡΙΑ ΠΡΑΓΜΑΤΑ ΠΡΕΠΕΙ ΝΑ ΣΥΜΦΩΝΟΥΝ: αυτό το
/// struct, το DEFINE TABLE παρακάτω, και κάθε
/// CREATE query. Ο πίνακας είναι SCHEMAFULL —
/// άγνωστο πεδίο ΑΠΟΡΡΙΠΤΕΙ ολόκληρη την εγγραφή.
///
/// ΜΕΤΡΗΜΕΝΟ (db_track_write.rs): το track_id
/// έλειπε από τα δύο πρώτα και υπήρχε στο CREATE
/// του master.rs. Κάθε εγγραφή απέτυχε, ο πίνακας
/// έμεινε άδειος, και κανείς δεν το είδε επειδή
/// το σφάλμα καταπινόταν σε tokio::spawn.
///
/// A single mastered track within a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: Option<String>,
    pub project_id: String,
    pub track_id: String,
    pub audio_path: String,
    pub blob_id: String,
    /// §Π AUTH-READY HOOK #2 — ο ΔΕΙΚΤΗΣ καταλόγου→δίσκου.
    /// Η DB ξέρει ΓΙΑ τα πράγματα· ο δίσκος ΕΧΕΙ τα πράγματα.
    /// ΤΕΛΕΥΤΑΙΟ καταφύγιο του get_blob, ΟΧΙ πρώτο: το write εδώ
    /// ζει σε fire-and-forget spawn, και ένα certificate που
    /// υπάρχει στον δίσκο δεν επιτρέπεται να γίνει απρόσιτο
    /// επειδή απέτυχε μια εγγραφή που κανείς δεν περίμενε.
    pub blob_path: String,
    pub lufs: f32,
    pub true_peak: f32,
    pub flavour_id: String,
    pub created_at: String,
    pub duration_ms: u64,
}

/// A mastering session (single run of the pipeline).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Option<String>,
    pub project_id: String,
    pub track_id: String,
    pub blob_id: String,
    pub preset_id: String,
    pub lufs: f32,
    pub created_at: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingPatchJson {
    pub finding_type: String,
    pub delta: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteringParamsJson {
    pub ceiling_dbtp: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DspStateJson {
    pub flavour: String,
    pub intents: std::collections::HashMap<String, f32>,
    pub surgical_fixes: Vec<FindingPatchJson>,
    pub mastering: MasteringParamsJson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixCommit {
    pub id: Option<String>,
    pub project_id: String,
    pub hash: String,
    pub parent_hash: Option<String>,
    pub branch_name: String,
    pub message: String,
    pub dsp_state: DspStateJson,
    pub blob_id: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub id: Option<String>,
    pub project_id: String,
    pub name: String,
    pub head_hash: String,
    pub created_at: u64,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: Option<String>,
    pub user_id: String,
    pub sessions_completed: u32,
    pub current_persona: String,
    pub last_flavour: Option<String>,
    pub last_project_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFindingFeedback {
    pub id: Option<String>,
    pub user_id: String,
    pub commit_id: String,
    pub finding_type: String,
    pub action: String,
    pub delta: Option<f32>,
    pub knob_held_ms: Option<u64>,
    pub session_id: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFlavourPreference {
    pub id: Option<String>,
    pub user_id: String,
    pub flavour_id: String,
    pub apply_count: u32,
    pub last_used: u64,
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
        DEFINE FIELD track_id    ON tracks TYPE string;
        DEFINE FIELD audio_path  ON tracks TYPE string;
        DEFINE FIELD blob_id     ON tracks TYPE string;
        DEFINE FIELD blob_path   ON tracks TYPE string;
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

        DEFINE TABLE mix_commits SCHEMAFULL;
        DEFINE FIELD project_id ON mix_commits TYPE string;
        DEFINE FIELD hash ON mix_commits TYPE string;
        DEFINE FIELD parent_hash ON mix_commits TYPE option<string>;
        DEFINE FIELD branch_name ON mix_commits TYPE string;
        DEFINE FIELD message ON mix_commits TYPE string;
        DEFINE FIELD dsp_state ON mix_commits TYPE object;
        DEFINE FIELD blob_id ON mix_commits TYPE option<string>;
        DEFINE FIELD timestamp ON mix_commits TYPE int;
        DEFINE INDEX idx_mix_commits_project_hash ON mix_commits COLUMNS project_id, hash UNIQUE;

        DEFINE TABLE branches SCHEMAFULL;
        DEFINE FIELD project_id ON branches TYPE string;
        DEFINE FIELD name ON branches TYPE string;
        DEFINE FIELD head_hash ON branches TYPE string;
        DEFINE FIELD created_at ON branches TYPE int;
        DEFINE FIELD is_default ON branches TYPE bool;
        DEFINE INDEX idx_branches_name_project ON branches COLUMNS name, project_id UNIQUE;

        DEFINE TABLE user_profiles SCHEMAFULL;
        DEFINE FIELD user_id ON user_profiles TYPE string;
        DEFINE FIELD sessions_completed ON user_profiles TYPE int;
        DEFINE FIELD current_persona ON user_profiles TYPE string;
        DEFINE FIELD last_flavour ON user_profiles TYPE option<string>;
        DEFINE FIELD last_project_id ON user_profiles TYPE option<string>;
        DEFINE FIELD created_at ON user_profiles TYPE int;
        DEFINE FIELD updated_at ON user_profiles TYPE int;
        DEFINE INDEX idx_user_profiles_user_id ON user_profiles COLUMNS user_id UNIQUE;

        DEFINE TABLE user_finding_feedback TYPE RELATION FROM user_profiles TO mix_commits SCHEMAFULL;
        DEFINE FIELD in ON user_finding_feedback TYPE record<user_profiles>;
        DEFINE FIELD out ON user_finding_feedback TYPE record<mix_commits>;
        DEFINE FIELD user_id ON user_finding_feedback TYPE string;
        DEFINE FIELD commit_id ON user_finding_feedback TYPE string;
        DEFINE FIELD finding_type ON user_finding_feedback TYPE string;
        DEFINE FIELD action ON user_finding_feedback TYPE string;
        DEFINE FIELD delta ON user_finding_feedback TYPE option<float>;
        DEFINE FIELD knob_held_ms ON user_finding_feedback TYPE option<int>;
        DEFINE FIELD session_id ON user_finding_feedback TYPE string;
        DEFINE FIELD timestamp ON user_finding_feedback TYPE int;

        DEFINE TABLE user_flavour_preference TYPE RELATION FROM user_profiles TO projects SCHEMAFULL;
        DEFINE FIELD in ON user_flavour_preference TYPE record<user_profiles>;
        DEFINE FIELD out ON user_flavour_preference TYPE record<projects>;
        DEFINE FIELD user_id ON user_flavour_preference TYPE string;
        DEFINE FIELD flavour_id ON user_flavour_preference TYPE string;
        DEFINE FIELD apply_count ON user_flavour_preference TYPE int;
        DEFINE FIELD last_used ON user_flavour_preference TYPE int;
    ").await?;
    Ok(())
}
