use std::path::PathBuf;

use crate::error::{AppError, AppResult, IpcResult};
use crate::state::SettingsState;

/// Opens a file location in the system file explorer.
#[tauri::command]
pub async fn reveal_in_explorer(path: String) -> IpcResult<()> {
    reveal_in_explorer_inner(&path).into()
}

pub(crate) fn reveal_in_explorer_inner(path: &str) -> AppResult<()> {
    let path = PathBuf::from(path);

    // Get the parent directory if it's a file
    let dir = if path.is_file() {
        path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
    } else {
        path
    };

    #[cfg(target_os = "windows")]
    {
        let dir_str = dir.to_string_lossy().replace('/', "\\");
        std::process::Command::new("explorer")
            .arg(dir_str)
            .spawn()
            .map_err(|e| AppError::Other(format!("Failed to open explorer: {}", e)))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(dir)
            .spawn()
            .map_err(|e| AppError::Other(format!("Failed to open Finder: {}", e)))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| AppError::Other(format!("Failed to open file manager: {}", e)))?;
    }

    Ok(())
}

/// Brings the main window forward, for something the reader is meant to see.
///
/// Three calls because there are two ways to be away and neither implies the
/// other: [`minimize_to_tray`] hides the window where the setting says so, and
/// minimizes it where it does not.
///
/// Each result is logged rather than dropped. Windows refuses a foreground
/// change to a process that does not already own the foreground, so this
/// failing is a thing that happens rather than a thing that cannot, and a
/// window that did not come forward should say which call declined.
pub(crate) fn raise_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;

    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("No main window to raise");
        return;
    };
    if let Err(e) = window.show() {
        tracing::debug!("Could not show the window: {e}");
    }
    if let Err(e) = window.unminimize() {
        tracing::debug!("Could not unminimize the window: {e}");
    }
    if let Err(e) = window.set_focus() {
        tracing::debug!("Could not focus the window: {e}");
    }
}

/// Minimizes the window to the system tray if the setting is enabled,
/// otherwise performs a regular minimize.
#[tauri::command]
pub fn minimize_to_tray(
    window: tauri::WebviewWindow,
    state: tauri::State<SettingsState>,
) -> IpcResult<()> {
    minimize_to_tray_inner(window, &state).into()
}

fn minimize_to_tray_inner(
    window: tauri::WebviewWindow,
    state: &tauri::State<SettingsState>,
) -> AppResult<()> {
    let settings = state.0.lock();

    if settings.minimize_to_tray {
        window
            .hide()
            .map_err(|e| AppError::Other(format!("Failed to hide window: {}", e)))?;
    } else {
        window
            .minimize()
            .map_err(|e| AppError::Other(format!("Failed to minimize window: {}", e)))?;
    }

    Ok(())
}
