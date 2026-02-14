use crate::error::{AppError, AppResult};
use crate::store_repo::read_store;
use crate::validation::validate_export_path;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri::Manager;

fn resolve_export_path(app: &AppHandle, configured: &str) -> AppResult<PathBuf> {
    validate_export_path(configured)?;
    let p = PathBuf::from(configured);
    if p.is_absolute() {
        return Ok(p);
    }

    // Relative paths are saved under app data dir for consistency across platforms.
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Store(format!("Failed to resolve app data directory: {e}")))?;
    Ok(base.join(p))
}

fn ensure_parent_dir(path: &Path) -> AppResult<()> {
    let Some(parent) = path.parent() else {
        return Err(AppError::InvalidInput("Export path must include a parent directory".to_string()));
    };
    fs::create_dir_all(parent).map_err(|e| {
        AppError::Store(format!(
            "Failed to create export directory '{}': {e}",
            parent.display()
        ))
    })?;
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    ensure_parent_dir(path)?;

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, bytes).map_err(|e| {
        AppError::Store(format!(
            "Failed to write export temp file '{}': {e}",
            tmp_path.display()
        ))
    })?;

    // Best-effort atomic swap (Windows rename fails if destination exists).
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp_path, path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        AppError::Store(format!(
            "Failed to move export file into place '{}': {e}",
            path.display()
        ))
    })?;

    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageSnapshotFile {
    schema_version: i32,
    app_name: String,
    app_version: String,
    generated_at: String,
    fetched_at: Option<String>,
    entries: Vec<crate::UsageSnapshotEntry>,
}

pub fn write_usage_snapshot(app: AppHandle, payload: crate::WriteUsageSnapshotPayload) -> AppResult<crate::ApiOk> {
    let store = read_store(&app)?;
    let settings = &store.settings.usage_export;
    if !settings.enabled {
        return Ok(crate::ApiOk { ok: true });
    }

    let Some(configured_path) = settings.path.as_deref() else {
        // Enabled but no path: treat as a no-op.
        return Ok(crate::ApiOk { ok: true });
    };

    let out_path = resolve_export_path(&app, configured_path)?;
    let now = chrono::Utc::now().to_rfc3339();
    let version = app.package_info().version.to_string();

    let entries = payload.entries.unwrap_or_default();
    let file = UsageSnapshotFile {
        schema_version: 1,
        app_name: crate::APP_NAME.to_string(),
        app_version: version,
        generated_at: now,
        fetched_at: payload.fetched_at,
        entries,
    };

    let json = serde_json::to_vec_pretty(&file)
        .map_err(|e| AppError::Message(format!("Failed to serialize snapshot JSON: {e}")))?;
    atomic_write(&out_path, &json)?;
    Ok(crate::ApiOk { ok: true })
}
