use super::callback_server::{bind_callback_listener, wait_for_callback};
use super::pkce;
use super::OAuthTokens;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::oneshot;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTH_URL: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const SCOPE: &str = "openid profile email offline_access";

/// Build the authorization URL and start the callback listener.
/// Returns (auth_url, cancel_sender, token_future).
pub async fn start_login() -> Result<(String, oneshot::Sender<()>, tokio::task::JoinHandle<Result<OAuthTokens, String>>), String> {
    let pkce = pkce::generate();
    let state = pkce::random_state();

    let (listener, port) = bind_callback_listener()
        .map_err(|e| format!("Failed to start callback server: {e}"))?;

    let redirect_uri = format!("http://localhost:{port}/callback");

    let auth_url = format!(
        "{AUTH_URL}?response_type=code&client_id={CLIENT_ID}&redirect_uri={redirect}&scope={scope}&code_challenge={challenge}&code_challenge_method=S256&state={state}",
        redirect = url_encode(&redirect_uri),
        scope = url_encode(SCOPE),
        challenge = pkce.challenge,
        state = state,
    );

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let expected_state = state.clone();
    let verifier = pkce.verifier.clone();

    let handle = tokio::task::spawn_blocking(move || {
        let callback = wait_for_callback(listener, cancel_rx)?;

        if callback.state != expected_state {
            return Err("OAuth state mismatch (possible CSRF)".into());
        }

        // We need to block on the async token exchange from within spawn_blocking.
        // Use a new tokio runtime for this isolated HTTP call.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to create runtime: {e}"))?;

        rt.block_on(exchange_code(&callback.code, &verifier, &redirect_uri))
    });

    Ok((auth_url, cancel_tx, handle))
}

async fn exchange_code(code: &str, verifier: &str, redirect_uri: &str) -> Result<OAuthTokens, String> {
    let mut params = HashMap::new();
    params.insert("grant_type", "authorization_code");
    params.insert("code", code);
    params.insert("code_verifier", verifier);
    params.insert("client_id", CLIENT_ID);
    params.insert("redirect_uri", redirect_uri);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read token response: {e}"))?;

    if !status.is_success() {
        return Err(format!("Token exchange failed (HTTP {status}): {body}"));
    }

    parse_token_response(&body)
}

pub async fn refresh_token(refresh_token: &str) -> Result<OAuthTokens, String> {
    let mut params = HashMap::new();
    params.insert("grant_type", "refresh_token");
    params.insert("refresh_token", refresh_token);
    params.insert("client_id", CLIENT_ID);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post(TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token refresh request failed: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read refresh response: {e}"))?;

    if !status.is_success() {
        return Err(format!("Token refresh failed (HTTP {status}): {body}"));
    }

    parse_token_response(&body)
}

fn parse_token_response(body: &str) -> Result<OAuthTokens, String> {
    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Invalid token JSON: {e}"))?;

    let access_token = json["access_token"]
        .as_str()
        .ok_or("Missing access_token in response")?
        .to_string();

    let refresh = json["refresh_token"].as_str().map(|s| s.to_string());

    let expires_at = json["expires_in"]
        .as_i64()
        .map(|secs| chrono_now_millis() + secs * 1000);

    Ok(OAuthTokens {
        access_token,
        refresh_token: refresh,
        expires_at,
    })
}

fn chrono_now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(HEX_UPPER[(b >> 4) as usize] as char);
                out.push(HEX_UPPER[(b & 0xf) as usize] as char);
            }
        }
    }
    out
}

const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";
