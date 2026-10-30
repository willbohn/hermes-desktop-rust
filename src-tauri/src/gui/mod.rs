//! The only privileged renderer is the bundled connection launcher.
#[cfg(target_os = "linux")]
mod media;
mod menu;
#[cfg(target_os = "linux")]
mod notifications;
mod windows;

use clap::Parser;
#[cfg(target_os = "linux")]
use hermes_desktop_rust::notifications::NotificationConsent;
use hermes_desktop_rust::{
    downloads::DownloadConsent,
    launch::{self, LocalOptions},
    policy::{is_launcher, remote_workspace_url},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Mutex as StdMutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tauri::{Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::Mutex;

#[derive(Parser)]
#[command(
    version,
    about = "Native Rust host for an existing Hermes Desktop workspace"
)]
pub struct Options {
    /// Installed Hermes executable. Run directly, never through a shell.
    #[arg(long, default_value = "hermes")]
    hermes: PathBuf,
    /// Explicit isolated or existing Hermes home; never copied or modified by the host.
    #[arg(long)]
    home: Option<PathBuf>,
    #[arg(long)]
    profile: Option<String>,
    /// Prebuilt upstream Webapp directory, passed through the supported runtime override.
    #[arg(long)]
    renderer: Option<PathBuf>,
    /// HTTPS remote workspace (no query parameters, fragments or credentials).
    #[arg(long)]
    url: Option<String>,
    /// Show the launcher without connecting automatically.
    #[arg(long)]
    no_auto_connect: bool,
    /// Synthetic WebKit devices for isolated native tests; never bypasses consent.
    #[cfg(target_os = "linux")]
    #[arg(long, hide = true)]
    test_mock_media: bool,
}

pub struct HostState {
    options: Options,
    auto_connect: AtomicBool,
    busy: Mutex<()>,
    counter: AtomicUsize,
    /// Per-window zoom level, removed when its window is destroyed.
    zoom: StdMutex<HashMap<String, f64>>,
    /// Explicit native-menu consent, scoped to this app session; never granted by a hosted page.
    downloads: StdMutex<DownloadConsent>,
    #[cfg(target_os = "linux")]
    notifications: StdMutex<NotificationConsent>,
}

#[derive(Debug, Serialize)]
pub struct UiError {
    code: &'static str,
    message: String,
    recovery: String,
}

impl UiError {
    fn new(code: &'static str, message: impl Into<String>, recovery: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            recovery: recovery.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase", deny_unknown_fields)]
enum ConnectRequest {
    Local { profile: Option<String> },
    Remote { url: String },
}

fn authorize(window: &WebviewWindow) -> Result<(), UiError> {
    if window
        .url()
        .is_ok_and(|url| is_launcher(window.label(), &url))
    {
        Ok(())
    } else {
        Err(UiError::new(
            "forbidden",
            "Native action refused.",
            "Use the bundled launcher, not a hosted page.",
        ))
    }
}

fn local_options(state: &HostState, profile: Option<String>) -> LocalOptions {
    LocalOptions {
        program: state.options.hermes.clone(),
        profile: profile.or_else(|| state.options.profile.clone()),
        home: state.options.home.clone(),
        renderer: state.options.renderer.clone(),
    }
}

#[derive(Serialize)]
struct LauncherOptions {
    profile: Option<String>,
    url: Option<String>,
    auto_connect: bool,
}

#[tauri::command]
fn launch_options(
    window: WebviewWindow,
    state: State<'_, HostState>,
) -> Result<LauncherOptions, UiError> {
    authorize(&window)?;
    Ok(LauncherOptions {
        profile: state.options.profile.clone(),
        url: state.options.url.clone(),
        auto_connect: state.auto_connect.swap(false, Ordering::Relaxed),
    })
}

#[tauri::command]
async fn launch_workspace(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, HostState>,
    request: ConnectRequest,
) -> Result<(), UiError> {
    authorize(&window)?;
    let _single_flight = state.busy.try_lock().map_err(|_| {
        UiError::new(
            "busy",
            "Another connection is still starting.",
            "Wait for it to finish before trying again.",
        )
    })?;
    let target = match request {
        ConnectRequest::Local { profile } => {
            let target = launch::open_local(&local_options(&state, profile))
                .await
                .map_err(|error| UiError::new("local_launch", error.message, error.recovery))?;
            target.url().clone()
        }
        ConnectRequest::Remote { url } => remote_workspace_url(&url).map_err(|message| {
            UiError::new(
                "invalid_url",
                message,
                "Enter the base HTTPS workspace address. Sign in after it opens.",
            )
        })?,
    };
    windows::open_workspace(&app, target, None, Some(window)).map_err(|_| {
        UiError::new(
            "window_failed",
            "The workspace window could not be created.",
            "Close an unused window or restart this app. The gateway is not stopped.",
        )
    })?;
    Ok(())
}

#[tauri::command]
async fn build_renderer(
    window: WebviewWindow,
    state: State<'_, HostState>,
    request: ConnectRequest,
) -> Result<(), UiError> {
    authorize(&window)?;
    let ConnectRequest::Local { profile } = request else {
        return Err(UiError::new(
            "local_only",
            "Only local workspaces can be built here.",
            "Build remote assets on the gateway machine.",
        ));
    };
    let _single_flight = state.busy.try_lock().map_err(|_| {
        UiError::new(
            "busy",
            "Hermes is already working on a connection or build.",
            "Wait for the current operation to finish.",
        )
    })?;
    launch::build_renderer(&local_options(&state, profile))
        .await
        .map_err(|error| UiError::new("build_failed", error.message, error.recovery))
}

/// A dismissed launcher is retired, not hidden: GTK3 WebKit can retain an unhandled-key replay
/// flag across hide/show. Recreate the same guarded surface without synthesizing keyboard input.
fn show_launcher(app: &tauri::AppHandle) -> Result<(), &'static str> {
    let launcher = match app.get_webview_window("launcher") {
        Some(launcher) => launcher,
        None => {
            // Every webview needs this policy: wry's WebKit download context is shared.
            let download_app = app.clone();
            let launcher =
                WebviewWindowBuilder::new(app, "launcher", WebviewUrl::App("index.html".into()))
                    .title("Hermes Desktop Rust — Connect")
                    .inner_size(650.0, 730.0)
                    .min_inner_size(400.0, 520.0)
                    .center()
                    .on_navigation(|url| is_launcher("launcher", url))
                    .on_download(move |_webview, event| {
                        windows::handle_download(&download_app, event)
                    })
                    .build()
                    .map_err(|_| "The connection window could not be created")?;
            #[cfg(target_os = "linux")]
            if let Err(error) =
                notifications::install_denied(&launcher).and_then(|()| media::install(&launcher))
            {
                let _ = launcher.destroy();
                return Err(error);
            }
            launcher
        }
    };
    launcher
        .show()
        .and_then(|()| launcher.set_focus())
        .map_err(|_| "The connection window could not be shown")
}

pub fn run() -> Result<(), &'static str> {
    let mut options = Options::parse();
    if let Some(url) = &options.url {
        options.url = Some(remote_workspace_url(url).map_err(|_| "invalid workspace address: use HTTPS without credentials, query or fragment")?.to_string());
    }
    tauri::Builder::default()
        .manage(HostState {
            auto_connect: AtomicBool::new(!options.no_auto_connect),
            options,
            busy: Mutex::new(()),
            counter: AtomicUsize::new(1),
            zoom: StdMutex::new(HashMap::new()),
            downloads: StdMutex::new(DownloadConsent::default()),
            #[cfg(target_os = "linux")]
            notifications: StdMutex::new(NotificationConsent::default()),
        })
        .plugin(tauri_plugin_dialog::init())
        // No injected link handler: external links must pass through the native confirmation in
        // `gui::windows`, never be opened by a page's own click handler.
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .with_denylist(&["launcher", "download-context"])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            launch_options,
            launch_workspace,
            build_renderer
        ])
        .setup(|app| {
            menu::install(app)?;
            #[cfg(target_os = "linux")]
            {
                notifications::initialize(app.handle())?;
                windows::create_download_context_owner(app.handle())?;
            }
            show_launcher(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let app = window.app_handle();
                let remaining = app.webview_windows();
                let peers = remaining
                    .keys()
                    .any(|label| label != window.label() && label.starts_with("workspace-"));
                let launcher_visible = app
                    .get_webview_window("launcher")
                    .is_some_and(|launcher| launcher.is_visible().unwrap_or(false));
                if !peers && !launcher_visible {
                    app.exit(0);
                }
            }
        })
        .run(tauri::generate_context!())
        .map_err(|_| "native desktop initialization failed; check system webview prerequisites")
}
