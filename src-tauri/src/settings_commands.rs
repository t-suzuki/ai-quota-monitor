use crate::store_repo::{read_store, write_store};
use crate::error::AppResult;
use crate::validation::{validate_discord_webhook_url, validate_export_path, validate_pushover_key};
use tauri::AppHandle;

pub fn get_settings(app: AppHandle) -> AppResult<crate::Settings> {
    Ok(read_store(&app)?.settings)
}

pub fn set_settings(app: AppHandle, payload: crate::SetSettingsPayload) -> AppResult<crate::Settings> {
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

    if let Some(es) = payload.usage_export {
        let current = &mut store.settings.usage_export;
        if let Some(v) = es.enabled {
            current.enabled = v;
        }
        if let Some(path) = es.path {
            let trimmed = path.trim().to_string();
            if trimmed.is_empty() {
                current.path = None;
            } else {
                validate_export_path(&trimmed)?;
                current.path = Some(trimmed);
            }
        }
        if current.enabled && current.path.is_none() {
            current.enabled = false;
        }
    }

    if let Some(en) = payload.external_notify {
        if let Some(discord) = en.discord {
            let current = &mut store.settings.external_notify.discord;
            if let Some(v) = discord.enabled {
                current.enabled = v;
            }
            if let Some(url) = discord.webhook_url {
                let trimmed = url.trim().to_string();
                if trimmed.is_empty() {
                    current.webhook_url = String::new();
                } else {
                    validate_discord_webhook_url(&trimmed)?;
                    current.webhook_url = trimmed;
                }
            }
            if current.enabled && current.webhook_url.is_empty() {
                current.enabled = false;
            }
        }
        if let Some(pushover) = en.pushover {
            let current = &mut store.settings.external_notify.pushover;
            if let Some(v) = pushover.enabled {
                current.enabled = v;
            }
            if let Some(token) = pushover.api_token {
                let trimmed = token.trim().to_string();
                if trimmed.is_empty() {
                    current.api_token = String::new();
                } else {
                    validate_pushover_key(&trimmed, "Pushover API Token")?;
                    current.api_token = trimmed;
                }
            }
            if let Some(key) = pushover.user_key {
                let trimmed = key.trim().to_string();
                if trimmed.is_empty() {
                    current.user_key = String::new();
                } else {
                    validate_pushover_key(&trimmed, "Pushover User Key")?;
                    current.user_key = trimmed;
                }
            }
            if current.enabled
                && (current.api_token.is_empty() || current.user_key.is_empty())
            {
                current.enabled = false;
            }
        }
    }

    write_store(&app, &store)?;
    Ok(store.settings)
}

pub fn get_polling_state(app: AppHandle) -> AppResult<crate::PollingState> {
    Ok(read_store(&app)?.settings.polling_state)
}

pub fn set_polling_state(
    app: AppHandle,
    payload: crate::SetPollingStatePayload,
) -> AppResult<crate::PollingState> {
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
