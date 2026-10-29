//! Real WebKit behavior, replacing assertions that merely searched GUI source strings.
//! Requires Linux, Node 24+, Xvfb, xdotool, D-Bus and openbox (or existing kwin_x11).
#![cfg(all(target_os = "linux", feature = "desktop"))]

use std::{path::Path, process::Command};

#[test]
fn native_workspace_boundary_download_consent_and_popup_handoff() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = Command::new("xvfb-run")
        .args(["-a", "-s", "-screen 0 1280x960x24 -nolisten tcp", "node"])
        .arg(root.join("scripts/native-window-check.mjs"))
        .arg(env!("CARGO_BIN_EXE_hermes-desktop-rust"))
        .current_dir(root)
        .output()
        .expect("Run the documented Linux native-test prerequisites");
    assert!(
        output.status.success(),
        "Native behavior check failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
