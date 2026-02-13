use crate::error::{AppError, AppResult};
use crate::oauth;
use crate::token_refresh;
use crate::token_store;
use serde::Serialize;
use std::sync::Mutex;
use tokio::sync::oneshot;
use zeroize::Zeroize;

/// Global cancel senders for in-progress logins (one per service).
static LOGIN_CANCEL: std::sync::OnceLock<Mutex<Option<oneshot::Sender<()>>>> =
    std::sync::OnceLock::new();

fn cancel_store() -> &'static Mutex<Option<oneshot::Sender<()>>> {
    LOGIN_CANCEL.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthLoginResult {
    pub success: bool,
    pub message: String,
    pub has_token: bool,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStatus {
    pub has_token: bool,
    pub has_refresh_token: bool,
    pub expires_at: Option<i64>,
    pub expired: bool,
    pub needs_refresh: bool,
}

/// Start an OAuth login flow: opens a browser URL and waits for callback.
/// Returns the auth URL that the frontend should open in the user's browser.
pub async fn oauth_login(service: &str, id: &str) -> AppResult<OAuthLoginResult> {
    token_store::ensure_service(service)?;
    crate::validation::validate_account_id(id)?;

    // Cancel any existing login
    cancel_login_impl();

    let (auth_url, cancel_tx, handle) = match service {
        "claude" => oauth::claude::start_login().await.map_err(AppError::from)?,
        "codex" => oauth::codex::start_login().await.map_err(AppError::from)?,
        _ => return Err(AppError::UnsupportedService),
    };

    // Store cancel sender
    {
        let mut lock = cancel_store().lock().map_err(|_| AppError::Message("Lock poisoned".into()))?;
        *lock = Some(cancel_tx);
    }

    // Open the browser. This uses the default system browser.
    if let Err(e) = open::that(&auth_url) {
        // Fallback: return the URL for manual opening
        return Err(AppError::Message(format!(
            "Failed to open browser: {e}. Please open this URL manually: {auth_url}"
        )));
    }

    // Wait for the callback (blocking the async task via JoinHandle)
    let result = handle.await.map_err(|e| AppError::Message(format!("Login task failed: {e}")))?;

    // Clear cancel sender
    {
        let mut lock = cancel_store().lock().map_err(|_| AppError::Message("Lock poisoned".into()))?;
        *lock = None;
    }

    match result {
        Ok(mut tokens) => {
            // Store access token
            token_store::set_token(service, id, &tokens.access_token)
                .map_err(|e| AppError::Message(format!("Failed to store token: {e}")))?;

            // Store refresh token
            if let Some(ref refresh) = tokens.refresh_token {
                token_store::set_refresh_token(service, id, refresh)
                    .map_err(|e| AppError::Message(format!("Failed to store refresh token: {e}")))?;
            }

            // Store expiry
            if let Some(exp) = tokens.expires_at {
                token_store::set_expires_at(service, id, exp)
                    .map_err(|e| AppError::Message(format!("Failed to store expiry: {e}")))?;
            }

            let expires_at = tokens.expires_at;
            tokens.access_token.zeroize();

            Ok(OAuthLoginResult {
                success: true,
                message: "Login successful".into(),
                has_token: true,
                expires_at,
            })
        }
        Err(e) => Ok(OAuthLoginResult {
            success: false,
            message: e,
            has_token: token_store::get_token(service, id).is_some(),
            expires_at: token_store::get_expires_at(service, id),
        }),
    }
}

/// Cancel an in-progress OAuth login.
pub fn cancel_login() -> AppResult<crate::ApiOk> {
    cancel_login_impl();
    Ok(crate::ApiOk { ok: true })
}

fn cancel_login_impl() {
    if let Ok(mut lock) = cancel_store().lock() {
        if let Some(tx) = lock.take() {
            let _ = tx.send(());
        }
    }
}

/// Manually trigger a token refresh for an account.
pub async fn refresh_account_token(service: &str, id: &str) -> AppResult<OAuthLoginResult> {
    token_store::ensure_service(service)?;
    crate::validation::validate_account_id(id)?;

    match token_refresh::do_refresh(service, id).await {
        Ok(_) => Ok(OAuthLoginResult {
            success: true,
            message: "Token refreshed".into(),
            has_token: true,
            expires_at: token_store::get_expires_at(service, id),
        }),
        Err(e) => Ok(OAuthLoginResult {
            success: false,
            message: e,
            has_token: token_store::get_token(service, id).is_some(),
            expires_at: token_store::get_expires_at(service, id),
        }),
    }
}

/// Get token status for an account.
pub fn get_token_status(service: &str, id: &str) -> AppResult<TokenStatus> {
    token_store::ensure_service(service)?;
    crate::validation::validate_account_id(id)?;

    let has_token = token_store::get_token(service, id).is_some();
    let has_refresh_token = token_store::get_refresh_token(service, id).is_some();
    let expires_at = token_store::get_expires_at(service, id);
    let now = now_millis();
    let expired = expires_at.map_or(false, |exp| now >= exp);
    let needs_refresh = expires_at.map_or(false, |exp| now >= exp - 5 * 60 * 1000);

    Ok(TokenStatus {
        has_token,
        has_refresh_token,
        expires_at,
        expired,
        needs_refresh,
    })
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
