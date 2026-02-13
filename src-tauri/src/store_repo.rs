use crate::validation::{validate_account_id, validate_account_name};
use crate::error::{AppError, AppResult};
use tauri::{AppHandle, Manager};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static STORE_CACHE: OnceLock<Mutex<Option<crate::Store>>> = OnceLock::new();

fn clamp_int(value: Option<i32>, fallback: i32, min: i32, max: i32) -> i32 {
    let n = value.unwrap_or(fallback);
    n.clamp(min, max)
}

pub fn default_normal_bounds() -> crate::Bounds {
    crate::Bounds {
        width: crate::NORMAL_WINDOW_DEFAULT_W,
        height: crate::NORMAL_WINDOW_DEFAULT_H,
        x: None,
        y: None,
    }
}

pub fn default_minimal_bounds() -> crate::Bounds {
    crate::Bounds {
        width: crate::MINIMAL_WINDOW_DEFAULT_W,
        height: crate::MINIMAL_WINDOW_DEFAULT_H,
        x: None,
        y: None,
    }
}

fn default_store() -> crate::Store {
    crate::Store {
        services: crate::Services {
            claude: Vec::new(),
            codex: Vec::new(),
        },
        settings: crate::Settings {
            poll_interval: 120,
            polling_state: crate::PollingState {
                active: false,
                started_at: None,
                interval: 120,
            },
            window_state: crate::WindowState {
                mode: "normal".to_string(),
                normal_bounds: default_normal_bounds(),
                minimal_bounds: None,
                minimal_min_width: crate::MINIMAL_MIN_W_DEFAULT,
                minimal_min_height: crate::MINIMAL_WINDOW_MIN_H_DEFAULT,
            },
            notify_settings: crate::NotifySettings {
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
    raw: Option<&crate::BoundsRaw>,
    min_width: i32,
    min_height: i32,
    fallback: &crate::Bounds,
) -> crate::Bounds {
    let width = clamp_int(raw.and_then(|x| x.width), fallback.width, min_width, 8192);
    let height = clamp_int(raw.and_then(|x| x.height), fallback.height, min_height, 8192);

    crate::Bounds {
        width,
        height,
        x: None,
        y: None,
    }
}

pub fn sanitize_bounds_live(
    bounds: Option<&crate::Bounds>,
    min_width: i32,
    min_height: i32,
    fallback: &crate::Bounds,
) -> crate::Bounds {
    let source = bounds.unwrap_or(fallback);
    let width = clamp_int(Some(source.width), fallback.width, min_width, 8192);
    let height = clamp_int(Some(source.height), fallback.height, min_height, 8192);
    crate::Bounds {
        width,
        height,
        x: None,
        y: None,
    }
}

fn normalize_accounts(raw: Option<&Vec<crate::AccountEntryRaw>>, service: &str) -> Vec<crate::AccountEntry> {
    let mut out = Vec::new();
    let Some(entries) = raw else {
        return out;
    };
    for entry in entries {
        let id = crate::sanitize_string(entry.id.as_deref(), "");
        if id.is_empty() {
            continue;
        }
        if validate_account_id(&id).is_err() {
            continue;
        }
        let fallback = format!("{}:{}", service, id);
        let name_candidate = crate::sanitize_string(entry.name.as_deref(), &fallback);
        let name = if validate_account_name(&name_candidate).is_ok() {
            name_candidate
        } else {
            fallback
        };
        out.push(crate::AccountEntry { id, name });
    }
    out
}

fn normalize_store(raw: crate::StoreRaw) -> crate::Store {
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
        crate::MINIMAL_MIN_W_DEFAULT,
        crate::MINIMAL_FLOOR_W,
        4096,
    );
    let minimal_min_height = clamp_int(
        window_raw.and_then(|w| w.minimal_min_height),
        crate::MINIMAL_WINDOW_MIN_H_DEFAULT,
        crate::MINIMAL_FLOOR_H,
        4096,
    );

    let normal_bounds = sanitize_bounds_raw(
        window_raw.and_then(|w| w.normal_bounds.as_ref()),
        crate::NORMAL_WINDOW_MIN_W,
        crate::NORMAL_WINDOW_MIN_H,
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
    let notify_settings = crate::NotifySettings {
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

    crate::Store {
        services: crate::Services { claude, codex },
        settings: crate::Settings {
            poll_interval,
            polling_state: crate::PollingState {
                active: polling_raw.and_then(|p| p.active).unwrap_or(false),
                started_at: polling_started_at,
                interval: polling_interval,
            },
            window_state: crate::WindowState {
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

fn store_path(app: &AppHandle) -> AppResult<PathBuf> {
    let mut dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Store(format!("Failed to resolve app data directory: {e}")))?;
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::Store(format!("Failed to create app data directory: {e}")))?;
    dir.push(crate::STORE_FILE);
    Ok(dir)
}

fn store_cache() -> &'static Mutex<Option<crate::Store>> {
    STORE_CACHE.get_or_init(|| Mutex::new(None))
}

pub fn read_store(app: &AppHandle) -> AppResult<crate::Store> {
    if let Some(cached) = store_cache()
        .lock()
        .map_err(|_| AppError::Store("Store cache lock is poisoned".to_string()))?
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
                .map_err(|_| AppError::Store("Store cache lock is poisoned".to_string()))?;
            if cache.is_none() {
                *cache = Some(fallback.clone());
            }
            return Ok(cache.as_ref().cloned().unwrap_or(fallback));
        }
    };

    let parsed = match serde_json::from_str::<crate::StoreRaw>(&raw) {
        Ok(parsed) => parsed,
        Err(_) => {
            let fallback = default_store();
            let mut cache = store_cache()
                .lock()
                .map_err(|_| AppError::Store("Store cache lock is poisoned".to_string()))?;
            if cache.is_none() {
                *cache = Some(fallback.clone());
            }
            return Ok(cache.as_ref().cloned().unwrap_or(fallback));
        }
    };

    let normalized = normalize_store(parsed);
    let mut cache = store_cache()
        .lock()
        .map_err(|_| AppError::Store("Store cache lock is poisoned".to_string()))?;
    if let Some(existing) = cache.as_ref() {
        return Ok(existing.clone());
    }
    *cache = Some(normalized.clone());
    Ok(normalized)
}

pub fn write_store(app: &AppHandle, store: &crate::Store) -> AppResult<()> {
    let path = store_path(app)?;
    let body = serde_json::to_string_pretty(store)
        .map_err(|e| AppError::Store(format!("Failed to serialize store: {e}")))?;
    fs::write(path, body).map_err(|e| AppError::Store(format!("Failed to write store: {e}")))?;
    let mut cache = store_cache()
        .lock()
        .map_err(|_| AppError::Store("Store cache lock is poisoned".to_string()))?;
    *cache = Some(store.clone());
    Ok(())
}
