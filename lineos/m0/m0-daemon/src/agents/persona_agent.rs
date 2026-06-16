use crate::db::DbConn;

pub async fn update_persona(
    _db: &DbConn,
    _user_id: &str,
    sessions_completed: u32,
) -> Result<String, String> {
    let persona = match sessions_completed {
        0..=4 => "beginner",
        5..=19 => "intermediate",
        _ => "pro",
    };
    // Update current_persona in user_profiles table.
    Ok(persona.to_string())
}
