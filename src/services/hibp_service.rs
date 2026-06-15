use crate::errors::{KeeStoreError, Result};

/// Valide qu'un préfixe est exactement 5 caractères hexadécimaux.
pub fn validate_prefix(prefix: &str) -> Result<String> {
    if prefix.len() != 5 || !prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(KeeStoreError::BadRequest(
            "Le préfixe doit être exactement 5 caractères hexadécimaux".to_string(),
        ));
    }
    Ok(prefix.to_uppercase())
}
