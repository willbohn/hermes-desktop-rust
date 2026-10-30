//! A labelled notification daemon fixture, reachable only on the native test's private bus.
use glib::variant::ToVariant;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};
use webkit2gtk::{gio, glib};

const INTERFACE: &str = "org.freedesktop.Notifications";
const OBJECT: &str = "/org/freedesktop/Notifications";
const XML: &str = r#"<node><interface name="org.freedesktop.Notifications">
<method name="GetCapabilities"><arg type="as" direction="out"/></method>
<method name="GetServerInformation"><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/></method>
<method name="Notify"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/><arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/></method>
<method name="CloseNotification"><arg type="u" direction="in"/></method>
<signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
<signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
<method name="FixtureClick"><arg type="u" direction="in"/></method>
<method name="FixtureSetMarkup"><arg type="b" direction="in"/></method>
</interface></node>"#;

pub fn run() {
    // An accidental `cargo test -- --ignored` must not attach to the user's notification service.
    let root =
        PathBuf::from(env::var_os("NOTIFICATION_FIXTURE_DIR").expect("private fixture directory"));
    assert_eq!(
        PathBuf::from(env::var_os("HOME").unwrap()),
        root.join("home")
    );
    assert_eq!(PathBuf::from(env::var_os("TMPDIR").unwrap()), root);
    let address = env::var("DBUS_SESSION_BUS_ADDRESS").expect("private session bus");
    let replacement_id = env::var("NOTIFICATION_FIXTURE_START_ID")
        .ok()
        .map(|value| value.parse::<u32>().expect("numeric fixture start id"));
    assert!(replacement_id.is_none_or(|id| id > 0));
    let replacing = replacement_id.is_some();
    let log = Arc::new(Mutex::new(
        OpenOptions::new()
            .append(true)
            .create_new(!replacing)
            .open(root.join("notification-events.jsonl"))
            .unwrap(),
    ));
    let serial = AtomicU32::new(replacement_id.unwrap_or(1));
    let markup = AtomicBool::new(true);
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
    let info = gio::DBusNodeInfo::for_xml(XML)
        .unwrap()
        .lookup_interface(INTERFACE)
        .unwrap();
    let _registration = connection
        .register_object(
            OBJECT,
            &info,
            move |connection, _, _, _, method, params, reply| match method {
                "GetCapabilities" => {
                    let mut capabilities = vec!["body", "actions"];
                    if markup.load(Ordering::Relaxed) {
                        capabilities.push("body-markup");
                    }
                    reply.return_value(Some(&(capabilities,).to_variant()));
                }
                "FixtureSetMarkup" => {
                    markup.store(
                        params.child_value(0).get::<bool>().unwrap(),
                        Ordering::Relaxed,
                    );
                    reply.return_value(Some(&().to_variant()));
                }
                "GetServerInformation" => reply.return_value(Some(
                    &("Hermes test fixture", "Test only", "1", "1.2").to_variant(),
                )),
                "Notify" => {
                    let id = serial.fetch_add(1, Ordering::Relaxed);
                    let record = serde_json::json!({
                        "event": "notify", "id": id,
                        "owner": connection.unique_name().map(|name| name.to_string()),
                        "app": params.child_value(0).get::<String>().unwrap(),
                        "title": params.child_value(3).get::<String>().unwrap(),
                        "body": params.child_value(4).get::<String>().unwrap(),
                        "actions": params.child_value(5).get::<Vec<String>>().unwrap(),
                    });
                    let mut bytes = serde_json::to_vec(&record).unwrap();
                    bytes.push(b'\n');
                    log.lock().unwrap().write_all(&bytes).unwrap();
                    reply.return_value(Some(&(id,).to_variant()));
                }
                "CloseNotification" => {
                    let id = params.child_value(0).get::<u32>().unwrap();
                    writeln!(
                        log.lock().unwrap(),
                        "{}",
                        serde_json::json!({"event":"close","id":id,
                            "owner":connection.unique_name().map(|name| name.to_string())})
                    )
                    .unwrap();
                    connection
                        .emit_signal(
                            None,
                            OBJECT,
                            INTERFACE,
                            "NotificationClosed",
                            Some(&(id, 3_u32).to_variant()),
                        )
                        .unwrap();
                    reply.return_value(Some(&().to_variant()));
                }
                "FixtureClick" => {
                    let id = params.child_value(0).get::<u32>().unwrap();
                    connection
                        .emit_signal(
                            None,
                            OBJECT,
                            INTERFACE,
                            "ActionInvoked",
                            Some(&(id, "default").to_variant()),
                        )
                        .unwrap();
                    reply.return_value(Some(&().to_variant()));
                }
                _ => reply.return_dbus_error(
                    "org.freedesktop.DBus.Error.UnknownMethod",
                    "Unknown fixture method",
                ),
            },
            |_, _, _, _, _| ().to_variant(),
            |_, _, _, _, _, _| false,
        )
        .unwrap();
    let mut flags =
        gio::BusNameOwnerFlags::DO_NOT_QUEUE | gio::BusNameOwnerFlags::ALLOW_REPLACEMENT;
    if replacing {
        flags |= gio::BusNameOwnerFlags::REPLACE;
    }
    let ready = if replacing {
        "notification-replacement-ready.json"
    } else {
        "notification-ready.json"
    };
    let _owner = gio::bus_own_name_on_connection(
        &connection,
        INTERFACE,
        flags,
        move |connection, _| {
            fs::write(
                root.join(ready),
                serde_json::to_vec(&serde_json::json!({"address":address,
                    "owner":connection.unique_name().map(|name| name.to_string())}))
                .unwrap(),
            )
            .unwrap();
        },
        |_, _| {
            eprintln!("Private notification fixture lost its service name");
            std::process::exit(2);
        },
    );
    glib::MainLoop::new(None, false).run();
}
