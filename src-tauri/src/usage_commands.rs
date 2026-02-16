use crate::api_client::{fetch_normalized_usage, FetchUsageResponse};
use crate::error::{AppError, AppResult};
use crate::store_repo::{read_store, write_store};
use crate::token_store::{ensure_service, get_token, set_token};
use crate::validation::{
    enforce_fetch_usage_rate_limit, validate_account_id, validate_account_name, validate_token,
};
use tauri::AppHandle;
use zeroize::Zeroize;

pub async fn fetch_usage(
    app: AppHandle,
    mut payload: crate::FetchUsagePayload,
) -> AppResult<FetchUsageResponse> {
    let service = crate::sanitize_string(payload.service.as_deref(), "");
    ensure_service(&service)?;

    let id = crate::sanitize_string(payload.id.as_deref(), "");
    validate_account_id(&id)?;

    let fallback_name = format!("{} {}", service.to_uppercase(), id);
    let name = crate::sanitize_string(payload.name.as_deref(), &fallback_name);
    validate_account_name(&name)?;

    let mut store = read_store(&app)?;
    let list = match service.as_str() {
        "claude" => &mut store.services.claude,
        "codex" => &mut store.services.codex,
        _ => return Err(AppError::UnsupportedService),
    };

    if let Some(existing) = list.iter_mut().find(|x| x.id == id) {
        if existing.name != name {
            existing.name = name.clone();
        }
    } else {
        list.push(crate::AccountEntry {
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
        .ok_or_else(|| AppError::InvalidInput("Token is not set for this account".to_string()))?;
    if let Err(e) = enforce_fetch_usage_rate_limit(&service, &token) {
        token.zeroize();
        return Err(e);
    }
    let fetch_result = fetch_normalized_usage(&service, &token, crate::ANTHROPIC_OAUTH_BETA).await;
    token.zeroize();
    fetch_result.map_err(AppError::from)
}
