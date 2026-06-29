use crate::db::schema::{Branch, DspStateJson, MixCommit};
use crate::db::DbConn;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn generate_hash(project_id: &str, timestamp: u64, dsp_state: &DspStateJson) -> String {
    let input = format!(
        "{}:{}:{}",
        project_id,
        timestamp,
        serde_json::to_string(dsp_state).unwrap_or_default()
    );
    blake3::hash(input.as_bytes()).to_hex().to_string()
}

pub async fn create_commit(
    _db: &DbConn, // we will use this later
    project_id: &str,
    branch: &str,
    dsp_state: &DspStateJson,
    message: &str,
) -> Result<MixCommit, String> {
    let ts = now_unix_ms();
    let hash = generate_hash(project_id, ts, dsp_state);

    Ok(MixCommit {
        id: None,
        project_id: project_id.to_string(),
        hash,
        parent_hash: None,
        branch_name: branch.to_string(),
        message: message.to_string(),
        dsp_state: dsp_state.clone(),
        blob_id: None,
        timestamp: ts,
    })
}

pub async fn checkout(
    _db: &DbConn,
    _project_id: &str,
    _commit_hash: &str,
) -> Result<DspStateJson, String> {
    // Fetch DspStateJson from DB only — no ArcSwap yet (Phase 4)
    Err("git_agent::checkout not yet implemented (dormant — wired in Git Phase)".into())
}

pub async fn create_branch(_db: &DbConn, project_id: &str, name: &str) -> Result<Branch, String> {
    Ok(Branch {
        id: None,
        project_id: project_id.to_string(),
        name: name.to_string(),
        head_hash: "mock_hash".to_string(),
        created_at: now_unix_ms(),
        is_default: false,
    })
}

pub async fn get_history(_db: &DbConn, _project_id: &str) -> Result<Vec<MixCommit>, String> {
    Err("git_agent::get_history not yet implemented (dormant — wired in Git Phase)".into())
}
