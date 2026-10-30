# Native download and window verification

The download path retains the original webview transfer. A system-browser fallback loses authenticated cookies and cannot consume page-local blob URLs. Reissuing a one-use URL after a picker is also unsafe.

## Current behavior

Downloads are refused until the user chooses **Workspace → Choose download folder…** (Ctrl/Cmd+Shift+D). The native picker starts at the existing selection or OS download folder, falling back to home. The blocked-download notice offers the same picker; after choosing, retry the original export action. It does not replay a possibly consumed URL.

Consent applies to all workspaces for this app session. **Stop allowing new downloads** revokes it; already authorized transfers may finish. The engine retains cookies and blob ownership. Rust confines suggested names to one bounded UTF-8 component, skips existing files/directories/dangling symlinks, and sets the destination inside the chosen folder. This is not a sandbox against another process running as the same OS user. Permission to download can still consume disk space; no download-progress manager or per-transfer size cap is claimed.

Every webview has the callback, including the launcher. Wry's WebKit handlers attach to a shared context; leaving a permissive default on the launcher bypasses a workspace-only callback. On Linux, a first-created, blank, unprivileged context owner keeps the initial callback valid for the app lifetime: Tauri otherwise cancels it after its originating view disappears, breaking transfers in surviving windows. Hosted pages still receive no native filesystem IPC.

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
8. The first real Return submits a recovered connection form even after a delayed web-process key reply crosses the previous launcher's dismissal.
9. Eight workspaces are allowed and a ninth refused, with or without the launcher; a popup does not dismiss an unrelated connection form.
10. Authenticated downloads still work after both the initial launcher and first workspace have closed.
11. Native shortcuts survive eight launcher closes, with recreation between closes; failure in any cycle fails the test.

These checks passed through Cargo on local WebKitGTK 2.54.2 and Ubuntu 24.04/WebKitGTK 2.52.6 in the combined 23-check native scenario. The no-consent assertion scans the entire disposable tree, including its home, and separately checks the configured OS download directory. Initial experiments failed: GTK's empty Recent view was not a usable default folder selection, so the picker now has an explicit initial directory and focused-window parent. Script-generated popup clicks lacked a user gesture; the popup gesture check uses actual keyboard activation. The additional window-limit check uses the inspector's explicit user-gesture facility to exercise the engine's native creation/refusal path. Native close/quit shortcuts and connection recovery each failed before their fixes; no successful result is obtained by retrying a failed test.

Discovery deliberately includes a retired launcher in a stale inspector snapshot. Only a disconnected target absent from a fresh target list may be skipped; still-live target failures remain fatal and no connection or input action is replayed. X11 `BadWindow` during `X_QueryTree` invalidates an entire search result: the helper discards partial output and repeats only that read, at most three times. Persistent or unrelated errors fail rather than masquerading as an absent window/consent dialog.

The inline create/click/immediately-revoke pattern failed in WebKit; the existing upstream helper (attached anchor plus deferred revocation) passed. `desktop-downloads.patch` switches session export to that helper. Its new regression failed first on immediate revocation, then passed with existing download tests: 8 tests in 2 files. ESLint, project TypeScript checking and formatting passed for the export change. macOS/Windows native download behavior is not yet verified.
