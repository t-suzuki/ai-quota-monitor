use crate::account_commands::{
    delete_account as delete_account_impl, list_accounts as list_accounts_impl,
    save_account as save_account_impl,
};
use crate::api_client::FetchUsageResponse;
use crate::notification_commands::send_notification as send_notification_impl;
use crate::settings_commands::{
    get_polling_state as get_polling_state_impl, get_settings as get_settings_impl,
    set_polling_state as set_polling_state_impl, set_settings as set_settings_impl,
};
use crate::usage_commands::fetch_usage as fetch_usage_impl;
use crate::window_commands::{
    get_window_state as get_window_state_impl, set_window_mode as set_window_mode_impl,
    set_window_position as set_window_position_impl,
};
use tauri::{AppHandle, WebviewWindow};

#[tauri::command]
pub fn list_accounts(app: AppHandle) -> Result<crate::AccountsSnapshot, String> {
    list_accounts_impl(app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_account(
    app: AppHandle,
    payload: crate::SaveAccountPayload,
) -> Result<crate::AccountSnapshotEntry, String> {
    save_account_impl(app, payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_account(app: AppHandle, payload: crate::DeleteAccountPayload) -> Result<crate::ApiOk, String> {
    delete_account_impl(app, payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<crate::Settings, String> {
    get_settings_impl(app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_settings(app: AppHandle, payload: crate::SetSettingsPayload) -> Result<crate::Settings, String> {
    set_settings_impl(app, payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_polling_state(app: AppHandle) -> Result<crate::PollingState, String> {
    get_polling_state_impl(app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_polling_state(
    app: AppHandle,
    payload: crate::SetPollingStatePayload,
) -> Result<crate::PollingState, String> {
    set_polling_state_impl(app, payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_usage(
    app: AppHandle,
    payload: crate::FetchUsagePayload,
) -> Result<FetchUsageResponse, String> {
    fetch_usage_impl(app, payload).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_window_state(app: AppHandle) -> Result<crate::WindowState, String> {
    get_window_state_impl(app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_window_mode(
    app: AppHandle,
    window: WebviewWindow,
    payload: crate::SetWindowModePayload,
) -> Result<crate::WindowState, String> {
    set_window_mode_impl(app, window, payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_window_position(
    app: AppHandle,
    window: WebviewWindow,
    payload: crate::SetWindowPositionPayload,
) -> Result<crate::ApiOk, String> {
    set_window_position_impl(app, window, payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_version(app: AppHandle) -> Result<String, String> {
    Ok(app.package_info().version.to_string())
}

#[tauri::command]
pub fn send_notification(
    app: AppHandle,
    payload: crate::SendNotificationPayload,
) -> Result<crate::ApiOk, String> {
    send_notification_impl(app, payload).map_err(|e| e.to_string())
}
