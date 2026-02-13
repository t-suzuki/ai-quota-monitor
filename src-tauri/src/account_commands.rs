use crate::store_repo::{read_store, write_store};
use crate::token_store::{delete_token, ensure_service, get_token, set_token};
use crate::error::{AppError, AppResult};
use crate::validation::{validate_account_id, validate_account_name, validate_token};
use tauri::AppHandle;
use zeroize::Zeroize;

pub fn list_accounts(app: AppHandle) -> AppResult<crate::AccountsSnapshot> {
    let store = read_store(&app)?;

    let map_accounts = |service: &str, accounts: &[crate::AccountEntry]| {
        accounts
            .iter()
            .filter_map(|entry| {
                let id = crate::sanitize_string(Some(&entry.id), "");
                if id.is_empty() {
                    return None;
                }
                if validate_account_id(&id).is_err() {
                    return None;
                }
                let fallback = format!("{service}:{id}");
                let name_candidate = crate::sanitize_string(Some(&entry.name), &fallback);
                let name = if validate_account_name(&name_candidate).is_ok() {
                    name_candidate
                } else {
                    fallback
                };
                let has_token = get_token(service, &id).is_some();
                Some(crate::AccountSnapshotEntry {
                    id,
                    name,
                    has_token,
                })
            })
            .collect::<Vec<_>>()
    };

    Ok(crate::AccountsSnapshot {
        claude: map_accounts("claude", &store.services.claude),
        codex: map_accounts("codex", &store.services.codex),
        settings: store.settings,
    })
}

pub fn save_account(
    app: AppHandle,
    mut payload: crate::SaveAccountPayload,
) -> AppResult<crate::AccountSnapshotEntry> {
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
        existing.name = name.clone();
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

    if payload.clear_token.unwrap_or(false) {
        delete_token(&service, &id)?;
    }

    write_store(&app, &store)?;

    Ok(crate::AccountSnapshotEntry {
        id: id.clone(),
        name,
        has_token: get_token(&service, &id).is_some(),
    })
}

pub fn delete_account(app: AppHandle, payload: crate::DeleteAccountPayload) -> AppResult<crate::ApiOk> {
    let service = crate::sanitize_string(payload.service.as_deref(), "");
    ensure_service(&service)?;

    let id = crate::sanitize_string(payload.id.as_deref(), "");
    validate_account_id(&id)?;

    let mut store = read_store(&app)?;
    let list = match service.as_str() {
        "claude" => &mut store.services.claude,
        "codex" => &mut store.services.codex,
        _ => return Err(AppError::UnsupportedService),
    };

    list.retain(|x| x.id != id);
    write_store(&app, &store)?;
    delete_token(&service, &id)?;

    Ok(crate::ApiOk { ok: true })
}
