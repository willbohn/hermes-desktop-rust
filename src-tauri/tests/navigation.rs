//! Navigation never turns a hosted page into a privileged local Tauri page.
use hermes_desktop_rust::policy::{is_launcher, navigation_allowed, window_title};
use url::Url;

#[test]
fn confines_loopback_and_allows_https_sign_in_without_native_privileges() {
    let workspace = Url::parse("http://127.0.0.1:9120/").unwrap();
    for target in [
        "http://127.0.0.1:9120/webapp/",
        "https://login.example/authorize",
        "about:blank",
    ] {
        assert!(
            navigation_allowed(&workspace, &Url::parse(target).unwrap()),
            "allow {target}"
        );
    }
    for target in [
        "http://127.0.0.1:9121/",
        "http://evil.example/",
        "file:///etc/passwd",
        "tauri://localhost/index.html",
        "https://user:secret@evil.example/",
    ] {
        assert!(
            !navigation_allowed(&workspace, &Url::parse(target).unwrap()),
            "refuse {target}"
        );
    }
}

#[test]
fn native_chrome_never_contains_sign_in_material_or_page_supplied_titles() {
    let target = Url::parse("https://host.example/webapp/launch?ticket=SECRET#private").unwrap();
    assert_eq!(
        window_title(&target),
        "Hermes Desktop Rust — https://host.example"
    );
}

#[test]
fn only_bundled_launcher_can_request_native_operations() {
    for origin in [
        "tauri://localhost",
        "tauri://localhost/",
        "http://tauri.localhost/",
        "tauri://localhost/index.html",
        "http://tauri.localhost/index.html",
        "https://tauri.localhost/index.html",
    ] {
        let url = Url::parse(origin).unwrap();
        assert!(is_launcher("launcher", &url));
        assert!(!is_launcher("workspace-1", &url));
    }
    for origin in [
        "http://localhost/index.html",
        "https://hermes.example/",
        "tauri://evil/index.html",
    ] {
        assert!(!is_launcher("launcher", &Url::parse(origin).unwrap()));
    }
}
