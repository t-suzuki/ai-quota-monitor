use crate::oauth;
use crate::token_store;
use zeroize::Zeroize;

/// Minimum remaining time (ms) before we trigger a refresh.
const REFRESH_MARGIN_MS: i64 = 5 * 60 * 1000; // 5 minutes

/// Check if the token for the given account needs refreshing,
/// and refresh it if a refresh_token is available.
/// Returns Ok(true) if refreshed, Ok(false) if not needed / not possible.
pub async fn try_refresh_if_needed(service: &str, id: &str) -> Result<bool, String> {
    let expires_at = match token_store::get_expires_at(service, id) {
        Some(ts) => ts,
        None => return Ok(false), // No expiry info — can't determine if refresh needed
    };

    let now = now_millis();
    if now < expires_at - REFRESH_MARGIN_MS {
        return Ok(false); // Still fresh
    }

    do_refresh(service, id).await
}

/// Force a token refresh using the stored refresh_token.
/// Returns Ok(true) on success.
pub async fn do_refresh(service: &str, id: &str) -> Result<bool, String> {
    let mut refresh_tok = match token_store::get_refresh_token(service, id) {
        Some(t) => t,
        None => return Err("No refresh token available. Please log in again.".into()),
    };

    let result = match service {
        "claude" => oauth::claude::refresh_token(&refresh_tok).await,
        "codex" => oauth::codex::refresh_token(&refresh_tok).await,
        _ => return Err(format!("Unsupported service: {service}")),
    };

    refresh_tok.zeroize();

    let mut tokens = result?;

    // Store new access token
    token_store::set_token(service, id, &tokens.access_token)
        .map_err(|e| format!("Failed to store refreshed token: {e}"))?;

    // Store new refresh token (rotation: new one replaces old)
    if let Some(ref new_refresh) = tokens.refresh_token {
        token_store::set_refresh_token(service, id, new_refresh)
            .map_err(|e| format!("Failed to store new refresh token: {e}"))?;
    }

    // Store new expiry
    if let Some(exp) = tokens.expires_at {
        token_store::set_expires_at(service, id, exp)
            .map_err(|e| format!("Failed to store new expiry: {e}"))?;
    }

    tokens.access_token.zeroize();

    Ok(true)
}

/// Attempt to refresh a token after an HTTP 401 error.
/// Returns the new access token if successful.
pub async fn refresh_on_401(service: &str, id: &str) -> Result<String, String> {
    do_refresh(service, id).await?;
    token_store::get_token(service, id)
        .ok_or_else(|| "Token not found after refresh".to_string())
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
