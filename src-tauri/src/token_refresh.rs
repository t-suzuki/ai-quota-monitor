use crate::oauth;
use crate::token_store;
use zeroize::Zeroize;

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
