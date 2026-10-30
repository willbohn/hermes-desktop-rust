//! Linux media configuration; native microphone consent is installed before navigation.
use super::HostState;
use std::{cell::Cell, rc::Rc};
use tauri::{Manager, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tokio::sync::oneshot;
use url::Url;
use webkit2gtk::{
    LoadEvent, PermissionRequestExt, SettingsExt, UserMediaPermissionRequest,
    UserMediaPermissionRequestExt, WebView, WebViewExt, glib, glib::prelude::*,
};

/// The normalized top-level origin, or `None` when the address is not a plain HTTP(S) site.
fn origin(url: &Url) -> Option<String> {
    (matches!(url.scheme(), "http" | "https")
        && url.has_host()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.origin().ascii_serialization())
}

fn view_origin(view: &WebView) -> Option<String> {
    origin(&Url::parse(view.uri()?.as_str()).ok()?)
}

/// Ask once for this window and document. Only a Boolean crosses the dialog callback; the
/// non-`Send` WebKit request object stays on GTK's main thread inside `spawn_local`.
fn decide(
    view: &WebView,
    window: &WebviewWindow,
    request: UserMediaPermissionRequest,
    generation: Rc<Cell<u64>>,
) {
    let Some(scope) = view_origin(view) else {
        request.deny();
        return;
    };
    let asked = generation.get();
    let (sender, receiver) = oneshot::channel();
    let target = window.clone();
    window
        .dialog()
        .message(format!(
            "Allow content in this workspace window, including embedded pages, to use the microphone?\n\nWindow website: {scope}\n\nAccess lasts until this workspace is closed. Recording can continue in the background while the window is open, and closing this window ends microphone access. This does not grant filesystem or process access.",
        ))
        .title("Allow microphone access?")
        .parent(window)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Allow microphone".into(),
            "Block".into(),
        ))
        .show(move |accepted| {
            let _ = sender.send(accepted);
        });
    let weak = view.downgrade();
    glib::MainContext::default().spawn_local(async move {
        let accepted = receiver.await.unwrap_or(false);
        // Only the document that asked may receive the answer: refuse late consent once the
        // top-level document has reloaded, its origin has changed or the window has closed.
        let same_document = generation.get() == asked
            && weak
                .upgrade()
                .and_then(|view| view_origin(&view))
                .as_deref()
                == Some(scope.as_str())
            && target.url().is_ok();
        if accepted && same_document {
            request.allow();
        } else {
            request.deny();
        }
    });
}

/// Confirm the mock-capture setting and install the native microphone decision before the
/// workspace navigates. The engine keeps its own cached grant, so the probe and the recording
/// reacquisition stay a single native decision.
pub(crate) fn install(window: &WebviewWindow) -> Result<(), &'static str> {
    let mock = window.state::<HostState>().options.test_mock_media;
    let target = window.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    window
        .with_webview(move |platform| {
            let view = platform.inner();
            let configured = view.settings().is_some_and(|settings| {
                settings.set_enable_mock_capture_devices(mock);
                settings.set_enable_media_stream(true);
                true
            });
            // Every committed top-level document starts a new generation, so a prompt still
            // open across a reload can never consent on behalf of the replacement document.
            let generation = Rc::new(Cell::new(0u64));
            {
                let generation = generation.clone();
                view.connect_load_changed(move |_view, event| {
                    if matches!(event, LoadEvent::Committed) {
                        generation.set(generation.get().wrapping_add(1));
                    }
                });
            }
            let deciding = target.clone();
            view.connect_permission_request(move |view, request| {
                let Some(media) = request.downcast_ref::<UserMediaPermissionRequest>() else {
                    // Unrelated permission types stay available to other native handlers.
                    return false;
                };
                if !deciding.label().starts_with("workspace-")
                    || !media.is_for_audio_device()
                    || media.is_for_video_device()
                {
                    // Only workspace microphone capture is consented; the launcher, camera and
                    // display capture are refused without prompting.
                    media.deny();
                    return true;
                }
                decide(view, &deciding, media.clone(), generation.clone());
                true
            });
            let _ = sender.send(configured);
        })
        .map_err(|_| "The webview could not configure media capture")?;
    match receiver.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(true) => Ok(()),
        _ => Err("The webview did not confirm its media configuration"),
    }
}
