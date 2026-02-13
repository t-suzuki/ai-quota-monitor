use crate::error::{AppError, AppResult};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn send_notification(
    app: AppHandle,
    payload: crate::SendNotificationPayload,
) -> AppResult<crate::ApiOk> {
    let title = crate::sanitize_string(payload.title.as_deref(), "");
    let body = crate::sanitize_string(payload.body.as_deref(), "");

    if title.is_empty() {
        return Err(AppError::InvalidInput("title is required".to_string()));
    }

    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| AppError::Message(format!("Failed to send notification: {e}")))?;

    Ok(crate::ApiOk { ok: true })
}
