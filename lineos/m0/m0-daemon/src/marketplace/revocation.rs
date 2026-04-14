//! Key revocation — M0 Constitution v2.0 §03.7
//! Any engine signed with a revoked key is immediately unloadable.

/// Returns true if the given public key hex is in the revoked_keys list.
/// Used by the Gatekeeper before signature verification.
pub fn is_revoked(key: &str, revoked_keys: &[String]) -> bool {
    revoked_keys.iter().any(|k| k == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_detects_revoked_key() {
        let revoked = vec!["bad_key_001".to_string(), "bad_key_002".to_string()];
        assert!(is_revoked("bad_key_001", &revoked));
        assert!(is_revoked("bad_key_002", &revoked));
    }

    #[test]
    fn revocation_passes_valid_key() {
        let revoked = vec!["bad_key_001".to_string()];
        assert!(!is_revoked("good_key_abc", &revoked));
    }

    #[test]
    fn revocation_empty_list_passes_all() {
        assert!(!is_revoked("any_key", &[]));
    }
}
