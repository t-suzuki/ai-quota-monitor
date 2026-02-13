use super::pkce;
use super::OAuthTokens;
use std::collections::HashMap;
use std::time::Duration;

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTH_URL: &str = "https://claude.ai/oauth/authorize";
const TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
const REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";
const SCOPE: &str = "org:create_api_key user:profile user:inference";

/// Build the authorization URL for Claude OAuth.
/// Claude uses a hosted redirect (console.anthropic.com), not a local callback server.
/// The user will complete auth in the browser, then the callback page displays the code.
/// We poll a clipboard / manual input approach — the callback page shows the code to the user.
///
/// Returns the auth URL. The callback page will show the authorization code.
pub fn build_auth_url() -> (String, String, String) {
    let pkce = pkce::generate();
    let state = pkce::random_state();

    let auth_url = format!(
        "{AUTH_URL}?response_type=code&client_id={CLIENT_ID}&redirect_uri={redirect}&scope={scope}&code_challenge={challenge}&code_challenge_method=S256&state={state}",
        redirect = url_encode(REDIRECT_URI),
        scope = url_encode(SCOPE),
        challenge = pkce.challenge,
        state = state,
    );

    (auth_url, pkce.verifier, state)
}

/// Exchange the authorization code for tokens.
/// Claude's callback page returns code#state — the caller should split on '#' if present.
pub async fn exchange_code(raw_code: &str, verifier: &str) -> Result<OAuthTokens, String> {
    // Claude returns code#state — extract just the code part
    let code = if let Some(pos) = raw_code.find('#') {
        &raw_code[..pos]
    } else {
        raw_code
    };

    let mut params = HashMap::new();
    params.insert("grant_type", "authorization_code");
    params.insert("code", code);
    params.insert("code_verifier", verifier);
    params.insert("client_id", CLIENT_ID);
    params.insert("redirect_uri", REDIRECT_URI);

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

pub async fn refresh_token(refresh_tok: &str) -> Result<OAuthTokens, String> {
    let mut params = HashMap::new();
    params.insert("grant_type", "refresh_token");
    params.insert("refresh_token", refresh_tok);
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
    let resp_body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read refresh response: {e}"))?;

    if !status.is_success() {
        return Err(format!("Token refresh failed (HTTP {status}): {resp_body}"));
    }

    parse_token_response(&resp_body)
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
        .map(|secs| now_millis() + secs * 1000);

    Ok(OAuthTokens {
        access_token,
        refresh_token: refresh,
        expires_at,
    })
}

fn now_millis() -> i64 {
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
