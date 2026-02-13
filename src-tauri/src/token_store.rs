use crate::error::{AppError, AppResult};

// Keep each chunk comfortably below the Windows CredentialBlob size limit (2560 bytes).
// keyring backends may store UTF-16 internally, so we use a conservative char count.
const TOKEN_CHUNK_SIZE: usize = 1000;
const TOKEN_PARTS_SUFFIX: &str = ":parts";
const TOKEN_PART_PREFIX: &str = ":part:";

pub fn ensure_service(service: &str) -> AppResult<()> {
    match service {
        "claude" | "codex" => Ok(()),
        _ => Err(AppError::UnsupportedService),
    }
}

fn token_key(service: &str, id: &str) -> String {
    format!("{service}:{id}")
}

fn token_parts_key(base_key: &str) -> String {
    format!("{base_key}{TOKEN_PARTS_SUFFIX}")
}

fn token_part_key(base_key: &str, index: usize) -> String {
    format!("{base_key}{TOKEN_PART_PREFIX}{index}")
}

fn open_entry(username: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(crate::APP_NAME, username)
        .map_err(|e| AppError::Keyring(format!("Failed to open keyring entry: {e}")))
}

fn delete_key(username: &str) -> AppResult<()> {
    let entry = open_entry(username)?;
    match entry.delete_password() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Keyring(format!(
            "Failed to delete token from keyring: {e}"
        ))),
    }
}

fn split_token_for_storage(token: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < token.len() {
        let end = (start + TOKEN_CHUNK_SIZE).min(token.len());
        out.push(&token[start..end]);
        start = end;
    }
    out
}

fn restore_chunked_token(base_key: &str) -> Option<String> {
    let parts_entry = keyring::Entry::new(crate::APP_NAME, &token_parts_key(base_key)).ok()?;
    let raw_count = parts_entry.get_password().ok()?;
    let part_count = raw_count.parse::<usize>().ok()?;
    if part_count == 0 {
        return None;
    }

    let mut token = String::new();
    for index in 0..part_count {
        let part_entry = keyring::Entry::new(crate::APP_NAME, &token_part_key(base_key, index)).ok()?;
        let part = part_entry.get_password().ok()?;
        token.push_str(&part);
    }
    Some(token)
}

pub fn get_token(service: &str, id: &str) -> Option<String> {
    let base_key = token_key(service, id);

    if let Some(token) = restore_chunked_token(&base_key) {
        return Some(token);
    }

    let entry = keyring::Entry::new(crate::APP_NAME, &base_key).ok()?;
    entry.get_password().ok()
}

pub fn set_token(service: &str, id: &str, token: &str) -> AppResult<()> {
    let base_key = token_key(service, id);
    delete_token(service, id)?;
    let token_len = token.len();

    if token.len() <= TOKEN_CHUNK_SIZE {
        let entry = open_entry(&base_key)?;
        return entry
            .set_password(token)
            .map_err(|e| AppError::Keyring(format!(
                "Failed to store token in keyring: {e} (len={token_len})"
            )));
    }

    let parts = split_token_for_storage(token);
    for (index, part) in parts.iter().enumerate() {
        let part_entry = open_entry(&token_part_key(&base_key, index))?;
        if let Err(e) = part_entry.set_password(part) {
            let _ = delete_token(service, id);
            return Err(AppError::Keyring(format!(
                "Failed to store token in keyring: {e} (len={token_len}, part={index}, part_len={})",
                part.len()
            )));
        }
    }

    let meta_entry = open_entry(&token_parts_key(&base_key))?;
    if let Err(e) = meta_entry.set_password(&parts.len().to_string()) {
        let _ = delete_token(service, id);
        return Err(AppError::Keyring(format!(
            "Failed to store token in keyring: {e} (len={token_len}, part_count={})",
            parts.len()
        )));
    }

    Ok(())
}

pub fn delete_token(service: &str, id: &str) -> AppResult<()> {
    let base_key = token_key(service, id);
    let mut first_error: Option<AppError> = None;

    if let Err(e) = delete_key(&base_key) {
        first_error.get_or_insert(e);
    }

    let parts_key = token_parts_key(&base_key);
    let part_count = match open_entry(&parts_key)?.get_password() {
        Ok(raw_count) => raw_count.parse::<usize>().unwrap_or(0),
        Err(keyring::Error::NoEntry) => 0,
        Err(e) => {
            first_error.get_or_insert(AppError::Keyring(format!(
                "Failed to delete token from keyring: {e}"
            )));
            0
        }
    };

    for index in 0..part_count {
        if let Err(e) = delete_key(&token_part_key(&base_key, index)) {
            first_error.get_or_insert(e);
        }
    }

    if let Err(e) = delete_key(&parts_key) {
        first_error.get_or_insert(e);
    }

    // Also clean up refresh token and expiry metadata
    let _ = delete_key(&refresh_token_key(service, id));
    let _ = delete_key(&expires_at_key(service, id));

    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

// ── Refresh token & expiry helpers ──

fn refresh_token_key(service: &str, id: &str) -> String {
    format!("{service}:{id}:refresh")
}

fn expires_at_key(service: &str, id: &str) -> String {
    format!("{service}:{id}:expires")
}

pub fn get_refresh_token(service: &str, id: &str) -> Option<String> {
    let key = refresh_token_key(service, id);
    if let Some(token) = restore_chunked_token(&key) {
        return Some(token);
    }
    let entry = keyring::Entry::new(crate::APP_NAME, &key).ok()?;
    entry.get_password().ok()
}

pub fn set_refresh_token(service: &str, id: &str, token: &str) -> AppResult<()> {
    let key = refresh_token_key(service, id);
    let _ = delete_key(&key);
    let entry = open_entry(&key)?;
    entry
        .set_password(token)
        .map_err(|e| AppError::Keyring(format!("Failed to store refresh token: {e}")))
}

pub fn get_expires_at(service: &str, id: &str) -> Option<i64> {
    let key = expires_at_key(service, id);
    let entry = keyring::Entry::new(crate::APP_NAME, &key).ok()?;
    let val = entry.get_password().ok()?;
    val.parse::<i64>().ok()
}

pub fn set_expires_at(service: &str, id: &str, epoch_ms: i64) -> AppResult<()> {
    let key = expires_at_key(service, id);
    let entry = open_entry(&key)?;
    entry
        .set_password(&epoch_ms.to_string())
        .map_err(|e| AppError::Keyring(format!("Failed to store token expiry: {e}")))
}

#[cfg(test)]
mod tests {
    use super::{split_token_for_storage, TOKEN_CHUNK_SIZE};

    #[test]
    fn split_token_for_storage_keeps_original_content() {
        let input = "a".repeat(TOKEN_CHUNK_SIZE * 2 + 13);
        let chunks = split_token_for_storage(&input);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), TOKEN_CHUNK_SIZE);
        assert_eq!(chunks[1].len(), TOKEN_CHUNK_SIZE);
        assert_eq!(chunks[2].len(), 13);
        assert_eq!(chunks.concat(), input);
    }
}
