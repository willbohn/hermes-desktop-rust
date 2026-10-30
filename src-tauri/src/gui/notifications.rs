//! Linux engine notifications retain the browser API, with native, session-only window consent.
use super::HostState;
mod delivery;
use hermes_desktop_rust::notifications::Prompt;
use jsc::ValueExt;

use glib::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use tauri::{Manager, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tokio::sync::oneshot;
use url::Url;
use webkit2gtk::{
    NotificationExt, NotificationPermissionRequest, PermissionRequest, PermissionRequestExt,
    WebView, WebViewExt, glib,
};

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

fn permitted(window: &WebviewWindow, scope: &str) -> bool {
    window
        .state::<HostState>()
        .notifications
        .lock()
        .is_ok_and(|consent| consent.allows(window.label(), scope))
}

/// The WebKit permission requests that must receive the one native decision for
/// a window's open prompt. Coalescing means a second request for the same
/// window and origin is answered by that decision instead of being denied.
#[derive(Default)]
struct Decision {
    requests: RefCell<Vec<PermissionRequest>>,
}

std::thread_local! {
    /// One decision per (window label, prompt serial). Every entry stays on the
    /// GTK main thread, so the non-`Send` WebKit request objects never cross a
    /// thread, and the map is bounded by the number of open prompts.
    static DECISIONS: RefCell<HashMap<(String, u64), Rc<Decision>>> = RefCell::new(HashMap::new());
}

/// Keep the GObject request on GTK's thread; only a Boolean crosses the dialog callback.
fn ask(window: &WebviewWindow, scope: String, prompt: u64) -> oneshot::Receiver<bool> {
    let (sender, receiver) = oneshot::channel();
    let target = window.clone();
    window
        .dialog()
        .message(format!(
            "Allow content in this workspace window, including embedded pages, to display desktop notifications for this app session?\n\nWindow website: {scope}\n\nNotification text may appear on your lock screen. Stop new notifications from the Workspace menu. This does not grant filesystem or process access.",
        ))
        .title("Allow desktop notifications?")
        .parent(window)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Allow notifications".into(),
            "Block".into(),
        ))
        .show(move |accepted| {
            let still_here = target.url().ok().and_then(|url| origin(&url)).as_deref()
                == Some(scope.as_str());
            let allowed = target
                .state::<HostState>()
                .notifications
                .lock()
                .is_ok_and(|mut consent| consent.finish_prompt(prompt, accepted && still_here));
            let _ = sender.send(allowed);
        });
    receiver
}

/// Opens, or joins, the single prompt for this window and origin, returning the
/// decision every request for it must join. Only the opener shows the dialog and
/// spawns the resolution future; a joiner is never denied.
fn decision_for(view: &WebView, window: &WebviewWindow, scope: &str) -> Option<Rc<Decision>> {
    let outcome = window
        .state::<HostState>()
        .notifications
        .lock()
        .ok()
        .and_then(|mut consent| consent.begin_prompt(window.label(), scope))?;
    let serial = outcome.serial();
    let label = window.label().to_owned();
    if matches!(outcome, Prompt::Joined(_)) {
        return DECISIONS.with(|decisions| decisions.borrow().get(&(label, serial)).cloned());
    }
    let decision = Rc::new(Decision::default());
    DECISIONS.with(|decisions| {
        decisions
            .borrow_mut()
            .insert((label.clone(), serial), decision.clone())
    });
    let result = ask(window, scope.to_owned(), serial);
    let target = window.clone();
    let scope = scope.to_owned();
    let view = view.downgrade();
    glib::MainContext::default().spawn_local(async move {
        let accepted = result.await.unwrap_or(false);
        let still_here = view
            .upgrade()
            .and_then(|view| view_origin(&view))
            .as_deref()
            == Some(scope.as_str());
        let allow = accepted && still_here && permitted(&target, &scope);
        let pending = DECISIONS.with(|decisions| decisions.borrow_mut().remove(&(label, serial)));
        if let Some(pending) = pending {
            // Every request that joined this decision receives the actual one.
            for request in pending.requests.borrow_mut().drain(..) {
                if allow {
                    request.allow();
                } else {
                    request.deny();
                }
            }
        }
    });
    Some(decision)
}

pub(crate) fn choose(window: &WebviewWindow) {
    if !window.label().starts_with("workspace-") {
        return;
    }
    let Some(scope) = window.url().ok().and_then(|url| origin(&url)) else {
        return;
    };
    let target = window.clone();
    let _ = window.with_webview(move |platform| {
        let view = platform.inner();
        let current = view.downgrade();
        // Only a native menu gesture reaches this. The isolated world avoids page replacements;
        // WebKit supplies user activation. No polyfill or page-to-native bridge is installed.
        view.call_async_javascript_function(
            "if (Notification.permission === 'granted') return 'resume'; if (Notification.permission === 'denied') return 'blocked'; await Notification.requestPermission(); return 'requested';",
            None, Some("hermes-desktop-notifications"), None, webkit2gtk::gio::Cancellable::NONE,
            move |result| {
                if current.upgrade().and_then(|view| view_origin(&view)).as_deref() != Some(scope.as_str()) {
                    return;
                }
                let status = result.ok().filter(|value| value.is_string()).map(|value| value.to_str());
                match status.as_ref().map(|value| value.as_str()) {
                    Some("requested") => (), // The engine request already went through native consent.
                    Some("resume") => {
                        // The engine already granted; re-affirm host consent through the same
                        // single prompt so it coalesces with any concurrent page request.
                        if let Some(view) = current.upgrade() {
                            let _ = decision_for(&view, &target, &scope);
                        }
                    }
                    status => {
                        let message = if status == Some("blocked") {
                            "Notifications were blocked in this browser session. Quit and reopen Hermes Desktop Rust, then choose Workspace → Allow notifications in this window. Your sign-in data is not cleared."
                        } else {
                            "The workspace could not request notifications. Reload it and try again."
                        };
                        target.dialog().message(message).title("Desktop notifications").parent(&target).show(|_| {});
                    }
                }
            },
        );
    });
}

pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), &'static str> {
    delivery::initialize(app)
}

/// Confirm every view has its native handlers before loading untrusted content.
/// Tauri runs this callback inline on GTK's thread, or dispatches it there from a worker.
fn configure(
    window: &WebviewWindow,
    install: impl FnOnce(WebView) + Send + 'static,
) -> Result<(), &'static str> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    window
        .with_webview(move |platform| {
            install(platform.inner());
            let _ = sender.send(());
        })
        .map_err(|_| "The webview could not install its notification policy")?;
    receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "The webview did not confirm its notification policy")
}

pub(crate) fn install(window: &WebviewWindow) -> Result<(), &'static str> {
    let target = window.clone();
    configure(window, move |view| {
        let delivering = target.clone();
        let capacity = Rc::new(Cell::new(0));
        view.connect_show_notification(move |view, notification| {
            let Some(scope) = view_origin(view) else {
                notification.close();
                return true;
            };
            let grant = delivering
                .state::<HostState>()
                .notifications
                .lock()
                .ok()
                .and_then(|consent| consent.grant_id(delivering.label(), &scope));
            if let Some(grant) = grant {
                delivery::show(view, notification, &delivering, scope, grant, &capacity);
            } else {
                notification.close();
            }
            // Never fall through to libnotify: it would bypass revocation and render raw markup.
            true
        });
        let previous = RefCell::new(view_origin(&view));
        let navigating = target.clone();
        view.connect_uri_notify(move |view| {
            let current = view_origin(view);
            if previous.replace(current.clone()) != current
                && let Ok(mut consent) = navigating.state::<HostState>().notifications.lock()
            {
                consent.forget(navigating.label());
            }
        });
        view.connect_permission_request(move |view, request| {
            if !request.is::<NotificationPermissionRequest>() {
                return false;
            }
            let Some(scope) = view_origin(view) else {
                request.deny();
                return true;
            };
            if permitted(&target, &scope) {
                request.allow();
                return true;
            }
            match decision_for(view, &target, &scope) {
                // Join the one native decision for this window and origin; a
                // concurrent request is never turned into a permanent denial.
                Some(decision) => {
                    decision.requests.borrow_mut().push(request.clone());
                    true
                }
                // Only an (unreachable) serial overflow leaves no prompt to join.
                None => {
                    request.deny();
                    true
                }
            }
        });
    })
}

/// The bundled launcher never grants notification consent or falls through to libnotify.
pub(crate) fn install_denied(window: &WebviewWindow) -> Result<(), &'static str> {
    configure(window, |view| {
        view.connect_show_notification(|_, notification| {
            notification.close();
            true
        });
        view.connect_permission_request(|_, request| {
            if request.is::<NotificationPermissionRequest>() {
                request.deny();
                return true;
            }
            false
        });
    })
}
