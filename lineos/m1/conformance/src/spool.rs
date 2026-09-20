use std::path::PathBuf;
use std::sync::OnceLock;

static SPOOL_DIR: OnceLock<PathBuf> = OnceLock::new();

/// transient files ONLY — everything here may be deleted
/// at any daemon restart; nothing here survives on purpose (F-050).
pub fn init_spool_dir(p: PathBuf) {
    let _ = std::fs::create_dir_all(&p);
    let _ = SPOOL_DIR.set(p);
}

pub fn spool_dir() -> PathBuf {
    SPOOL_DIR.get().cloned().unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spool_dir_fallback() {
        let sd = spool_dir();
        assert_eq!(sd, std::env::temp_dir());
    }
}
