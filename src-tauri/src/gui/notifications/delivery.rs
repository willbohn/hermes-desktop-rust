//! Session-only freedesktop delivery, with engine fallbacks blocked on the shared bus.
use super::{HostState, view_origin};
use glib::{prelude::*, variant::ToVariant};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use tauri::{Manager, WebviewWindow};
use webkit2gtk::{Notification, NotificationExt, WebView, gio, glib};

const BUS: &str = "org.freedesktop.Notifications";
const OBJECT: &str = "/org/freedesktop/Notifications";
const MAX_ACTIVE: usize = 32;

struct Transport {
    // Keep the shared connection and its filter alive for the entire app session.
    _shared: gio::DBusConnection,
    _filter: gio::FilterId,
    approved: gio::DBusConnection,
}

pub(super) fn initialize(app: &tauri::AppHandle) -> Result<(), &'static str> {
    let shared = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(|_| "Could not secure the desktop notification connection")?;
    // Never reconnect to an unfiltered shared bus after session-bus failure.
    shared.set_exit_on_close(true);
    let filter = shared.add_filter(|_, message, incoming| {
        // GIO runs filters on its I/O thread: no captured GUI objects or mutable state.
        let interface = message.interface();
        let path = message.path();
        let member = message.member();
        let desktop_mutation = (interface.as_deref() == Some(BUS)
            || path.as_deref() == Some(OBJECT))
            && matches!(member.as_deref(), Some("Notify" | "CloseNotification"));
        let portal_mutation = (interface.as_deref() == Some("org.freedesktop.portal.Notification")
            || path.as_deref() == Some("/org/freedesktop/portal/desktop"))
            && matches!(
                member.as_deref(),
                Some("AddNotification" | "RemoveNotification")
            );
        if !incoming
            && message.message_type() == gio::DBusMessageType::MethodCall
            && (desktop_mutation || portal_mutation)
        {
            None
        } else {
            Some(message.clone())
        }
    });
    let address = gio::dbus_address_get_for_bus_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(|_| "Could not connect approved desktop notifications")?;
    let approved = gio::DBusConnection::for_address_sync(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
    .map_err(|_| "Could not connect approved desktop notifications")?;
    approved.set_exit_on_close(false);
    if shared.unique_name() == approved.unique_name() {
        return Err("Desktop notification connections were not isolated");
    }
    if !app.manage(Transport {
        _shared: shared,
        _filter: filter,
        approved,
    }) {
        return Err("Desktop notifications were initialized twice");
    }
    Ok(())
}

struct Active {
    connection: gio::DBusConnection,
    owner: String,
    id: u32,
    subscription: Option<gio::SignalSubscriptionId>,
    close_on_drop: bool,
}

impl Drop for Active {
    fn drop(&mut self) {
        if let Some(subscription) = self.subscription.take() {
            self.connection.signal_unsubscribe(subscription);
        }
        if self.close_on_drop {
            self.connection.call(
                Some(&self.owner),
                OBJECT,
                BUS,
                "CloseNotification",
                Some(&(self.id,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                2000,
                gio::Cancellable::NONE,
                |_| {},
            );
        }
    }
}

struct Delivery {
    view: glib::WeakRef<WebView>,
    window: WebviewWindow,
    scope: String,
    grant: u64,
    capacity: Rc<Cell<usize>>,
    closed: Cell<bool>,
    active: RefCell<Option<Active>>,
}

impl Delivery {
    fn current(&self) -> bool {
        !self.closed.get()
            && self
                .view
                .upgrade()
                .and_then(|view| view_origin(&view))
                .as_deref()
                == Some(&self.scope)
            && self
                .window
                .state::<HostState>()
                .notifications
                .lock()
                .is_ok_and(|consent| {
                    consent.grant_id(self.window.label(), &self.scope) == Some(self.grant)
                })
    }
}

impl Drop for Delivery {
    fn drop(&mut self) {
        self.capacity.set(self.capacity.get() - 1);
    }
}

fn text(value: &str, limit: usize) -> String {
    value
        .chars()
        .take(limit)
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

pub(super) fn show(
    view: &WebView,
    notification: &Notification,
    window: &WebviewWindow,
    scope: String,
    grant: u64,
    capacity: &Rc<Cell<usize>>,
) {
    if capacity.get() >= MAX_ACTIVE {
        notification.close();
        return;
    }
    capacity.set(capacity.get() + 1);
    let delivery = Rc::new(Delivery {
        view: view.downgrade(),
        window: window.clone(),
        scope,
        grant,
        capacity: capacity.clone(),
        closed: Cell::new(false),
        active: RefCell::new(None),
    });
    let closing = delivery.clone();
    notification.connect_closed(move |_| {
        closing.closed.set(true);
        closing.active.borrow_mut().take();
    });
    let title = text(
        notification.title().as_ref().map_or("", |s| s.as_str()),
        160,
    );
    let body = text(
        notification.body().as_ref().map_or("", |s| s.as_str()),
        2048,
    );
    let connection = window.state::<Transport>().approved.clone();
    let notification = notification.downgrade();
    glib::MainContext::default().spawn_local(async move {
        let Some(note) = notification.upgrade() else {
            return;
        };

        if !delivery.current() {
            note.close();
            return;
        }
        // Capture the actual replying daemon, preserving service auto-activation.
        // Reusable bus names and numeric IDs must not cross a daemon replacement.
        let request =
            gio::DBusMessage::new_method_call(Some(BUS), OBJECT, Some(BUS), "GetCapabilities");
        request.set_body(&().to_variant());
        let capabilities = connection
            .send_message_with_reply_future(&request, gio::DBusSendMessageFlags::NONE, 2000)
            .await
            .ok()
            .filter(|reply| reply.message_type() == gio::DBusMessageType::MethodReturn)
            .and_then(|reply| {
                let owner = reply
                    .sender()
                    .filter(|name| gio::dbus_is_unique_name(name))?;
                let (capabilities,) = reply.body()?.get::<(Vec<String>,)>()?;
                Some((owner.to_string(), capabilities))
            });
        let Some((owner, capabilities)) = capabilities else {
            note.close();
            return;
        };
        if !delivery.current() {
            note.close();
            return;
        }
        let body = if capabilities
            .iter()
            .any(|capability| capability == "body-markup")
        {
            glib::markup_escape_text(&body).to_string()
        } else {
            body
        };
        let actions = if capabilities
            .iter()
            .any(|capability| capability == "actions")
        {
            vec!["default", "Open workspace"]
        } else {
            vec![]
        };
        let parameters = (
            "hermes-desktop-rust",
            0_u32,
            "",
            title,
            body,
            actions,
            HashMap::from([(
                "desktop-entry",
                "io.github.willbohn.hermes-desktop-rust".to_variant(),
            )]),
            -1_i32,
        )
            .to_variant();
        let replying = connection.clone();
        let target = owner.clone();
        connection.call(
            Some(&owner),
            OBJECT,
            BUS,
            "Notify",
            Some(&parameters),
            None,
            gio::DBusCallFlags::NONE,
            2000,
            gio::Cancellable::NONE,
            move |result| {
                let Some((id,)) = result
                    .ok()
                    .and_then(|value| value.get::<(u32,)>())
                    .filter(|(id,)| *id != 0)
                else {
                    if let Some(note) = notification.upgrade() {
                        note.close();
                    }
                    return;
                };
                let mut active = Active {
                    connection: replying.clone(),
                    owner: target,
                    id,
                    subscription: None,
                    close_on_drop: true,
                };
                if !delivery.current() {
                    if let Some(note) = notification.upgrade() {
                        note.close();
                    }
                    return; // Drop closes even a late OS reply after revocation/navigation.
                }
                let weak = Rc::downgrade(&delivery);
                let subscription = replying.signal_subscribe(
                    Some(&active.owner),
                    Some(BUS),
                    None,
                    Some(OBJECT),
                    None,
                    gio::DBusSignalFlags::NONE,
                    move |_, _, _, _, signal, parameters| {
                        let Some(delivery) = weak.upgrade() else {
                            return;
                        };
                        let Some(note) = notification.upgrade() else {
                            delivery.active.borrow_mut().take();
                            return;
                        };
                        let id = delivery.active.borrow().as_ref().map(|active| active.id);
                        if signal == "NotificationClosed" {
                            if let Some((closed_id, _reason)) = parameters.get::<(u32, u32)>()
                                && id == Some(closed_id)
                            {
                                if let Some(mut active) = delivery.active.borrow_mut().take() {
                                    active.close_on_drop = false;
                                }
                                note.close();
                            }
                        } else if signal == "ActionInvoked"
                            && let Some((clicked_id, action)) = parameters.get::<(u32, String)>()
                            && id == Some(clicked_id)
                            && action == "default"
                            && delivery.current()
                        {
                            let _ = delivery.window.unminimize();
                            let _ = delivery.window.set_focus();
                            note.clicked();
                            note.close();
                        }
                    },
                );
                active.subscription = Some(subscription);
                *delivery.active.borrow_mut() = Some(active);
            },
        );
    });
}
