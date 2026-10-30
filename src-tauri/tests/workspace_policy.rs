//! Pure policy for workspace windows: popup origin, download names and bounded zoom.
//! No webview, gateway or live data is involved.
use hermes_desktop_rust::policy::{
    ZOOM_LEVELS, confirmable_link, download_destination, next_download_destination,
    popup_origin_allowed, sanitized_download_name, zoom_step,
};
use std::path::Path;
use url::Url;

fn url(value: &str) -> Url {
    Url::parse(value).expect("test url")
}

#[test]
fn a_popup_is_allowed_only_from_the_document_the_user_is_on() {
    // Same origin as the live document, including after a redirect to an identity provider.
    assert!(popup_origin_allowed(
        &url("https://app.example/"),
        &url("https://app.example/child")
    ));
    assert!(popup_origin_allowed(
        &url("https://accounts.example/authorize?x=1"),
        &url("https://accounts.example/oauth/popup")
    ));
    assert!(popup_origin_allowed(
        &url("http://127.0.0.1:9120/webapp/"),
        &url("http://127.0.0.1:9120/popup")
    ));

    // A popup to any other origin is not handed a window; it goes to the external confirmation.
    assert!(!popup_origin_allowed(
        &url("https://accounts.example/"),
        &url("https://app.example/")
    ));
    assert!(!popup_origin_allowed(
        &url("https://app.example/"),
        &url("http://app.example/")
    ));
    assert!(!popup_origin_allowed(
        &url("http://127.0.0.1:9120/"),
        &url("http://127.0.0.1:9121/")
    ));
    // Non-http schemes, credentials and opaque documents never open an in-process window.
    assert!(!popup_origin_allowed(
        &url("https://app.example/"),
        &url("javascript:alert(1)")
    ));
    assert!(!popup_origin_allowed(
        &url("https://app.example/"),
        &url("https://user:secret@app.example/")
    ));
    assert!(!popup_origin_allowed(
        &url("about:blank"),
        &url("https://app.example/")
    ));
}

#[test]
fn the_confirmation_names_the_address_without_query_or_fragment() {
    assert_eq!(
        confirmable_link(&url("https://docs.example/guide/page?token=SECRET#part-2")),
        "https://docs.example/guide/page"
    );
    assert_eq!(
        confirmable_link(&url("https://docs.example/")),
        "https://docs.example/"
    );
}

#[test]
fn a_suggested_download_name_is_confined_to_one_safe_segment() {
    assert_eq!(sanitized_download_name("report.pdf"), "report.pdf");
    assert_eq!(sanitized_download_name("../../etc/passwd"), "passwd");
    assert_eq!(sanitized_download_name("/absolute/evil.txt"), "evil.txt");
    assert_eq!(sanitized_download_name(r"C:\Windows\evil.exe"), "evil.exe");
    assert_eq!(sanitized_download_name("a\u{0}b\u{7f}.txt"), "ab.txt");
    assert_eq!(sanitized_download_name(".."), "download");
    assert_eq!(sanitized_download_name("."), "download");
    assert_eq!(sanitized_download_name(""), "download");
    assert_eq!(sanitized_download_name("   "), "download");
    assert_eq!(sanitized_download_name("hidden."), "hidden");
    // Windows device names are never created verbatim.
    assert_eq!(sanitized_download_name("CON"), "_CON");
    assert_eq!(sanitized_download_name("con.txt"), "_con.txt");
    // Length is bounded.
    let long = "x".repeat(500);
    assert_eq!(sanitized_download_name(&long).chars().count(), 180);
}

#[test]
fn a_download_destination_stays_inside_the_given_directory() {
    let dir = Path::new("/tmp/hermes-downloads");
    let destination = download_destination(dir, "../../etc/passwd");
    assert_eq!(destination, dir.join("passwd"));
    assert!(destination.starts_with(dir));
}

#[test]
fn download_names_fit_filesystems_with_utf8_byte_limits() {
    let name = sanitized_download_name(&"日本語".repeat(120));
    assert!(name.len() <= 180);
    assert!(!name.is_empty());
}

#[test]
fn a_download_does_not_replace_an_existing_file_or_directory() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("report.txt"), b"keep this").unwrap();
    std::fs::create_dir(folder.path().join("report (1).txt")).unwrap();
    let next = next_download_destination(folder.path(), "../../report.txt").unwrap();
    assert_eq!(next, folder.path().join("report (2).txt"));
    assert_eq!(
        std::fs::read(folder.path().join("report.txt")).unwrap(),
        b"keep this"
    );
    assert!(!next.exists());
}

#[cfg(unix)]
#[test]
fn a_download_does_not_follow_a_dangling_symlink() {
    let folder = tempfile::tempdir().unwrap();
    let elsewhere = folder.path().join("must-not-be-created");
    std::os::unix::fs::symlink(&elsewhere, folder.path().join("report.txt")).unwrap();
    assert_eq!(
        next_download_destination(folder.path(), "report.txt").unwrap(),
        folder.path().join("report (1).txt")
    );
    assert!(!elsewhere.exists());
}

#[test]
fn zoom_is_incremental_and_bounded_at_both_ends() {
    assert_eq!(zoom_step(1.0, 1), 1.1);
    assert_eq!(zoom_step(1.0, -1), 0.9);
    assert_eq!(zoom_step(1.0, 0), 1.0);
    assert_eq!(zoom_step(2.0, 0), 1.0);
    // Clamped at the ends, never past them.
    assert_eq!(zoom_step(ZOOM_LEVELS[ZOOM_LEVELS.len() - 1], 1), 3.0);
    assert_eq!(zoom_step(ZOOM_LEVELS[0], -1), 0.5);
    // A value between levels steps to the neighbouring level in that direction.
    assert_eq!(zoom_step(1.05, 1), 1.1);
    assert_eq!(zoom_step(1.05, -1), 1.0);
}

#[test]
fn repeated_zooming_never_leaves_the_ladder() {
    let mut level = 1.0_f64;
    for _ in 0..64 {
        level = zoom_step(level, 1);
        assert!(ZOOM_LEVELS.contains(&level));
    }
    assert_eq!(level, 3.0);
    for _ in 0..64 {
        level = zoom_step(level, -1);
        assert!(ZOOM_LEVELS.contains(&level));
    }
    assert_eq!(level, 0.5);
}
