use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const FETCH_USAGE_MIN_INTERVAL_MS: u64 = 500;
const MAX_ACCOUNT_ID_LEN: usize = 128;
const MAX_ACCOUNT_NAME_LEN: usize = 256;
const MAX_TOKEN_LEN: usize = 4096;

static FETCH_USAGE_RATE_LIMITER: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

fn has_control_chars(input: &str) -> bool {
    input.chars().any(char::is_control)
}

pub fn validate_account_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Account id is required".to_string());
    }
    if id.len() > MAX_ACCOUNT_ID_LEN {
        return Err(format!(
            "Account id is too long (max {MAX_ACCOUNT_ID_LEN} chars)"
        ));
    }
    if has_control_chars(id) {
        return Err("Account id contains control characters".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err("Account id contains unsupported characters".to_string());
    }
    Ok(())
}

pub fn validate_account_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Account name is required".to_string());
    }
    if name.len() > MAX_ACCOUNT_NAME_LEN {
        return Err(format!(
            "Account name is too long (max {MAX_ACCOUNT_NAME_LEN} chars)"
        ));
    }
    if has_control_chars(name) {
        return Err("Account name contains control characters".to_string());
    }
    Ok(())
}

pub fn validate_token(token: &str) -> Result<(), String> {
    if token.is_empty() {
        return Err("Token is required".to_string());
    }
    if token.len() > MAX_TOKEN_LEN {
        return Err(format!("Token is too long (max {MAX_TOKEN_LEN} chars)"));
    }
    if has_control_chars(token) {
        return Err("Token contains control characters".to_string());
    }
    if !token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
    {
        return Err("Token contains unsupported characters".to_string());
    }
    Ok(())
}

pub fn validate_upstream_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid upstream URL: {e}"))?;
    if parsed.scheme() != "https" {
        return Err("Upstream URL must use https".to_string());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "Upstream URL must include host".to_string())?;
    match host {
        "api.anthropic.com" | "chatgpt.com" => Ok(()),
        _ => Err("Upstream host is not allowlisted".to_string()),
    }
}

pub fn enforce_fetch_usage_rate_limit(service: &str, id: &str) -> Result<(), String> {
    let key = format!("{service}:{id}");
    let limiter = FETCH_USAGE_RATE_LIMITER.get_or_init(|| Mutex::new(HashMap::new()));
    let mut lock = limiter
        .lock()
        .map_err(|_| "Rate limiter lock is poisoned".to_string())?;

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
            return Err(format!("Rate limited. Retry in {wait_ms}ms"));
        }
    }

    lock.insert(key, now);
    Ok(())
}
