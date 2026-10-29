//! Hosted pages are unprivileged; links never drive the OS without confirmation.
use super::HostState;
use hermes_desktop_rust::policy::{
    confirmable_link, navigation_allowed, next_download_destination, popup_origin_allowed,
    sanitized_download_name, window_title,
};
use std::sync::atomic::Ordering;

use tauri::{
    Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    webview::{DownloadEvent, NewWindowFeatures, NewWindowResponse},
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_opener::OpenerExt;
use url::Url;

const MAX_WORKSPACE_WINDOWS: usize = 8;

pub fn open_workspace(
    app: &tauri::AppHandle,
    target: Url,
    features: Option<NewWindowFeatures>,
) -> Result<WebviewWindow, &'static str> {
    if app.webview_windows().len() > MAX_WORKSPACE_WINDOWS {
        return Err("Close an unused workspace window before opening another");
    }
    let state = app.state::<HostState>();
    let label = format!(
        "workspace-{}",
        state.counter.fetch_add(1, Ordering::Relaxed)
    );
    let navigation_origin = target.clone();
    let popup_label = label.clone();
    let cleanup_label = label.clone();
    let popup_app = app.clone();
    let page_app = app.clone();
    let download_app = app.clone();
    let zoom_app = app.clone();
    // A popup window starts at `about:blank`; the engine then navigates the webview it hands back
    // to the requested URL. This follows Tauri's documented handoff; a duplicate fetch was not
    // reproduced on WebKitGTK. A top-level window has no handoff and loads the target directly.
    let initial = match features.is_some() {
        true => WebviewUrl::External(Url::parse("about:blank").expect("about:blank is a URL")),
        false => WebviewUrl::External(target.clone()),
    };
    let mut builder = WebviewWindowBuilder::new(app, label, initial)
        .title(window_title(&target))
        .inner_size(1280.0, 850.0)
        .min_inner_size(680.0, 480.0)
        .disable_drag_drop_handler()
        .on_navigation(move |url| navigation_allowed(&navigation_origin, url))
        .on_page_load(move |window, payload| {
            let _ = window.set_title(&window_title(payload.url()));
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
                && let Some(launcher) = page_app.get_webview_window("launcher")
            {
                let _ = launcher.hide();
            }
        })
        .on_new_window(move |url, features| {
            // Compare against the document the user is on now, not the address this window first
            // opened at, so a redirect keeps its own same-origin popups.
            let document = popup_app
                .get_webview_window(&popup_label)
                .and_then(|window| window.url().ok());
            if document
                .as_ref()
                .is_some_and(|document| popup_origin_allowed(document, &url))
            {
                return match open_workspace(&popup_app, url, Some(features)) {
                    Ok(window) => NewWindowResponse::Create { window },
                    Err(_) => NewWindowResponse::Deny,
                };
            }
            confirm_external_link(&popup_app, url);
            NewWindowResponse::Deny
        })
        // Require a user-selected download directory; retain the engine's authenticated/blob
        // transfer rather than re-fetching in Rust or losing its credentials in another browser.
        .on_download(move |_webview, event| handle_download(&download_app, event));
    if let Some(features) = features {
        builder = builder.window_features(features);
    }
    let window = builder
        .build()
        .map_err(|_| "The system webview could not create a window")?;
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed)
            && let Some(state) = zoom_app.try_state::<HostState>()
            && let Ok(mut zoom) = state.zoom.lock()
        {
            zoom.remove(&cleanup_label);
        }
    });
    Ok(window)
}

/// Downloads need per-session directory consent. Every webview installs this, the launcher included:
/// wry registers a
/// download handler on the app's shared web context once per webview, and its built-in default is
/// permissive, so a webview left without one would allow downloads for the whole app.
pub(crate) fn handle_download(app: &tauri::AppHandle, event: DownloadEvent<'_>) -> bool {
    match event {
        DownloadEvent::Requested { url, destination } => {
            if !matches!(url.scheme(), "https" | "http" | "blob" | "data")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return false;
            }
            let suggested = destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("download")
                .to_owned();
            let directory = app
                .state::<HostState>()
                .downloads
                .lock()
                .ok()
                .and_then(|consent| consent.directory().map(std::path::Path::to_path_buf));
            if let Some(directory) = directory
                && let Ok(path) = next_download_destination(&directory, &suggested)
            {
                *destination = path;
                return true;
            }
            request_download_consent(app, &suggested);
            false
        }
        _ => true,
    }
}

fn request_download_consent(app: &tauri::AppHandle, suggested: &str) {
    let Some(prompt) = begin_download_prompt(app) else {
        return;
    };
    let name = sanitized_download_name(suggested);
    let handle = app.clone();
    app.dialog()
        .message(format!(
            "A workspace wants to save “{name}”. No file was saved.\n\nChoose a folder to allow workspace downloads for this app session, then retry the download. Files stay in that folder; stopping new downloads is available in the Workspace menu.",
        ))
        .title("Allow workspace downloads?")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Choose folder…".into(),
            "Cancel".into(),
        ))
        .show(move |accepted| {
            if accepted {
                pick_download_folder(&handle, prompt);
            } else if let Ok(mut consent) = handle.state::<HostState>().downloads.lock() {
                consent.finish_prompt(prompt, None);
            }
        });
}

pub(crate) fn choose_download_folder(app: &tauri::AppHandle) {
    if let Some(prompt) = begin_download_prompt(app) {
        pick_download_folder(app, prompt);
    }
}

fn begin_download_prompt(app: &tauri::AppHandle) -> Option<u64> {
    app.state::<HostState>()
        .downloads
        .lock()
        .ok()?
        .begin_prompt()
}

fn pick_download_folder(app: &tauri::AppHandle, prompt: u64) {
    let handle = app.clone();
    let previous = {
        let state = app.state::<HostState>();
        let Ok(consent) = state.downloads.lock() else {
            return;
        };
        if !consent.is_current(prompt) {
            return;
        }
        consent.directory().map(std::path::Path::to_path_buf)
    };
    let initial = previous
        .or_else(|| app.path().download_dir().ok().filter(|path| path.is_dir()))
        .or_else(|| app.path().home_dir().ok());
    let mut picker = app
        .dialog()
        .file()
        .set_title("Allow workspace downloads here for this app session");
    if let Some(path) = initial {
        picker = picker.set_directory(path);
    }
    if let Some(parent) = app
        .webview_windows()
        .into_values()
        .find(|window| window.is_focused().unwrap_or(false))
    {
        picker = picker.set_parent(&parent);
    }
    // Nonblocking: a GTK download callback cannot wait on a main-thread dialog. No replay of a
    // possibly consumed ticket or revoked blob URL: the user retries the original export action.
    picker.pick_folder(move |selected| {
        let directory = selected
            .and_then(|file| file.into_path().ok())
            .and_then(|path| path.canonicalize().ok())
            .filter(|path| path.is_dir());
        if let Ok(mut consent) = handle.state::<HostState>().downloads.lock() {
            consent.finish_prompt(prompt, directory);
        }
    });
}

fn confirm_external_link(app: &tauri::AppHandle, url: Url) {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return;
    }
    let handle = app.clone();
    app.dialog()
        .message(format!(
            "Open {} in your default browser?\n\nThis website will leave Hermes Desktop Rust.",
            confirmable_link(&url)
        ))
        .title("Open external website")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Open browser".into(),
            "Cancel".into(),
        ))
        .show(move |accepted| {
            if accepted
                && handle
                    .opener()
                    .open_url(url.as_str(), None::<&str>)
                    .is_err()
            {
                handle
                    .dialog()
                    .message(
                        "The browser could not be opened. Use the website address directly in your browser.",
                    )
                    .title("Could not open browser")
                    .show(|_| {});
            }
        });
}
