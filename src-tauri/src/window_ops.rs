use tauri::{LogicalPosition, LogicalSize, Position, Size, WebviewWindow};
use crate::error::{AppError, AppResult};

pub fn current_window_bounds(window: &WebviewWindow) -> AppResult<crate::Bounds> {
    let scale_factor = window
        .scale_factor()
        .map_err(|e| AppError::Window(format!("Failed to read window scale factor: {e}")))?;

    let size = window
        .inner_size()
        .map_err(|e| AppError::Window(format!("Failed to read window size: {e}")))?;
    let logical_size = size.to_logical::<f64>(scale_factor);
    let mut bounds = crate::Bounds {
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

pub fn set_window_size(window: &WebviewWindow, width: i32, height: i32) -> AppResult<()> {
    window
        .set_size(Size::Logical(LogicalSize::new(width as f64, height as f64)))
        .map_err(|e| AppError::Window(format!("Failed to set window size: {e}")))
}

pub fn set_window_position_inner(window: &WebviewWindow, x: i32, y: i32) -> AppResult<()> {
    window
        .set_position(Position::Logical(LogicalPosition::new(x as f64, y as f64)))
        .map_err(|e| AppError::Window(format!("Failed to set window position: {e}")))
}

pub fn apply_window_mode(window: &WebviewWindow, ws: &crate::WindowState) -> AppResult<()> {
    let is_minimal = ws.mode == "minimal";
    let min_width = if is_minimal {
        ws.minimal_min_width
    } else {
        crate::NORMAL_WINDOW_MIN_W
    };
    let min_height = if is_minimal {
        ws.minimal_min_height
    } else {
        crate::NORMAL_WINDOW_MIN_H
    };

    let fallback = if is_minimal {
        crate::store_repo::default_minimal_bounds()
    } else {
        crate::store_repo::default_normal_bounds()
    };

    let source = if is_minimal {
        ws.minimal_bounds.as_ref().unwrap_or(&fallback)
    } else {
        &ws.normal_bounds
    };

    let bounds = crate::store_repo::sanitize_bounds_live(Some(source), min_width, min_height, &fallback);

    window
        .set_min_size(Some(Size::Logical(LogicalSize::new(
            min_width as f64,
            min_height as f64,
        ))))
        .map_err(|e| AppError::Window(format!("Failed to set minimum window size: {e}")))?;

    window
        .set_decorations(!is_minimal)
        .map_err(|e| AppError::Window(format!("Failed to update window decorations: {e}")))?;
    window
        .set_always_on_top(is_minimal)
        .map_err(|e| AppError::Window(format!("Failed to update always-on-top: {e}")))?;

    set_window_size(window, bounds.width, bounds.height)?;

    if let (Some(x), Some(y)) = (bounds.x, bounds.y) {
        set_window_position_inner(window, x, y)?;
    }

    Ok(())
}
