# Changelog

## Unreleased — experimental Rust host

- Pin upstream Hermes Agent/Desktop at `f2cc013158ce4b89f50154c09486a885835133b9` and retain its renderer, gateway and plugin SDK.
- Add a Rust/Tauri launcher, native workspace windows/menus, recovery controls, zoom and window-state restoration without an Electron host process.
- Bound local CLI launch time/output, parse supported launch messages, avoid exposing raw diagnostics, and handle inherited output pipes after successful exit.
- Confine native IPC to the bundled launcher; validate navigation, hide launch credentials from native chrome, and confirm external browser links.
- Add native, session-scoped download-folder consent while preserving authenticated and blob transfers; sanitize names and avoid existing paths.
- Make permission revocation recover stalled dialogs and reject stale callbacks; keep the connection launcher recoverable and provide tested native close/quit shortcuts.
- Improve keyboard pane controls and Unicode/CJK tooltip wrapping with a separable upstream patch.
- Preserve session-export blob lifetime through the existing upstream download helper.
- Add actual Linux WebKit boundary/download/popup checks, an SDK smoke fixture, isolated runtime tooling, and cross-platform CI targets.

Full Electron-native feature parity, signed installers, automatic updates, cross-platform runtime validation and upstream acceptance remain unverified or unimplemented. See `docs/compatibility.md`.
