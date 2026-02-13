use crate::usage_parser::{parse_claude_usage, parse_codex_usage, UsageWindow};
use crate::validation::{validate_token, validate_upstream_url};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const HTTP_REQUEST_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize)]
pub struct FetchUsageResponse {
    pub raw: Value,
    pub windows: Vec<UsageWindow>,
}

#[derive(Debug, Clone)]
struct RawUpstream {
    ok: bool,
    status: u16,
    content_type: String,
    body: String,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Unsupported service")]
    UnsupportedService,
    #[error("{0}")]
    Validation(String),
    #[error("Failed to build HTTP client: {0}")]
    ClientBuild(reqwest::Error),
    #[error("Failed upstream request: {0}")]
    Request(reqwest::Error),
    #[error("Failed to read upstream response body: {0}")]
    ResponseBody(reqwest::Error),
    #[error("Upstream returned non-JSON response")]
    NonJson,
    #[error("{0}")]
    Upstream(String),
}

pub(crate) fn build_error_message(status: u16, content_type: &str) -> String {
    if status == 403 && content_type.contains("text/html") {
        return "Upstream blocked request (OpenAI edge / Cloudflare)".to_string();
    }

    match status {
        400 => "Upstream rejected request (HTTP 400)".to_string(),
        401 => "Authentication failed (HTTP 401)".to_string(),
        403 => "Permission denied by upstream (HTTP 403)".to_string(),
        404 => "Upstream endpoint not found (HTTP 404)".to_string(),
        429 => "Upstream rate limit exceeded (HTTP 429)".to_string(),
        500..=599 => format!("Upstream server error (HTTP {status})"),
        _ => format!("Upstream request failed (HTTP {status})"),
    }
}

async fn fetch_usage_raw(url: &str, headers: HeaderMap) -> Result<RawUpstream, ApiError> {
    validate_upstream_url(url).map_err(ApiError::Validation)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_REQUEST_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(ApiError::ClientBuild)?;
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(ApiError::Request)?;

    let status = response.status().as_u16();
    let ok = response.status().is_success();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    let body = response.text().await.map_err(ApiError::ResponseBody)?;

    Ok(RawUpstream {
        ok,
        status,
        content_type,
        body,
    })
}

async fn fetch_claude_usage_raw(token: &str, anthropic_oauth_beta: &str) -> Result<RawUpstream, ApiError> {
    let token = token.trim();
    validate_token(token).map_err(ApiError::Validation)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|e| ApiError::Validation(format!("Invalid authorization header: {e}")))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_str(anthropic_oauth_beta)
            .map_err(|e| ApiError::Validation(format!("Invalid anthropic-beta header: {e}")))?,
    );

    fetch_usage_raw(CLAUDE_USAGE_URL, headers).await
}

async fn fetch_codex_usage_raw(token: &str) -> Result<RawUpstream, ApiError> {
    let token = token.trim();
    validate_token(token).map_err(ApiError::Validation)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|e| ApiError::Validation(format!("Invalid authorization header: {e}")))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    fetch_usage_raw(CODEX_USAGE_URL, headers).await
}

pub async fn fetch_normalized_usage(
    service: &str,
    token: &str,
    anthropic_oauth_beta: &str,
) -> Result<FetchUsageResponse, ApiError> {
    let raw = match service {
        "claude" => fetch_claude_usage_raw(token, anthropic_oauth_beta).await?,
        "codex" => fetch_codex_usage_raw(token).await?,
        _ => return Err(ApiError::UnsupportedService),
    };

    if !raw.ok {
        return Err(ApiError::Upstream(build_error_message(
            raw.status,
            &raw.content_type,
        )));
    }

    let parsed: Value = serde_json::from_str(&raw.body).map_err(|_| ApiError::NonJson)?;
    let windows = match service {
        "claude" => parse_claude_usage(&parsed),
        "codex" => parse_codex_usage(&parsed),
        _ => Vec::new(),
    };

    Ok(FetchUsageResponse {
        raw: parsed,
        windows,
    })
}
