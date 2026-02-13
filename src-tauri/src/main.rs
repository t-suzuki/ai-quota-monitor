#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use api_client::FetchUsageResponse;
use account_commands::{
    delete_account as delete_account_impl, list_accounts as list_accounts_impl,
    save_account as save_account_impl,
};
use serde::{Deserialize, Serialize};
use settings_commands::{
    get_polling_state as get_polling_state_impl, get_settings as get_settings_impl,
    set_polling_state as set_polling_state_impl, set_settings as set_settings_impl,
};
use store_repo::{read_store, write_store};
use tauri::{AppHandle, Manager, WebviewWindow};
use usage_commands::fetch_usage as fetch_usage_impl;
use window_commands::{
    get_window_state as get_window_state_impl, set_window_mode as set_window_mode_impl,
    set_window_position as set_window_position_impl,
};
use window_ops::apply_window_mode;
mod api_client;
mod account_commands;
mod settings_commands;
mod store_repo;
mod token_store;
mod usage_commands;
mod usage_parser;
mod validation;
mod window_commands;
mod window_ops;

const APP_NAME: &str = "AI Quota Monitor";
const STORE_FILE: &str = "accounts.json";
const ANTHROPIC_OAUTH_BETA: &str = "oauth-2025-04-20";

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

fn sanitize_string(input: Option<&str>, fallback: &str) -> String {
    let v = input.unwrap_or("").trim();
    if v.is_empty() {
        fallback.to_string()
    } else {
        v.to_string()
    }
}

#[tauri::command]
fn list_accounts(app: AppHandle) -> Result<AccountsSnapshot, String> {
    list_accounts_impl(app)
}

#[tauri::command]
fn save_account(app: AppHandle, payload: SaveAccountPayload) -> Result<AccountSnapshotEntry, String> {
    save_account_impl(app, payload)
}

#[tauri::command]
fn delete_account(app: AppHandle, payload: DeleteAccountPayload) -> Result<ApiOk, String> {
    delete_account_impl(app, payload)
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<Settings, String> {
    get_settings_impl(app)
}

#[tauri::command]
fn set_settings(app: AppHandle, payload: SetSettingsPayload) -> Result<Settings, String> {
    set_settings_impl(app, payload)
}

#[tauri::command]
fn get_polling_state(app: AppHandle) -> Result<PollingState, String> {
    get_polling_state_impl(app)
}

#[tauri::command]
fn set_polling_state(app: AppHandle, payload: SetPollingStatePayload) -> Result<PollingState, String> {
    set_polling_state_impl(app, payload)
}

#[tauri::command]
async fn fetch_usage(app: AppHandle, payload: FetchUsagePayload) -> Result<FetchUsageResponse, String> {
    fetch_usage_impl(app, payload).await
}

#[tauri::command]
fn get_window_state(app: AppHandle) -> Result<WindowState, String> {
    get_window_state_impl(app)
}

#[tauri::command]
fn set_window_mode(
    app: AppHandle,
    window: WebviewWindow,
    payload: SetWindowModePayload,
) -> Result<WindowState, String> {
    set_window_mode_impl(app, window, payload)
}

#[tauri::command]
fn set_window_position(
    app: AppHandle,
    window: WebviewWindow,
    payload: SetWindowPositionPayload,
) -> Result<ApiOk, String> {
    set_window_position_impl(app, window, payload)
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
        assert!(validation::validate_token("abc\ndef").is_err());
    }

    #[test]
    fn validate_token_accepts_common_jwt_format() {
        assert!(validation::validate_token("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.abc_xyz-123/456").is_ok());
    }

    #[test]
    fn validate_account_id_rejects_unsupported_characters() {
        assert!(validation::validate_account_id("abc:def").is_err());
    }

    #[test]
    fn validate_upstream_url_requires_https_and_allowlisted_host() {
        assert!(validation::validate_upstream_url("https://api.anthropic.com/api/oauth/usage").is_ok());
        assert!(validation::validate_upstream_url("https://chatgpt.com/backend-api/wham/usage").is_ok());
        assert!(validation::validate_upstream_url("http://api.anthropic.com/api/oauth/usage").is_err());
        assert!(validation::validate_upstream_url("https://example.com").is_err());
    }

    #[test]
    fn build_error_returns_sanitized_message() {
        assert_eq!(
            api_client::build_error_message(401, "application/json"),
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
        assert!(validation::enforce_fetch_usage_rate_limit("claude", &unique_id).is_ok());
        assert!(validation::enforce_fetch_usage_rate_limit("claude", &unique_id).is_err());
    }
}
