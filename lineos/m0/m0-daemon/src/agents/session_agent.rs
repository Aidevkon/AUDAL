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

pub async fn increment_session_counter(
    _db: &DbConn,
    _user_id: &str,
) -> Result<u32, String> {
    todo!("increment_session_counter")
}

pub async fn get_last_flavour(
    _db: &DbConn,
    _user_id: &str,
) -> Result<Option<String>, String> {
    todo!("get_last_flavour")
}

pub async fn get_progressive_gate(
    _db: &DbConn,
    _user_id: &str,
) -> Result<ActiveGates, String> {
    todo!("get_progressive_gate")
}
