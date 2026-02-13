pub fn ensure_service(service: &str) -> Result<(), String> {
    match service {
        "claude" | "codex" => Ok(()),
        _ => Err("Unsupported service".to_string()),
    }
}

fn token_key(service: &str, id: &str) -> String {
    format!("{service}:{id}")
}

pub fn get_token(service: &str, id: &str) -> Option<String> {
    let entry = keyring::Entry::new(crate::APP_NAME, &token_key(service, id)).ok()?;
    entry.get_password().ok()
}

pub fn set_token(service: &str, id: &str, token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(crate::APP_NAME, &token_key(service, id))
        .map_err(|e| format!("Failed to open keyring entry: {e}"))?;
    entry
        .set_password(token)
        .map_err(|e| format!("Failed to store token in keyring: {e}"))
}

pub fn delete_token(service: &str, id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(crate::APP_NAME, &token_key(service, id))
        .map_err(|e| format!("Failed to open keyring entry: {e}"))?;
    match entry.delete_password() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Failed to delete token from keyring: {e}")),
    }
}
