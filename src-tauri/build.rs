//! Build only the local launcher IPC; hosted pages receive no native permissions.
fn main() {
    #[cfg(feature = "desktop")]
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "launch_workspace",
            "build_renderer",
            "launch_options",
        ]),
    ))
    .expect("could not build the desktop resources");
}
