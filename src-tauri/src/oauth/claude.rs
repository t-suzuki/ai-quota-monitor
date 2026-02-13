use super::pkce;
use super::OAuthTokens;
use std::time::Duration;

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTH_URL: &str = "https://claude.ai/oauth/authorize";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const REDIRECT_URI: &str = "https://platform.claude.com/oauth/code/callback";
const SCOPE: &str = "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers";

/// Build the authorization URL for Claude OAuth.
/// We currently run a two-step/manual flow:
/// 1) Open browser for authorization
/// 2) User pastes code (or callback URL) back into the app
pub fn build_auth_url() -> (String, String, String) {
    let pkce = pkce::generate();
    let state = pkce::random_state();

    let auth_url = format!(
        "{AUTH_URL}?code=true&client_id={CLIENT_ID}&response_type=code&redirect_uri={redirect}&scope={scope}&code_challenge={challenge}&code_challenge_method=S256&state={state}",
        redirect = url_encode(REDIRECT_URI),
        scope = encode_scope_query_value(SCOPE),
        challenge = pkce.challenge,
        state = state,
    );

    (auth_url, pkce.verifier, state)
}

/// Exchange the authorization code for tokens.
/// Accepts raw code or a full callback URL pasted by the user.
pub async fn exchange_code(raw_code: &str, verifier: &str, expected_state: Option<&str>) -> Result<OAuthTokens, String> {
    let (code, input_state) = extract_code_and_state(raw_code)
        .ok_or_else(|| "Authorization code not found in input".to_string())?;

    if let (Some(expected), Some(actual)) = (expected_state, input_state.as_deref()) {
        if expected != actual {
            return Err("OAuth state mismatch (possible CSRF)".to_string());
        }
    }

    let state = input_state
        .or_else(|| expected_state.map(ToString::to_string))
        .ok_or_else(|| "OAuth state not found in input".to_string())?;

    let params = serde_json::json!({
        "grant_type": "authorization_code",
        "code": code,
        "code_verifier": verifier,
        "client_id": CLIENT_ID,
        "redirect_uri": REDIRECT_URI,
        "state": state,
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post(TOKEN_URL)
        .json(&params)
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
    let params = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_tok,
        "client_id": CLIENT_ID,
        // Claude CLI sends this scope set on refresh.
        "scope": "user:profile user:inference user:sessions:claude_code user:mcp_servers",
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post(TOKEN_URL)
        .json(&params)
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

fn encode_scope_query_value(scope: &str) -> String {
    scope
        .split_whitespace()
        .map(url_encode)
        .collect::<Vec<_>>()
        .join("+")
}

const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

fn extract_code(raw: &str) -> Option<String> {
    extract_code_and_state(raw).map(|(code, _state)| code)
}

fn extract_code_and_state(raw: &str) -> Option<(String, Option<String>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((code, state)) = extract_code_and_state_from_query_like(trimmed) {
        return Some((code, state));
    }

    if let Some((head, _tail)) = trimmed.split_once('#') {
        if !head.trim().is_empty() {
            let mut state: Option<String> = None;
            let tail = _tail.trim();
            if !tail.is_empty() {
                if let Some((_c, s)) = extract_code_and_state_from_query_like(tail) {
                    state = s;
                } else if !tail.contains('=') {
                    state = Some(tail.to_string());
                }
            }
            return Some((head.trim().to_string(), state));
        }
    }

    Some((trimmed.to_string(), None))
}

fn extract_code_and_state_from_query_like(s: &str) -> Option<(String, Option<String>)> {
    let mut candidates = Vec::with_capacity(2);

    if let Some((_, frag)) = s.split_once('#') {
        candidates.push(frag);
    }
    if let Some((_, query)) = s.split_once('?') {
        candidates.push(query);
    }
    candidates.push(s);

    for candidate in candidates {
        let mut code: Option<String> = None;
        let mut state: Option<String> = None;
        for pair in candidate.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                if key == "code" && !value.is_empty() {
                    code = Some(url_decode(value));
                } else if key == "state" && !value.is_empty() {
                    state = Some(url_decode(value));
                }
            }
        }
        if let Some(code) = code {
            return Some((code, state));
        }
    }

    None
}

fn url_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.as_bytes().iter().copied();
    while let Some(b) = it.next() {
        match b {
            b'%' => {
                let hi = it.next().and_then(hex_val);
                let lo = it.next().and_then(hex_val);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push((h << 4 | l) as char);
                }
            }
            b'+' => out.push(' '),
            _ => out.push(b as char),
        }
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_auth_url, encode_scope_query_value, extract_code, extract_code_and_state,
        url_encode, AUTH_URL, CLIENT_ID, REDIRECT_URI, SCOPE,
    };

    #[test]
    fn extract_plain_code() {
        assert_eq!(extract_code("abc123").as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_code_state_fragment_style() {
        assert_eq!(extract_code("abc123#statexyz").as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_from_callback_query() {
        let input = "https://platform.claude.com/oauth/code/callback?code=abc123&state=xyz789";
        assert_eq!(extract_code(input).as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_from_fragment_query() {
        let input = "https://example.com/#code=abc123&state=xyz789";
        assert_eq!(extract_code(input).as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_code_and_state_from_manual_fragment_style() {
        let input = "abc123#xyz789";
        assert_eq!(
            extract_code_and_state(input),
            Some(("abc123".to_string(), Some("xyz789".to_string())))
        );
    }

    #[test]
    fn extract_code_and_state_from_callback_query() {
        let input = "https://platform.claude.com/oauth/code/callback?code=abc123&state=xyz789";
        assert_eq!(
            extract_code_and_state(input),
            Some(("abc123".to_string(), Some("xyz789".to_string())))
        );
    }

    #[test]
    fn build_auth_url_matches_claude_cli_shape() {
        let (auth_url, verifier, state) = build_auth_url();
        assert!(!verifier.is_empty());
        assert!(!state.is_empty());
        assert!(auth_url.starts_with(AUTH_URL));
        assert!(auth_url.contains("code=true"));
        assert!(auth_url.contains(&format!("client_id={CLIENT_ID}")));
        assert!(auth_url.contains("response_type=code"));
        assert!(auth_url.contains(&format!(
            "redirect_uri={}",
            url_encode(REDIRECT_URI)
        )));
        assert!(auth_url.contains(&format!(
            "scope={}",
            encode_scope_query_value(SCOPE)
        )));
        assert!(!auth_url.contains("%20"));
        assert!(auth_url.contains("code_challenge="));
        assert!(auth_url.contains("code_challenge_method=S256"));
        assert!(auth_url.contains("state="));
    }
}
