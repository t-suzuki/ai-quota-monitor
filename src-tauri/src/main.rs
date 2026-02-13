#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Position, Size, WebviewWindow};
use usage_parser::{parse_claude_usage, parse_codex_usage, UsageWindow};
use validation::{
    enforce_fetch_usage_rate_limit, validate_account_id, validate_account_name, validate_token,
    validate_upstream_url,
};
use zeroize::Zeroize;
mod usage_parser;
mod validation;

const APP_NAME: &str = "AI Quota Monitor";
const STORE_FILE: &str = "accounts.json";
const ANTHROPIC_OAUTH_BETA: &str = "oauth-2025-04-20";
const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const HTTP_REQUEST_TIMEOUT_SECS: u64 = 30;

const NORMAL_WINDOW_DEFAULT_W: i32 = 1100;
const NORMAL_WINDOW_DEFAULT_H: i32 = 840;
const NORMAL_WINDOW_MIN_W: i32 = 980;
const NORMAL_WINDOW_MIN_H: i32 = 700;

const MINIMAL_CARD_WIDTH: i32 = 290;
const MINIMAL_PAD: i32 = 16;
const MINIMAL_MIN_W_DEFAULT: i32 = MINIMAL_CARD_WIDTH + MINIMAL_PAD;
const MINIMAL_WINDOW_DEFAULT_W: i32 = MINIMAL_MIN_W_DEFAULT + 74;
const MINIMAL_WINDOW_DEFAULT_H: i32 = 420;
const MINIMAL_WINDOW_MIN_H_DEFAULT: i32 = 240;
const MINIMAL_FLOOR_W: i32 = MINIMAL_CARD_WIDTH - 40;
const MINIMAL_FLOOR_H: i32 = 220;
static STORE_CACHE: OnceLock<Mutex<Option<Store>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountEntry {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Services {
    claude: Vec<AccountEntry>,
    codex: Vec<AccountEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollingState {
    active: bool,
    started_at: Option<i64>,
    interval: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bounds {
    width: i32,
    height: i32,
    x: Option<i32>,
    y: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowState {
    mode: String,
    normal_bounds: Bounds,
    minimal_bounds: Option<Bounds>,
    minimal_min_width: i32,
    minimal_min_height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotifySettings {
    critical: bool,
    recovery: bool,
    warning: bool,
    threshold_warning: i32,
    threshold_critical: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    poll_interval: i32,
    polling_state: PollingState,
    window_state: WindowState,
    notify_settings: NotifySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Store {
    services: Services,
    settings: Settings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountEntryRaw {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ServicesRaw {
    claude: Option<Vec<AccountEntryRaw>>,
    codex: Option<Vec<AccountEntryRaw>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollingStateRaw {
    active: Option<bool>,
    started_at: Option<i64>,
    interval: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BoundsRaw {
    width: Option<i32>,
    height: Option<i32>,
    x: Option<i32>,
    y: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowStateRaw {
    mode: Option<String>,
    normal_bounds: Option<BoundsRaw>,
    minimal_bounds: Option<BoundsRaw>,
    minimal_min_width: Option<i32>,
    minimal_min_height: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotifySettingsRaw {
    critical: Option<bool>,
    recovery: Option<bool>,
    warning: Option<bool>,
    threshold_warning: Option<i32>,
    threshold_critical: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsRaw {
    poll_interval: Option<i32>,
    polling_state: Option<PollingStateRaw>,
    window_state: Option<WindowStateRaw>,
    notify_settings: Option<NotifySettingsRaw>,
}

#[derive(Debug, Clone, Deserialize)]
struct StoreRaw {
    services: Option<ServicesRaw>,
    settings: Option<SettingsRaw>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountSnapshotEntry {
    id: String,
    name: String,
    has_token: bool,
}

#[derive(Debug, Clone, Serialize)]
struct AccountsSnapshot {
    claude: Vec<AccountSnapshotEntry>,
    codex: Vec<AccountSnapshotEntry>,
    settings: Settings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiOk {
    ok: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveAccountPayload {
    service: Option<String>,
    id: Option<String>,
    name: Option<String>,
    token: Option<String>,
    clear_token: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteAccountPayload {
    service: Option<String>,
    id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotifySettingsPatch {
    critical: Option<bool>,
    recovery: Option<bool>,
    warning: Option<bool>,
    threshold_warning: Option<i32>,
    threshold_critical: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetSettingsPayload {
    poll_interval: Option<i32>,
    notify_settings: Option<NotifySettingsPatch>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetPollingStatePayload {
    active: Option<bool>,
    started_at: Option<i64>,
    interval: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchUsagePayload {
    service: Option<String>,
    id: Option<String>,
    name: Option<String>,
    token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetWindowModePayload {
    mode: Option<String>,
    min_width: Option<i32>,
    min_height: Option<i32>,
    preferred_width: Option<i32>,
    preferred_height: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetWindowPositionPayload {
    x: Option<i32>,
    y: Option<i32>,
    width: Option<i32>,
    height: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
struct FetchUsageResponse {
    raw: Value,
    windows: Vec<UsageWindow>,
}

#[derive(Debug, Clone)]
struct RawUpstream {
    ok: bool,
    status: u16,
    content_type: String,
    body: String,
}

fn clamp_int(value: Option<i32>, fallback: i32, min: i32, max: i32) -> i32 {
    let n = value.unwrap_or(fallback);
    n.clamp(min, max)
}

fn sanitize_string(input: Option<&str>, fallback: &str) -> String {
    let v = input.unwrap_or("").trim();
    if v.is_empty() {
        fallback.to_string()
    } else {
        v.to_string()
    }
}

fn default_normal_bounds() -> Bounds {
    Bounds {
        width: NORMAL_WINDOW_DEFAULT_W,
        height: NORMAL_WINDOW_DEFAULT_H,
        x: None,
        y: None,
    }
}

fn default_minimal_bounds() -> Bounds {
    Bounds {
        width: MINIMAL_WINDOW_DEFAULT_W,
        height: MINIMAL_WINDOW_DEFAULT_H,
        x: None,
        y: None,
    }
}

fn default_store() -> Store {
    Store {
        services: Services {
            claude: Vec::new(),
            codex: Vec::new(),
        },
        settings: Settings {
            poll_interval: 120,
            polling_state: PollingState {
                active: false,
                started_at: None,
                interval: 120,
            },
            window_state: WindowState {
                mode: "normal".to_string(),
                normal_bounds: default_normal_bounds(),
                minimal_bounds: None,
                minimal_min_width: MINIMAL_MIN_W_DEFAULT,
                minimal_min_height: MINIMAL_WINDOW_MIN_H_DEFAULT,
            },
            notify_settings: NotifySettings {
                critical: true,
                recovery: true,
                warning: false,
                threshold_warning: 75,
                threshold_critical: 90,
            },
        },
    }
}

fn sanitize_bounds_raw(
    raw: Option<&BoundsRaw>,
    min_width: i32,
    min_height: i32,
    fallback: &Bounds,
) -> Bounds {
    let width = clamp_int(raw.and_then(|x| x.width), fallback.width, min_width, 8192);
    let height = clamp_int(raw.and_then(|x| x.height), fallback.height, min_height, 8192);
    let x = raw.and_then(|x| x.x);
    let y = raw.and_then(|x| x.y);

    Bounds {
        width,
        height,
        x,
        y,
    }
}

fn sanitize_bounds_live(
    bounds: Option<&Bounds>,
    min_width: i32,
    min_height: i32,
    fallback: &Bounds,
) -> Bounds {
    let source = bounds.unwrap_or(fallback);
    let width = clamp_int(Some(source.width), fallback.width, min_width, 8192);
    let height = clamp_int(Some(source.height), fallback.height, min_height, 8192);
    Bounds {
        width,
        height,
        x: source.x,
        y: source.y,
    }
}

fn normalize_accounts(raw: Option<&Vec<AccountEntryRaw>>, service: &str) -> Vec<AccountEntry> {
    let mut out = Vec::new();
    let Some(entries) = raw else {
        return out;
    };
    for entry in entries {
        let id = sanitize_string(entry.id.as_deref(), "");
        if id.is_empty() {
            continue;
        }
        if validate_account_id(&id).is_err() {
            continue;
        }
        let fallback = format!("{}:{}", service, id);
        let name_candidate = sanitize_string(entry.name.as_deref(), &fallback);
        let name = if validate_account_name(&name_candidate).is_ok() {
            name_candidate
        } else {
            fallback
        };
        out.push(AccountEntry { id, name });
    }
    out
}

fn normalize_store(raw: StoreRaw) -> Store {
    let base = default_store();

    let services_raw = raw.services;
    let settings_raw = raw.settings;

    let claude = normalize_accounts(
        services_raw
            .as_ref()
            .and_then(|s| s.claude.as_ref()),
        "claude",
    );
    let codex = normalize_accounts(
        services_raw
            .as_ref()
            .and_then(|s| s.codex.as_ref()),
        "codex",
    );

    let poll_interval = clamp_int(
        settings_raw.as_ref().and_then(|s| s.poll_interval),
        base.settings.poll_interval,
        30,
        600,
    );

    let polling_raw = settings_raw.as_ref().and_then(|s| s.polling_state.as_ref());
    let polling_interval = clamp_int(
        polling_raw.and_then(|p| p.interval),
        poll_interval,
        30,
        600,
    );
    let polling_started_at = polling_raw.and_then(|p| p.started_at).filter(|n| *n > 0);

    let window_raw = settings_raw.as_ref().and_then(|s| s.window_state.as_ref());
    let mode = match window_raw.and_then(|w| w.mode.as_deref()) {
        Some("minimal") => "minimal".to_string(),
        _ => "normal".to_string(),
    };

    let minimal_min_width = clamp_int(
        window_raw.and_then(|w| w.minimal_min_width),
        MINIMAL_MIN_W_DEFAULT,
        MINIMAL_FLOOR_W,
        4096,
    );
    let minimal_min_height = clamp_int(
        window_raw.and_then(|w| w.minimal_min_height),
        MINIMAL_WINDOW_MIN_H_DEFAULT,
        MINIMAL_FLOOR_H,
        4096,
    );

    let normal_bounds = sanitize_bounds_raw(
        window_raw.and_then(|w| w.normal_bounds.as_ref()),
        NORMAL_WINDOW_MIN_W,
        NORMAL_WINDOW_MIN_H,
        &default_normal_bounds(),
    );

    let minimal_bounds = window_raw
        .and_then(|w| w.minimal_bounds.as_ref())
        .map(|raw_bounds| {
            sanitize_bounds_raw(
                Some(raw_bounds),
                minimal_min_width,
                minimal_min_height,
                &default_minimal_bounds(),
            )
        });

    let notify_raw = settings_raw.as_ref().and_then(|s| s.notify_settings.as_ref());
    let notify_settings = NotifySettings {
        critical: notify_raw
            .and_then(|n| n.critical)
            .unwrap_or(base.settings.notify_settings.critical),
        recovery: notify_raw
            .and_then(|n| n.recovery)
            .unwrap_or(base.settings.notify_settings.recovery),
        warning: notify_raw
            .and_then(|n| n.warning)
            .unwrap_or(base.settings.notify_settings.warning),
        threshold_warning: clamp_int(
            notify_raw.and_then(|n| n.threshold_warning),
            base.settings.notify_settings.threshold_warning,
            1,
            99,
        ),
        threshold_critical: clamp_int(
            notify_raw.and_then(|n| n.threshold_critical),
            base.settings.notify_settings.threshold_critical,
            1,
            99,
        ),
    };

    Store {
        services: Services { claude, codex },
        settings: Settings {
            poll_interval,
            polling_state: PollingState {
                active: polling_raw.and_then(|p| p.active).unwrap_or(false),
                started_at: polling_started_at,
                interval: polling_interval,
            },
            window_state: WindowState {
                mode,
                normal_bounds,
                minimal_bounds,
                minimal_min_width,
                minimal_min_height,
            },
            notify_settings,
        },
    }
}

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create app data directory: {e}"))?;
    dir.push(STORE_FILE);
    Ok(dir)
}

fn store_cache() -> &'static Mutex<Option<Store>> {
    STORE_CACHE.get_or_init(|| Mutex::new(None))
}

fn read_store(app: &AppHandle) -> Result<Store, String> {
    if let Some(cached) = store_cache()
        .lock()
        .map_err(|_| "Store cache lock is poisoned".to_string())?
        .as_ref()
        .cloned()
    {
        return Ok(cached);
    }

    let path = store_path(app)?;
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => {
            let fallback = default_store();
            let mut cache = store_cache()
                .lock()
                .map_err(|_| "Store cache lock is poisoned".to_string())?;
            if cache.is_none() {
                *cache = Some(fallback.clone());
            }
            return Ok(cache.as_ref().cloned().unwrap_or(fallback));
        }
    };

    let parsed = match serde_json::from_str::<StoreRaw>(&raw) {
        Ok(parsed) => parsed,
        Err(_) => {
            let fallback = default_store();
            let mut cache = store_cache()
                .lock()
                .map_err(|_| "Store cache lock is poisoned".to_string())?;
            if cache.is_none() {
                *cache = Some(fallback.clone());
            }
            return Ok(cache.as_ref().cloned().unwrap_or(fallback));
        }
    };

    let normalized = normalize_store(parsed);
    let mut cache = store_cache()
        .lock()
        .map_err(|_| "Store cache lock is poisoned".to_string())?;
    if let Some(existing) = cache.as_ref() {
        return Ok(existing.clone());
    }
    *cache = Some(normalized.clone());
    Ok(normalized)
}

fn write_store(app: &AppHandle, store: &Store) -> Result<(), String> {
    let path = store_path(app)?;
    let body = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize store: {e}"))?;
    fs::write(path, body).map_err(|e| format!("Failed to write store: {e}"))?;
    let mut cache = store_cache()
        .lock()
        .map_err(|_| "Store cache lock is poisoned".to_string())?;
    *cache = Some(store.clone());
    Ok(())
}

fn ensure_service(service: &str) -> Result<(), String> {
    match service {
        "claude" | "codex" => Ok(()),
        _ => Err("Unsupported service".to_string()),
    }
}

fn token_key(service: &str, id: &str) -> String {
    format!("{service}:{id}")
}

fn get_token(service: &str, id: &str) -> Option<String> {
    let entry = keyring::Entry::new(APP_NAME, &token_key(service, id)).ok()?;
    entry.get_password().ok()
}

fn set_token(service: &str, id: &str, token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(APP_NAME, &token_key(service, id))
        .map_err(|e| format!("Failed to open keyring entry: {e}"))?;
    entry
        .set_password(token)
        .map_err(|e| format!("Failed to store token in keyring: {e}"))
}

fn delete_token(service: &str, id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(APP_NAME, &token_key(service, id))
        .map_err(|e| format!("Failed to open keyring entry: {e}"))?;
    match entry.delete_password() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Failed to delete token from keyring: {e}")),
    }
}

fn current_window_bounds(window: &WebviewWindow) -> Result<Bounds, String> {
    let scale_factor = window
        .scale_factor()
        .map_err(|e| format!("Failed to read window scale factor: {e}"))?;

    let size = window
        .inner_size()
        .map_err(|e| format!("Failed to read window size: {e}"))?;
    let logical_size = size.to_logical::<f64>(scale_factor);
    let mut bounds = Bounds {
        width: logical_size.width.round() as i32,
        height: logical_size.height.round() as i32,
        x: None,
        y: None,
    };
    if let Ok(pos) = window.outer_position() {
        let logical_pos = pos.to_logical::<f64>(scale_factor);
        bounds.x = Some(logical_pos.x.round() as i32);
        bounds.y = Some(logical_pos.y.round() as i32);
    }
    Ok(bounds)
}

fn set_window_size(window: &WebviewWindow, width: i32, height: i32) -> Result<(), String> {
    window
        .set_size(Size::Logical(LogicalSize::new(width as f64, height as f64)))
        .map_err(|e| format!("Failed to set window size: {e}"))
}

fn set_window_position_inner(window: &WebviewWindow, x: i32, y: i32) -> Result<(), String> {
    window
        .set_position(Position::Logical(LogicalPosition::new(x as f64, y as f64)))
        .map_err(|e| format!("Failed to set window position: {e}"))
}

fn apply_window_mode(window: &WebviewWindow, ws: &WindowState) -> Result<(), String> {
    let is_minimal = ws.mode == "minimal";
    let min_width = if is_minimal {
        ws.minimal_min_width
    } else {
        NORMAL_WINDOW_MIN_W
    };
    let min_height = if is_minimal {
        ws.minimal_min_height
    } else {
        NORMAL_WINDOW_MIN_H
    };

    let fallback = if is_minimal {
        default_minimal_bounds()
    } else {
        default_normal_bounds()
    };

    let source = if is_minimal {
        ws.minimal_bounds.as_ref().unwrap_or(&fallback)
    } else {
        &ws.normal_bounds
    };

    let bounds = sanitize_bounds_live(Some(source), min_width, min_height, &fallback);

    window
        .set_min_size(Some(Size::Logical(LogicalSize::new(
            min_width as f64,
            min_height as f64,
        ))))
        .map_err(|e| format!("Failed to set minimum window size: {e}"))?;

    window
        .set_decorations(!is_minimal)
        .map_err(|e| format!("Failed to update window decorations: {e}"))?;
    window
        .set_always_on_top(is_minimal)
        .map_err(|e| format!("Failed to update always-on-top: {e}"))?;

    set_window_size(window, bounds.width, bounds.height)?;

    if let (Some(x), Some(y)) = (bounds.x, bounds.y) {
        set_window_position_inner(window, x, y)?;
    }

    Ok(())
}

fn build_error(status: u16, content_type: &str) -> String {
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

async fn fetch_usage_raw(url: &str, headers: HeaderMap) -> Result<RawUpstream, String> {
    validate_upstream_url(url)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_REQUEST_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("Failed upstream request: {e}"))?;

    let status = response.status().as_u16();
    let ok = response.status().is_success();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read upstream response body: {e}"))?;

    Ok(RawUpstream {
        ok,
        status,
        content_type,
        body,
    })
}

async fn fetch_claude_usage_raw(token: &str) -> Result<RawUpstream, String> {
    let token = token.trim();
    validate_token(token)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|e| format!("Invalid authorization header: {e}"))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static(ANTHROPIC_OAUTH_BETA),
    );

    fetch_usage_raw(CLAUDE_USAGE_URL, headers).await
}

async fn fetch_codex_usage_raw(token: &str) -> Result<RawUpstream, String> {
    let token = token.trim();
    validate_token(token)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|e| format!("Invalid authorization header: {e}"))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    fetch_usage_raw(CODEX_USAGE_URL, headers).await
}

async fn fetch_normalized_usage(service: &str, token: &str) -> Result<FetchUsageResponse, String> {
    let raw = match service {
        "claude" => fetch_claude_usage_raw(token).await?,
        "codex" => fetch_codex_usage_raw(token).await?,
        _ => return Err("Unsupported service".to_string()),
    };

    if !raw.ok {
        return Err(build_error(raw.status, &raw.content_type));
    }

    let parsed: Value = serde_json::from_str(&raw.body)
        .map_err(|_| "Upstream returned non-JSON response".to_string())?;
    let windows = match service {
        "claude" => parse_claude_usage(&parsed),
        "codex" => parse_codex_usage(&parsed),
        _ => Vec::new(),
    };

    Ok(FetchUsageResponse { raw: parsed, windows })
}

#[tauri::command]
fn list_accounts(app: AppHandle) -> Result<AccountsSnapshot, String> {
    let store = read_store(&app)?;

    let map_accounts = |service: &str, accounts: &[AccountEntry]| {
        accounts
            .iter()
            .filter_map(|entry| {
                let id = sanitize_string(Some(&entry.id), "");
                if id.is_empty() {
                    return None;
                }
                if validate_account_id(&id).is_err() {
                    return None;
                }
                let fallback = format!("{service}:{id}");
                let name_candidate = sanitize_string(Some(&entry.name), &fallback);
                let name = if validate_account_name(&name_candidate).is_ok() {
                    name_candidate
                } else {
                    fallback
                };
                let has_token = get_token(service, &id).is_some();
                Some(AccountSnapshotEntry {
                    id,
                    name,
                    has_token,
                })
            })
            .collect::<Vec<_>>()
    };

    Ok(AccountsSnapshot {
        claude: map_accounts("claude", &store.services.claude),
        codex: map_accounts("codex", &store.services.codex),
        settings: store.settings,
    })
}

#[tauri::command]
fn save_account(app: AppHandle, mut payload: SaveAccountPayload) -> Result<AccountSnapshotEntry, String> {
    let service = sanitize_string(payload.service.as_deref(), "");
    ensure_service(&service)?;

    let id = sanitize_string(payload.id.as_deref(), "");
    validate_account_id(&id)?;

    let fallback_name = format!("{} {}", service.to_uppercase(), id);
    let name = sanitize_string(payload.name.as_deref(), &fallback_name);
    validate_account_name(&name)?;

    let mut store = read_store(&app)?;
    let list = match service.as_str() {
        "claude" => &mut store.services.claude,
        "codex" => &mut store.services.codex,
        _ => return Err("Unsupported service".to_string()),
    };

    if let Some(existing) = list.iter_mut().find(|x| x.id == id) {
        existing.name = name.clone();
    } else {
        list.push(AccountEntry {
            id: id.clone(),
            name: name.clone(),
        });
    }

    if let Some(mut token_input) = payload.token.take() {
        let mut trimmed = token_input.trim().to_string();
        if !trimmed.is_empty() {
            validate_token(&trimmed)?;
            set_token(&service, &id, &trimmed)?;
        }
        trimmed.zeroize();
        token_input.zeroize();
    }

    if payload.clear_token.unwrap_or(false) {
        delete_token(&service, &id)?;
    }

    write_store(&app, &store)?;

    Ok(AccountSnapshotEntry {
        id: id.clone(),
        name,
        has_token: get_token(&service, &id).is_some(),
    })
}

#[tauri::command]
fn delete_account(app: AppHandle, payload: DeleteAccountPayload) -> Result<ApiOk, String> {
    let service = sanitize_string(payload.service.as_deref(), "");
    ensure_service(&service)?;

    let id = sanitize_string(payload.id.as_deref(), "");
    validate_account_id(&id)?;

    let mut store = read_store(&app)?;
    let list = match service.as_str() {
        "claude" => &mut store.services.claude,
        "codex" => &mut store.services.codex,
        _ => return Err("Unsupported service".to_string()),
    };

    list.retain(|x| x.id != id);
    write_store(&app, &store)?;
    delete_token(&service, &id)?;

    Ok(ApiOk { ok: true })
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<Settings, String> {
    Ok(read_store(&app)?.settings)
}

#[tauri::command]
fn set_settings(app: AppHandle, payload: SetSettingsPayload) -> Result<Settings, String> {
    let mut store = read_store(&app)?;

    if let Some(poll_interval) = payload.poll_interval {
        if (30..=600).contains(&poll_interval) {
            store.settings.poll_interval = poll_interval;
            if !store.settings.polling_state.active {
                store.settings.polling_state.interval = poll_interval;
            }
        }
    }

    if let Some(ns) = payload.notify_settings {
        let current = &mut store.settings.notify_settings;
        if let Some(v) = ns.critical {
            current.critical = v;
        }
        if let Some(v) = ns.recovery {
            current.recovery = v;
        }
        if let Some(v) = ns.warning {
            current.warning = v;
        }
        if let Some(v) = ns.threshold_warning {
            if (1..=99).contains(&v) {
                current.threshold_warning = v;
            }
        }
        if let Some(v) = ns.threshold_critical {
            if (1..=99).contains(&v) {
                current.threshold_critical = v;
            }
        }
    }

    write_store(&app, &store)?;
    Ok(store.settings)
}

#[tauri::command]
fn get_polling_state(app: AppHandle) -> Result<PollingState, String> {
    Ok(read_store(&app)?.settings.polling_state)
}

#[tauri::command]
fn set_polling_state(app: AppHandle, payload: SetPollingStatePayload) -> Result<PollingState, String> {
    let mut store = read_store(&app)?;
    let current = &mut store.settings.polling_state;

    current.active = payload.active.unwrap_or(false);
    current.started_at = payload.started_at.filter(|n| *n > 0);

    if let Some(interval) = payload.interval {
        if (30..=600).contains(&interval) {
            current.interval = interval;
        }
    }

    let out = current.clone();
    write_store(&app, &store)?;
    Ok(out)
}

#[tauri::command]
async fn fetch_usage(app: AppHandle, mut payload: FetchUsagePayload) -> Result<FetchUsageResponse, String> {
    let service = sanitize_string(payload.service.as_deref(), "");
    ensure_service(&service)?;

    let id = sanitize_string(payload.id.as_deref(), "");
    validate_account_id(&id)?;
    enforce_fetch_usage_rate_limit(&service, &id)?;

    let fallback_name = format!("{} {}", service.to_uppercase(), id);
    let name = sanitize_string(payload.name.as_deref(), &fallback_name);
    validate_account_name(&name)?;

    let mut store = read_store(&app)?;
    let list = match service.as_str() {
        "claude" => &mut store.services.claude,
        "codex" => &mut store.services.codex,
        _ => return Err("Unsupported service".to_string()),
    };

    if let Some(existing) = list.iter_mut().find(|x| x.id == id) {
        if existing.name != name {
            existing.name = name.clone();
        }
    } else {
        list.push(AccountEntry {
            id: id.clone(),
            name: name.clone(),
        });
    }

    if let Some(mut token_input) = payload.token.take() {
        let mut trimmed = token_input.trim().to_string();
        if !trimmed.is_empty() {
            validate_token(&trimmed)?;
            set_token(&service, &id, &trimmed)?;
        }
        trimmed.zeroize();
        token_input.zeroize();
    }

    write_store(&app, &store)?;

    let mut token = get_token(&service, &id)
        .ok_or_else(|| "Token is not set for this account".to_string())?;
    let fetch_result = fetch_normalized_usage(&service, &token).await;
    token.zeroize();
    fetch_result
}

#[tauri::command]
fn get_window_state(app: AppHandle) -> Result<WindowState, String> {
    Ok(read_store(&app)?.settings.window_state)
}

#[tauri::command]
fn set_window_mode(
    app: AppHandle,
    window: WebviewWindow,
    payload: SetWindowModePayload,
) -> Result<WindowState, String> {
    let mut store = read_store(&app)?;
    let ws = &mut store.settings.window_state;

    let requested_mode = if payload.mode.as_deref() == Some("minimal") {
        "minimal"
    } else {
        "normal"
    };

    if requested_mode == "minimal" {
        if let Some(min_width) = payload.min_width {
            if min_width >= MINIMAL_FLOOR_W {
                ws.minimal_min_width = min_width;
            }
        }
        if let Some(min_height) = payload.min_height {
            if min_height >= MINIMAL_FLOOR_H {
                ws.minimal_min_height = min_height;
            }
        }

        if let (Some(preferred_width), Some(preferred_height)) =
            (payload.preferred_width, payload.preferred_height)
        {
            let current = current_window_bounds(&window).ok();
            let proposed = Bounds {
                width: preferred_width,
                height: preferred_height,
                x: current.as_ref().and_then(|b| b.x),
                y: current.as_ref().and_then(|b| b.y),
            };
            ws.minimal_bounds = Some(sanitize_bounds_live(
                Some(&proposed),
                ws.minimal_min_width,
                ws.minimal_min_height,
                &default_minimal_bounds(),
            ));
        }
    }

    if let Ok(current_bounds) = current_window_bounds(&window) {
        if ws.mode == "minimal" {
            ws.minimal_bounds = Some(sanitize_bounds_live(
                Some(&current_bounds),
                ws.minimal_min_width,
                ws.minimal_min_height,
                &default_minimal_bounds(),
            ));
        } else {
            ws.normal_bounds = sanitize_bounds_live(
                Some(&current_bounds),
                NORMAL_WINDOW_MIN_W,
                NORMAL_WINDOW_MIN_H,
                &default_normal_bounds(),
            );
        }
    }

    ws.mode = requested_mode.to_string();
    apply_window_mode(&window, ws)?;
    let out = ws.clone();
    write_store(&app, &store)?;

    Ok(out)
}

#[tauri::command]
fn set_window_position(
    app: AppHandle,
    window: WebviewWindow,
    payload: SetWindowPositionPayload,
) -> Result<ApiOk, String> {
    let x = payload
        .x
        .ok_or_else(|| "x and y are required".to_string())?;
    let y = payload
        .y
        .ok_or_else(|| "x and y are required".to_string())?;

    let has_valid_size = match (payload.width, payload.height) {
        (Some(w), Some(h)) if w >= MINIMAL_FLOOR_W && h >= MINIMAL_FLOOR_H => Some((w, h)),
        _ => None,
    };

    if let Some((w, h)) = has_valid_size {
        set_window_size(&window, w, h)?;
        set_window_position_inner(&window, x, y)?;
    } else {
        set_window_position_inner(&window, x, y)?;
    }

    let mut store = read_store(&app)?;
    let ws = &mut store.settings.window_state;

    if ws.mode == "minimal" {
        let mut next = ws
            .minimal_bounds
            .clone()
            .unwrap_or_else(default_minimal_bounds);
        next.x = Some(x);
        next.y = Some(y);
        if let Some((w, h)) = has_valid_size {
            next.width = w;
            next.height = h;
        }
        ws.minimal_bounds = Some(sanitize_bounds_live(
            Some(&next),
            ws.minimal_min_width,
            ws.minimal_min_height,
            &default_minimal_bounds(),
        ));
    } else {
        let mut next = ws.normal_bounds.clone();
        next.x = Some(x);
        next.y = Some(y);
        if let Some((w, h)) = has_valid_size {
            next.width = w;
            next.height = h;
        }
        ws.normal_bounds = sanitize_bounds_live(
            Some(&next),
            NORMAL_WINDOW_MIN_W,
            NORMAL_WINDOW_MIN_H,
            &default_normal_bounds(),
        );
    }

    write_store(&app, &store)?;
    Ok(ApiOk { ok: true })
}

#[tauri::command]
fn get_version(app: AppHandle) -> Result<String, String> {
    Ok(app.package_info().version.to_string())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let store = read_store(&handle)?;
            write_store(&handle, &store)?;

            if let Some(webview_window) = app.get_webview_window("main") {
                apply_window_mode(&webview_window, &store.settings.window_state)?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_accounts,
            save_account,
            delete_account,
            get_settings,
            set_settings,
            get_polling_state,
            set_polling_state,
            fetch_usage,
            get_window_state,
            set_window_mode,
            set_window_position,
            get_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validate_token_rejects_control_chars() {
        assert!(validate_token("abc\ndef").is_err());
    }

    #[test]
    fn validate_token_accepts_common_jwt_format() {
        assert!(validate_token("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.abc_xyz-123/456").is_ok());
    }

    #[test]
    fn validate_account_id_rejects_unsupported_characters() {
        assert!(validate_account_id("abc:def").is_err());
    }

    #[test]
    fn validate_upstream_url_requires_https_and_allowlisted_host() {
        assert!(validate_upstream_url("https://api.anthropic.com/api/oauth/usage").is_ok());
        assert!(validate_upstream_url("https://chatgpt.com/backend-api/wham/usage").is_ok());
        assert!(validate_upstream_url("http://api.anthropic.com/api/oauth/usage").is_err());
        assert!(validate_upstream_url("https://example.com").is_err());
    }

    #[test]
    fn build_error_returns_sanitized_message() {
        assert_eq!(
            build_error(401, "application/json"),
            "Authentication failed (HTTP 401)"
        );
    }

    #[test]
    fn rate_limit_blocks_immediate_repeat() {
        let unique_id = format!(
            "t{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos()
        );
        assert!(enforce_fetch_usage_rate_limit("claude", &unique_id).is_ok());
        assert!(enforce_fetch_usage_rate_limit("claude", &unique_id).is_err());
    }
}
