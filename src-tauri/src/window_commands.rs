use crate::store_repo::{
    default_minimal_bounds, default_normal_bounds, read_store, sanitize_bounds_live, write_store,
};
use crate::error::{AppError, AppResult};
use crate::window_ops::{
    apply_window_mode, current_window_bounds, set_window_position_inner, set_window_size,
};
use tauri::{AppHandle, WebviewWindow};

pub fn get_window_state(app: AppHandle) -> AppResult<crate::WindowState> {
    Ok(read_store(&app)?.settings.window_state)
}

pub fn set_window_mode(
    app: AppHandle,
    window: WebviewWindow,
    payload: crate::SetWindowModePayload,
) -> AppResult<crate::WindowState> {
    let mut store = read_store(&app)?;
    let ws = &mut store.settings.window_state;

    let requested_mode = if payload.mode.as_deref() == Some("minimal") {
        "minimal"
    } else {
        "normal"
    };

    if requested_mode == "minimal" {
        if let Some(min_width) = payload.min_width {
            if min_width >= crate::MINIMAL_FLOOR_W {
                ws.minimal_min_width = min_width;
            }
        }
        if let Some(min_height) = payload.min_height {
            if min_height >= crate::MINIMAL_FLOOR_H {
                ws.minimal_min_height = min_height;
            }
        }

        if let (Some(preferred_width), Some(preferred_height)) =
            (payload.preferred_width, payload.preferred_height)
        {
            let proposed = crate::Bounds {
                width: preferred_width,
                height: preferred_height,
                x: None,
                y: None,
            };
            ws.minimal_bounds = Some(sanitize_bounds_live(
                Some(&proposed),
                ws.minimal_min_width,
                ws.minimal_min_height,
                &default_minimal_bounds(),
            ));
        }
    }

    if let Ok(mut current_bounds) = current_window_bounds(&window) {
        current_bounds.x = None;
        current_bounds.y = None;
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
                crate::NORMAL_WINDOW_MIN_W,
                crate::NORMAL_WINDOW_MIN_H,
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

pub fn set_window_position(
    _app: AppHandle,
    window: WebviewWindow,
    payload: crate::SetWindowPositionPayload,
) -> AppResult<crate::ApiOk> {
    let x = payload
        .x
        .ok_or_else(|| AppError::InvalidInput("x and y are required".to_string()))?;
    let y = payload
        .y
        .ok_or_else(|| AppError::InvalidInput("x and y are required".to_string()))?;

    let has_valid_size = match (payload.width, payload.height) {
        (Some(w), Some(h)) if w >= crate::MINIMAL_FLOOR_W && h >= crate::MINIMAL_FLOOR_H => Some((w, h)),
        _ => None,
    };

    if let Some((w, h)) = has_valid_size {
        set_window_size(&window, w, h)?;
    }
    set_window_position_inner(&window, x, y)?;

    Ok(crate::ApiOk { ok: true })
}

pub fn start_window_drag(_app: AppHandle, window: WebviewWindow) -> AppResult<crate::ApiOk> {
    window
        .start_dragging()
        .map_err(|e| AppError::Window(format!("Failed to start window drag: {e}")))?;

    Ok(crate::ApiOk { ok: true })
}
