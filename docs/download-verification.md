# Native download and window verification

The download path retains the original webview transfer. A system-browser fallback loses authenticated cookies and cannot consume page-local blob URLs. Reissuing a one-use URL after a picker is also unsafe.

## Current behavior

Downloads are refused until the user chooses **Workspace → Choose download folder…** (Ctrl/Cmd+Shift+D). The native picker starts at the existing selection or OS download folder, falling back to home. The blocked-download notice offers the same picker; after choosing, retry the original export action. It does not replay a possibly consumed URL.

Consent applies to all workspaces for this app session. **Stop allowing new downloads** revokes it; already authorized transfers may finish. The engine retains cookies and blob ownership. Rust confines suggested names to one bounded UTF-8 component, skips existing files/directories/dangling symlinks, and sets the destination inside the chosen folder. This is not a sandbox against another process running as the same OS user. Permission to download can still consume disk space; no download-progress manager or per-transfer size cap is claimed.

Every webview has the callback, including the launcher. Wry's WebKit handlers attach to a shared context; leaving a permissive default on the launcher bypasses a workspace-only callback. Hosted pages still receive no native filesystem IPC.

Keep declarative `app.windows` empty so the guarded Rust builders create every view. Revocation also resets a stalled dialog gate and invalidates its generation; an older callback cannot re-grant permission or clear a newer prompt. Canceling a replacement folder picker preserves existing consent. The focused state regression failed before recovery was implemented and now passes.

## Executed on Linux

`cargo test --locked --test window_contract` runs `scripts/native-window-check.mjs` in a separate Xvfb display, private D-Bus session and disposable home. Its HTTP fixture is explicitly labelled, not fake Hermes/model output. It verifies:

1. A hosted page cannot invoke the native launcher command.
2. No file is written before native folder consent, despite a valid OS download directory.
3. The original authenticated request saves expected fixture bytes in the selected folder.
4. The actual pinned upstream `downloadBlob` helper saves expected blob bytes through WebKit.
5. A keyboard-activated same-origin popup loads exactly once.
6. Closing the connection launcher does not strand active workspaces: the recovery menu opens it again.
7. The explicit native Quit shortcut exits the host successfully.

All seven passed through Cargo. The no-consent assertion scans the entire disposable tree, including its home, and separately checks the configured OS download directory. Initial experiments failed: GTK's empty Recent view was not a usable default folder selection, so the picker now has an explicit initial directory and focused-window parent. Script-generated popup clicks lacked a user gesture; the test uses actual keyboard activation, not a bypass. Native close/quit shortcuts and connection recovery each failed before the corresponding fixes; no successful result is obtained by retrying a failed test.

The inline create/click/immediately-revoke pattern failed in WebKit; the existing upstream helper (attached anchor plus deferred revocation) passed. `desktop-downloads.patch` switches session export to that helper. Its new regression failed first on immediate revocation, then passed with existing download tests: 8 tests in 2 files. ESLint, project TypeScript checking and formatting passed for the export change. macOS/Windows native download behavior is not yet verified.
