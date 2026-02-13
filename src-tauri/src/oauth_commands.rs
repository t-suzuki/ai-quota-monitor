use crate::error::{AppError, AppResult};
use crate::oauth;
use crate::token_refresh;
use crate::token_store;
use serde::Serialize;
use std::sync::Mutex;
use tokio::sync::oneshot;
use zeroize::Zeroize;

/// Global cancel sender for in-progress Codex login.
static LOGIN_CANCEL: std::sync::OnceLock<Mutex<Option<oneshot::Sender<()>>>> =
    std::sync::OnceLock::new();

/// Global PKCE verifier for in-progress Claude login (two-step flow).
static CLAUDE_PENDING: std::sync::OnceLock<Mutex<Option<(String, String)>>> =
    std::sync::OnceLock::new();

fn cancel_store() -> &'static Mutex<Option<oneshot::Sender<()>>> {
    LOGIN_CANCEL.get_or_init(|| Mutex::new(None))
}

fn claude_pending_store() -> &'static Mutex<Option<(String, String)>> {
    CLAUDE_PENDING.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthLoginResult {
    pub success: bool,
    pub message: String,
    pub has_token: bool,
    pub expires_at: Option<i64>,
    /// For Claude two-step flow: signals the frontend to show a code input dialog.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_code: Option<bool>,
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

/// Start an OAuth login flow.
/// - For Codex: opens browser, waits for local callback, returns completed result.
/// - For Claude: opens browser, returns needs_code=true. Frontend then calls
///   `oauth_exchange_code` with the code from the callback page.
pub async fn oauth_login(service: &str, id: &str) -> AppResult<OAuthLoginResult> {
    token_store::ensure_service(service)?;
    crate::validation::validate_account_id(id)?;

    cancel_login_impl();

    match service {
        "claude" => oauth_login_claude(),
        "codex" => oauth_login_codex(id).await,
        _ => Err(AppError::UnsupportedService),
    }
}

fn oauth_login_claude() -> AppResult<OAuthLoginResult> {
    let (auth_url, verifier, state) = oauth::claude::build_auth_url();

    // Store verifier + state for the second step
    {
        let mut lock = claude_pending_store()
            .lock()
            .map_err(|_| AppError::Message("Lock poisoned".into()))?;
        *lock = Some((verifier, state));
    }

    if let Err(e) = open::that(&auth_url) {
        return Err(AppError::Message(format!(
            "ブラウザを開けませんでした: {e}\nこのURLを手動で開いてください: {auth_url}"
        )));
    }

    Ok(OAuthLoginResult {
        success: false,
        message: "ブラウザで認証してください。表示されたコードをペーストしてください。".into(),
        has_token: false,
        expires_at: None,
        needs_code: Some(true),
    })
}

async fn oauth_login_codex(id: &str) -> AppResult<OAuthLoginResult> {
    let (auth_url, cancel_tx, handle) = oauth::codex::start_login()
        .await
        .map_err(AppError::from)?;

    {
        let mut lock = cancel_store()
            .lock()
            .map_err(|_| AppError::Message("Lock poisoned".into()))?;
        *lock = Some(cancel_tx);
    }

    if let Err(e) = open::that(&auth_url) {
        return Err(AppError::Message(format!(
            "ブラウザを開けませんでした: {e}\nこのURLを手動で開いてください: {auth_url}"
        )));
    }

    let result = handle
        .await
        .map_err(|e| AppError::Message(format!("Login task failed: {e}")))?;

    {
        let mut lock = cancel_store()
            .lock()
            .map_err(|_| AppError::Message("Lock poisoned".into()))?;
        *lock = None;
    }

    match result {
        Ok(mut tokens) => {
            store_tokens("codex", id, &mut tokens)?;
            Ok(OAuthLoginResult {
                success: true,
                message: "ログイン成功".into(),
                has_token: true,
                expires_at: tokens.expires_at,
                needs_code: None,
            })
        }
        Err(e) => Ok(OAuthLoginResult {
            success: false,
            message: e,
            has_token: token_store::get_token("codex", id).is_some(),
            expires_at: token_store::get_expires_at("codex", id),
            needs_code: None,
        }),
    }
}

/// Second step for Claude OAuth: exchange the authorization code.
pub async fn oauth_exchange_code(
    service: &str,
    id: &str,
    code: &str,
) -> AppResult<OAuthLoginResult> {
    token_store::ensure_service(service)?;
    crate::validation::validate_account_id(id)?;

    if service != "claude" {
        return Err(AppError::Message(
            "Code exchange is only supported for Claude".into(),
        ));
    }

    let (verifier, _state) = {
        let mut lock = claude_pending_store()
            .lock()
            .map_err(|_| AppError::Message("Lock poisoned".into()))?;
        lock.take()
            .ok_or_else(|| AppError::Message("No pending Claude login. Start login first.".into()))?
    };

    let result = oauth::claude::exchange_code(code, &verifier).await;

    match result {
        Ok(mut tokens) => {
            store_tokens(service, id, &mut tokens)?;
            Ok(OAuthLoginResult {
                success: true,
                message: "ログイン成功".into(),
                has_token: true,
                expires_at: tokens.expires_at,
                needs_code: None,
            })
        }
        Err(e) => Ok(OAuthLoginResult {
            success: false,
            message: e,
            has_token: token_store::get_token(service, id).is_some(),
            expires_at: token_store::get_expires_at(service, id),
            needs_code: None,
        }),
    }
}

fn store_tokens(
    service: &str,
    id: &str,
    tokens: &mut oauth::OAuthTokens,
) -> AppResult<()> {
    token_store::set_token(service, id, &tokens.access_token)
        .map_err(|e| AppError::Message(format!("Failed to store token: {e}")))?;

    if let Some(ref refresh) = tokens.refresh_token {
        token_store::set_refresh_token(service, id, refresh)
            .map_err(|e| AppError::Message(format!("Failed to store refresh token: {e}")))?;
    }

    if let Some(exp) = tokens.expires_at {
        token_store::set_expires_at(service, id, exp)
            .map_err(|e| AppError::Message(format!("Failed to store expiry: {e}")))?;
    }

    tokens.access_token.zeroize();
    Ok(())
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
    // Also clear pending Claude state
    if let Ok(mut lock) = claude_pending_store().lock() {
        *lock = None;
    }
}

/// Manually trigger a token refresh for an account.
pub async fn refresh_account_token(service: &str, id: &str) -> AppResult<OAuthLoginResult> {
    token_store::ensure_service(service)?;
    crate::validation::validate_account_id(id)?;

    match token_refresh::do_refresh(service, id).await {
        Ok(_) => Ok(OAuthLoginResult {
            success: true,
            message: "トークンを更新しました".into(),
            has_token: true,
            expires_at: token_store::get_expires_at(service, id),
            needs_code: None,
        }),
        Err(e) => Ok(OAuthLoginResult {
            success: false,
            message: e,
            has_token: token_store::get_token(service, id).is_some(),
            expires_at: token_store::get_expires_at(service, id),
            needs_code: None,
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
