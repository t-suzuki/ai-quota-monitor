use crate::error::{AppError, AppResult};
use crate::oauth;
use crate::token_refresh;
use crate::token_store;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
    /// Authorization URL to open in a browser (preferred: user copies into their browser of choice).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    /// For Claude two-step flow: signals the frontend to show a code input dialog.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_code: Option<bool>,
    /// For async flows (Codex): login has started and we're waiting for callback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<bool>,
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
/// - For Claude: opens browser and returns needs_code=true for the two-step flow.
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

    Ok(OAuthLoginResult {
        success: false,
        message: "ログインURLをブラウザで開いて認証してください。表示された `code#state`（またはリダイレクト先URL全体）を貼り付けてください。".into(),
        has_token: false,
        expires_at: None,
        auth_url: Some(auth_url),
        needs_code: Some(true),
        pending: Some(true),
    })
}

/// Import Claude CLI credentials from ~/.claude/.credentials.json.
pub fn import_claude_cli_credentials(service: &str, id: &str) -> AppResult<OAuthLoginResult> {
    token_store::ensure_service(service)?;
    crate::validation::validate_account_id(id)?;
    if service != "claude" {
        return Err(AppError::Message(
            "Claude CLI credentials import is only supported for Claude".into(),
        ));
    }

    let path = find_existing_claude_credentials_path()
        .ok_or_else(|| AppError::Message("Claude CLI認証情報ファイルが見つかりませんでした".into()))?;

    let raw = std::fs::read_to_string(&path).map_err(|e| {
        AppError::Message(format!("Claude credentials を読み取れませんでした ({}): {e}", path.display()))
    })?;

    let parsed: ClaudeCredentialFile = serde_json::from_str(&raw).map_err(|e| {
        AppError::Message(format!("Claude credentials の形式が不正です ({}): {e}", path.display()))
    })?;

    let oauth = parsed
        .claude_ai_oauth
        .ok_or_else(|| AppError::Message(format!("claudeAiOauth が見つかりません ({})", path.display())))?;

    if oauth.access_token.trim().is_empty() {
        return Err(AppError::Message(format!(
            "accessToken が空です ({})",
            path.display()
        )));
    }

    let mut tokens = oauth::OAuthTokens {
        access_token: oauth.access_token,
        refresh_token: oauth.refresh_token,
        expires_at: oauth.expires_at,
    };
    let expires_at = tokens.expires_at;
    store_tokens("claude", id, &mut tokens)?;

    Ok(OAuthLoginResult {
        success: true,
        message: "Claude CLIの認証情報を取り込みました".into(),
        has_token: true,
        expires_at,
        auth_url: None,
        needs_code: None,
        pending: None,
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

    // Run the callback wait + token exchange in the background so we can return the URL immediately.
    let id_owned = id.to_string();
    let id_for_task = id_owned.clone();
    tauri::async_runtime::spawn(async move {
        let result = handle
            .await
            .map_err(|e| format!("Login task failed: {e}"))
            .and_then(|x| x);

        if let Ok(mut lock) = cancel_store().lock() {
            *lock = None;
        }

        match result {
            Ok(mut tokens) => {
                let _ = store_tokens("codex", &id_for_task, &mut tokens);
            }
            Err(_) => {
                // Frontend polls token status; errors will show as timeout unless we add a status API.
            }
        }
    });

    Ok(OAuthLoginResult {
        success: false,
        message: "ログインURLをブラウザで開いて認証してください（完了を待っています）".into(),
        has_token: token_store::get_token("codex", &id_owned).is_some(),
        expires_at: token_store::get_expires_at("codex", &id_owned),
        auth_url: Some(auth_url),
        needs_code: None,
        pending: Some(true),
    })
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

    let (verifier, state) = {
        let mut lock = claude_pending_store()
            .lock()
            .map_err(|_| AppError::Message("Lock poisoned".into()))?;
        lock.take()
            .ok_or_else(|| AppError::Message("No pending Claude login. Start login first.".into()))?
    };

    let result = oauth::claude::exchange_code(code, &verifier, Some(&state)).await;

    match result {
        Ok(mut tokens) => {
            store_tokens(service, id, &mut tokens)?;
            Ok(OAuthLoginResult {
                success: true,
                message: "ログイン成功".into(),
                has_token: true,
                expires_at: tokens.expires_at,
                auth_url: None,
                needs_code: None,
                pending: None,
            })
        }
        Err(e) => Ok(OAuthLoginResult {
            success: false,
            message: e,
            has_token: token_store::get_token(service, id).is_some(),
            expires_at: token_store::get_expires_at(service, id),
            auth_url: None,
            needs_code: None,
            pending: None,
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
            auth_url: None,
            needs_code: None,
            pending: None,
        }),
        Err(e) => Ok(OAuthLoginResult {
            success: false,
            message: e,
            has_token: token_store::get_token(service, id).is_some(),
            expires_at: token_store::get_expires_at(service, id),
            auth_url: None,
            needs_code: None,
            pending: None,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCredentialFile {
    claude_ai_oauth: Option<ClaudeCredentialOauth>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCredentialOauth {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
}

fn find_existing_claude_credentials_path() -> Option<PathBuf> {
    for candidate in claude_credentials_candidates() {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn claude_credentials_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            out.push(PathBuf::from(dir).join(".credentials.json"));
        }
    }

    for key in ["HOME", "USERPROFILE"] {
        if let Ok(home) = std::env::var(key) {
            let home = home.trim();
            if !home.is_empty() {
                out.push(PathBuf::from(home).join(".claude").join(".credentials.json"));
            }
        }
    }

    out
}
