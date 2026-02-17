use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use crate::error::{AppError, AppResult};
use sha2::{Digest, Sha256};

const FETCH_USAGE_MIN_INTERVAL_MS: u64 = 1000;
const MAX_ACCOUNT_ID_LEN: usize = 128;
const MAX_ACCOUNT_NAME_LEN: usize = 256;
const MAX_TOKEN_LEN: usize = 16384;
const MAX_EXPORT_PATH_LEN: usize = 4096;

static FETCH_USAGE_RATE_LIMITER: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

fn has_control_chars(input: &str) -> bool {
    input.chars().any(char::is_control)
}

pub fn validate_account_id(id: &str) -> AppResult<()> {
    if id.is_empty() {
        return Err(AppError::InvalidInput("Account id is required".to_string()));
    }
    if id.len() > MAX_ACCOUNT_ID_LEN {
        return Err(AppError::InvalidInput(format!(
            "Account id is too long (max {MAX_ACCOUNT_ID_LEN} chars)"
        )));
    }
    if has_control_chars(id) {
        return Err(AppError::InvalidInput(
            "Account id contains control characters".to_string(),
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(AppError::InvalidInput(
            "Account id contains unsupported characters".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_account_name(name: &str) -> AppResult<()> {
    if name.is_empty() {
        return Err(AppError::InvalidInput("Account name is required".to_string()));
    }
    if name.len() > MAX_ACCOUNT_NAME_LEN {
        return Err(AppError::InvalidInput(format!(
            "Account name is too long (max {MAX_ACCOUNT_NAME_LEN} chars)"
        )));
    }
    if has_control_chars(name) {
        return Err(AppError::InvalidInput(
            "Account name contains control characters".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_token(token: &str) -> AppResult<()> {
    if token.is_empty() {
        return Err(AppError::InvalidInput("Token is required".to_string()));
    }
    if token.len() > MAX_TOKEN_LEN {
        return Err(AppError::InvalidInput(format!(
            "Token is too long (max {MAX_TOKEN_LEN} chars)"
        )));
    }
    if has_control_chars(token) {
        return Err(AppError::InvalidInput(
            "Token contains control characters".to_string(),
        ));
    }
    if !token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
    {
        return Err(AppError::InvalidInput(
            "Token contains unsupported characters".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_upstream_url(url: &str) -> AppResult<()> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| AppError::InvalidInput(format!("Invalid upstream URL: {e}")))?;
    if parsed.scheme() != "https" {
        return Err(AppError::InvalidInput(
            "Upstream URL must use https".to_string(),
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::InvalidInput("Upstream URL must include host".to_string()))?;
    match host {
        "api.anthropic.com" | "chatgpt.com" | "console.anthropic.com" | "auth.openai.com" | "claude.ai" => Ok(()),
        _ => Err(AppError::InvalidInput(
            "Upstream host is not allowlisted".to_string(),
        )),
    }
}

const MAX_WEBHOOK_URL_LEN: usize = 2048;
const MAX_PUSHOVER_KEY_LEN: usize = 64;

pub fn validate_discord_webhook_url(url: &str) -> AppResult<()> {
    if url.is_empty() {
        return Err(AppError::InvalidInput("Discord webhook URL is required".to_string()));
    }
    if url.len() > MAX_WEBHOOK_URL_LEN {
        return Err(AppError::InvalidInput(format!(
            "Discord webhook URL is too long (max {MAX_WEBHOOK_URL_LEN} chars)"
        )));
    }
    if has_control_chars(url) {
        return Err(AppError::InvalidInput(
            "Discord webhook URL contains control characters".to_string(),
        ));
    }
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| AppError::InvalidInput(format!("Invalid Discord webhook URL: {e}")))?;
    if parsed.scheme() != "https" {
        return Err(AppError::InvalidInput(
            "Discord webhook URL must use https".to_string(),
        ));
    }
    let host = parsed.host_str().unwrap_or("");
    if host != "discord.com" && host != "discordapp.com" {
        return Err(AppError::InvalidInput(
            "Discord webhook URL must be from discord.com".to_string(),
        ));
    }
    if !parsed.path().starts_with("/api/webhooks/") {
        return Err(AppError::InvalidInput(
            "Invalid Discord webhook URL path".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_pushover_key(key: &str, label: &str) -> AppResult<()> {
    if key.is_empty() {
        return Err(AppError::InvalidInput(format!("{label} is required")));
    }
    if key.len() > MAX_PUSHOVER_KEY_LEN {
        return Err(AppError::InvalidInput(format!(
            "{label} is too long (max {MAX_PUSHOVER_KEY_LEN} chars)"
        )));
    }
    if has_control_chars(key) {
        return Err(AppError::InvalidInput(format!(
            "{label} contains control characters"
        )));
    }
    if !key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::InvalidInput(format!(
            "{label} contains invalid characters"
        )));
    }
    Ok(())
}

pub fn validate_export_path(path: &str) -> AppResult<()> {
    if path.is_empty() {
        return Err(AppError::InvalidInput("Export path is required".to_string()));
    }
    if path.len() > MAX_EXPORT_PATH_LEN {
        return Err(AppError::InvalidInput(format!(
            "Export path is too long (max {MAX_EXPORT_PATH_LEN} chars)"
        )));
    }
    if has_control_chars(path) {
        return Err(AppError::InvalidInput(
            "Export path contains control characters".to_string(),
        ));
    }
    Ok(())
}

pub fn enforce_fetch_usage_rate_limit(service: &str, token: &str) -> AppResult<()> {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());
    let key = format!("{service}:{token_hash}");
    let limiter = FETCH_USAGE_RATE_LIMITER.get_or_init(|| Mutex::new(HashMap::new()));
    let mut lock = limiter
        .lock()
        .map_err(|_| AppError::Message("Rate limiter lock is poisoned".to_string()))?;

    if lock.len() > 4096 {
        let ttl = Duration::from_secs(600);
        let now = Instant::now();
        lock.retain(|_, seen_at| now.duration_since(*seen_at) <= ttl);
    }

    let now = Instant::now();
    let min_interval = Duration::from_millis(FETCH_USAGE_MIN_INTERVAL_MS);
    if let Some(previous) = lock.get(&key) {
        let elapsed = now.duration_since(*previous);
        if elapsed < min_interval {
            let wait_ms = min_interval.saturating_sub(elapsed).as_millis();
            return Err(AppError::Message(format!("Rate limited. Retry in {wait_ms}ms")));
        }
    }

    lock.insert(key, now);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_export_path_rejects_empty() {
        assert!(validate_export_path("").is_err());
    }

    #[test]
    fn validate_export_path_rejects_control_chars() {
        assert!(validate_export_path("a\nb").is_err());
    }

    #[test]
    fn validate_export_path_accepts_reasonable() {
        assert!(validate_export_path("quota.json").is_ok());
    }

    #[test]
    fn validate_discord_webhook_url_accepts_valid() {
        assert!(validate_discord_webhook_url(
            "https://discord.com/api/webhooks/123456/abcdef"
        ).is_ok());
        assert!(validate_discord_webhook_url(
            "https://discordapp.com/api/webhooks/123/tok"
        ).is_ok());
    }

    #[test]
    fn validate_discord_webhook_url_rejects_invalid() {
        assert!(validate_discord_webhook_url("").is_err());
        assert!(validate_discord_webhook_url("http://discord.com/api/webhooks/1/t").is_err());
        assert!(validate_discord_webhook_url("https://example.com/hook").is_err());
        assert!(validate_discord_webhook_url("https://discord.com/other/path").is_err());
    }

    #[test]
    fn validate_pushover_key_accepts_alphanumeric() {
        assert!(validate_pushover_key("abc123DEF", "API Token").is_ok());
    }

    #[test]
    fn validate_pushover_key_rejects_invalid() {
        assert!(validate_pushover_key("", "API Token").is_err());
        assert!(validate_pushover_key("abc-123", "API Token").is_err());
        assert!(validate_pushover_key("abc 123", "User Key").is_err());
    }
}
