//! Recovery stays reachable even if a hosted renderer fails to load.
use super::{HostState, windows};
use hermes_desktop_rust::policy::zoom_step;
use tauri::{
    Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
};

pub fn install(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle();
    let connect = MenuItem::with_id(
        handle,
        "connect",
        "Connect to workspace…",
        true,
        Some("CmdOrCtrl+Shift+O"),
    )?;
    let reload = MenuItem::with_id(
        handle,
        "reload",
        "Reload workspace",
        true,
        Some("CmdOrCtrl+R"),
    )?;
    let zoom_in = MenuItem::with_id(handle, "zoom-in", "Zoom in", true, Some("CmdOrCtrl+Plus"))?;
    let zoom_out = MenuItem::with_id(handle, "zoom-out", "Zoom out", true, Some("CmdOrCtrl+-"))?;
    let zoom_reset = MenuItem::with_id(
        handle,
        "zoom-reset",
        "Actual size",
        true,
        Some("CmdOrCtrl+0"),
    )?;
    let downloads = MenuItem::with_id(
        handle,
        "downloads",
        "Choose download folder…",
        true,
        Some("CmdOrCtrl+Shift+D"),
    )?;
    let disable_downloads = MenuItem::with_id(
        handle,
        "disable-downloads",
        "Stop allowing new downloads",
        true,
        None::<&str>,
    )?;
    // Explicit handlers keep close/quit consistent across platform menu implementations.
    let close = MenuItem::with_id(handle, "close", "Close window", true, Some("CmdOrCtrl+W"))?;
    let quit = MenuItem::with_id(
        handle,
        "quit",
        "Quit Hermes Desktop Rust",
        true,
        Some("CmdOrCtrl+Q"),
    )?;

    let workspace = Submenu::with_items(
        handle,
        "Workspace",
        true,
        &[
            &connect,
            &reload,
            &downloads,
            &disable_downloads,
            &PredefinedMenuItem::separator(handle)?,
            &close,
            &quit,
        ],
    )?;
    #[cfg(target_os = "linux")]
    workspace.insert_items(
        &[
            &MenuItem::with_id(
                handle,
                "notifications",
                "Allow notifications in this window…",
                true,
                Some("CmdOrCtrl+Alt+N"),
            )?,
            &MenuItem::with_id(
                handle,
                "disable-notifications",
                "Stop new notifications",
                true,
                Some("CmdOrCtrl+Alt+M"),
            )?,
        ],
        4,
    )?;
    let edit = Submenu::with_items(
        handle,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(handle, None)?,
            &PredefinedMenuItem::redo(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::cut(handle, None)?,
            &PredefinedMenuItem::copy(handle, None)?,
            &PredefinedMenuItem::paste(handle, None)?,
            &PredefinedMenuItem::select_all(handle, None)?,
        ],
    )?;
    let view = Submenu::with_items(handle, "View", true, &[&zoom_in, &zoom_out, &zoom_reset])?;
    app.set_menu(Menu::with_items(handle, &[&workspace, &edit, &view])?)?;
    app.on_menu_event(|app, event| {
        let focused = app
            .webview_windows()
            .into_values()
            .find(|window| window.is_focused().unwrap_or(false));
        match event.id().as_ref() {
            "close" => {
                if let Some(window) = focused {
                    let _ = window.close();
                }
            }

            "quit" => app.exit(0),
            "connect" => {
                if let Some(window) = app.get_webview_window("launcher") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    // Window focus alone does not focus the embedded webview for keyboard input.
                    let _ = window.as_ref().set_focus();
                }
            }
            "downloads" => windows::choose_download_folder(app),
            #[cfg(target_os = "linux")]
            "notifications" => {
                if let Some(window) = focused {
                    super::notifications::choose(&window);
                }
            }
            #[cfg(target_os = "linux")]
            "disable-notifications" => {
                if let Ok(mut consent) = app.state::<HostState>().notifications.lock() {
                    consent.revoke();
                }
            }
            "disable-downloads" => {
                if let Ok(mut consent) = app.state::<HostState>().downloads.lock() {
                    consent.revoke();
                }
            }
            "reload" => {
                if let Some(window) = focused {
                    let _ = window.reload();
                }
            }
            "zoom-in" | "zoom-out" | "zoom-reset" => {
                if let Some(window) = focused {
                    let direction = match event.id().as_ref() {
                        "zoom-in" => 1,
                        "zoom-out" => -1,
                        _ => 0,
                    };
                    // Incremental, bounded and per window: the level is remembered between steps
                    // and dropped when the window is destroyed.
                    let state = app.state::<HostState>();
                    let label = window.label().to_string();
                    if let Ok(mut levels) = state.zoom.lock() {
                        let current = *levels.get(&label).unwrap_or(&1.0);
                        let next = zoom_step(current, direction);
                        levels.insert(label, next);
                        drop(levels);
                        let _ = window.set_zoom(next);
                    }
                }
            }
            _ => (),
        }
    });
    Ok(())
}
