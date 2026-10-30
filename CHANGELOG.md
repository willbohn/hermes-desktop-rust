# Changelog

## Unreleased — experimental Rust host

### Added

- Pin upstream Hermes Agent/Desktop at `f2cc013158ce4b89f50154c09486a885835133b9` and retain its renderer, gateway and plugin SDK.
- Add a Rust/Tauri launcher, native workspace windows/menus, recovery controls, zoom and window-state restoration without an Electron host process.
- Bound local CLI launch time/output, parse supported launch messages, avoid exposing raw diagnostics, and handle inherited output pipes after successful exit.
- Confine native IPC to the bundled launcher; validate navigation, hide launch credentials from native chrome, and confirm external browser links.
- Add native, session-scoped download-folder consent while preserving authenticated and blob transfers; sanitize names and avoid existing paths.
- Add Linux completion notifications through the existing browser API, with native per-window consent, bounded text, owner-bound click/close handling and session revocation. Concurrent requests share the actual consent decision instead of treating a busy prompt as denial.
- Block unattributed engine notification fallbacks, including service-worker delivery, on the shared GIO connection; approved Rust delivery uses a separate connection. Keep browser APIs, workers and launcher-only IPC unchanged.
- Add Linux microphone consent through the existing browser media API. Keep the engine's probe/reacquire flow, reject stale decisions and non-microphone requests, and use workspace close as the revocation boundary. Native tests use synthetic devices and actual encoding, not physical audio or a fake recording API.
- Make permission revocation recover stalled dialogs and reject stale callbacks; keep the connection launcher recoverable and provide tested native close/quit shortcuts.
- Improve keyboard pane controls and Unicode/CJK tooltip wrapping with a separable upstream patch.
- Preserve session-export blob lifetime through the existing upstream download helper.
- Add actual Linux WebKit boundary/download/popup checks, an SDK smoke fixture, isolated runtime tooling, and cross-platform CI targets.

### Fixed

- Include Ubuntu's GStreamer recording prerequisites; native WebKit tests exercise both local WebM/Opus and Ubuntu MP4/AAC encoding instead of accepting permission-only success.
- Recover the first native key after dismissing a busy connection launcher. Retire the old webview instead of retaining GTK WebKit's stale key-replay state; recreate it through the same guarded builder.
- Keep Linux's shared-context download policy alive independently of connection/workspace windows, without granting the blank policy owner native IPC or page-navigation authority.
- Count only workspaces toward the eight-window bound. A popup or later page reload no longer dismisses an unrelated connection form.

### Changed

- Connection forms restart from CLI defaults after dismissal, rather than preserving unsaved edits in a hidden webview. Startup auto-connect is consumed once, not repeated on recovery.
- The native regression forces a delayed keyboard reply across dismissal and checks real first-key submission. Inspector discovery skips unloaded blank contexts; temporary GTK input tracing is removed.

Full Electron-native feature parity, signed installers, automatic updates, cross-platform runtime validation and upstream acceptance remain unverified or unimplemented. See `docs/compatibility.md`.
