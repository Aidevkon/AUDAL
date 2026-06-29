use crate::db::DbConn;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CorpusAction {
    Apply,
    Ignore,
    Cancelled,
    Reverted,
}

pub async fn record_feedback(
    _db: &DbConn,
    _user_id: &str,
    _commit_id: &str,
    _finding_type: &str,
    _action: CorpusAction,
    _delta: Option<f32>,
) -> Result<(), String> {
    Ok(())
}

pub async fn get_finding_score(
    _db: &DbConn,
    _user_id: &str,
    _finding_type: &str,
) -> Result<f32, String> {
    // apply=+2.0, ignore=-1.0, cancelled=0.0, reverted=-1.0
    Err(
        "corpus_agent::get_finding_score not yet implemented (dormant — wired in Corpus Phase)"
            .into(),
    )
}
