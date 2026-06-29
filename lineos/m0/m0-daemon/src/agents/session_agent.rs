use crate::db::DbConn;

pub enum SessionState {
    Idle,
    Analyzing,
    Ready,
    Rendering,
    Completed,
}

pub struct ActiveGates {
    pub gate1: bool,
    pub gate2: bool,
    pub gate3: bool,
    pub gate4: bool,
}

pub async fn increment_session_counter(_db: &DbConn, _user_id: &str) -> Result<u32, String> {
    Err("session_agent::increment_session_counter not yet implemented (dormant — wired in Session Phase)".into())
}

pub async fn get_last_flavour(_db: &DbConn, _user_id: &str) -> Result<Option<String>, String> {
    Err(
        "session_agent::get_last_flavour not yet implemented (dormant — wired in Session Phase)"
            .into(),
    )
}

pub async fn get_progressive_gate(_db: &DbConn, _user_id: &str) -> Result<ActiveGates, String> {
    Err("session_agent::get_progressive_gate not yet implemented (dormant — wired in Session Phase)".into())
}
