# Linux notification behavior and limits

The host preserves the renderer's real `Notification` API. It does not add workspace native IPC, replace browser permission properties, or disable service workers.

## User controls

- **Workspace → Allow notifications in this window…** (Ctrl+Alt+N) requests native consent for this app session. The grant covers content in that window, including embedded pages, and is dropped when the top-level origin changes or the window closes.
- **Stop new notifications** (Ctrl+Alt+M) revokes host delivery across workspaces and invalidates pending decisions. Existing visible notifications may remain, but their actions cannot regain a revoked grant. A late reply for an in-flight OS delivery is closed.
- Browser permission and host delivery consent are separate. Stop does not change `Notification.permission`. An explicit browser-level refusal remains blocked until the app restarts; sign-in data is not cleared.
- Notification text can appear on a lock screen. The native dialog discloses this before granting consent.

## Delivery boundary

WebKit's page notification signal does not cover persistent/service-worker delivery. The host therefore installs an outgoing GIO filter **before creating any webview**, blocking unattributed notification mutations on the process-shared session-bus connection. Consent-checked Rust delivery uses a separate authenticated connection. Other bus traffic and incoming signals are unaffected.

Persistent/service-worker OS notifications are deliberately suppressed even when page notifications are allowed: WebKit provides no consented-window identity for that path. Their browser methods still exist and can resolve; successful API completion is not an OS-delivery receipt. Normal worker execution and messaging remain available.

The filter covers freedesktop `Notify`/`CloseNotification` and portal `AddNotification`/`RemoveNotification`. The shared connection is retained for the app session with exit-on-close enabled; losing that bus exits the host rather than reconnecting without its filter. The canonical Hermes gateway is not stopped.

Approved delivery captures the unique sender of the successful `GetCapabilities` reply. `Notify`, click/close signals and later `CloseNotification` calls stay bound to that daemon instance, not its reusable service name. Reused numeric IDs after a daemon restart cannot redirect old callbacks or close another application's notification.

This relies on WebKit obtaining its notification proxy from the shared GIO session connection. The [WebKitGTK 2.54.2 backend source](https://github.com/WebKit/WebKit/blob/webkitgtk-2.54.2/Source/WebKit/UIProcess/Notifications/glib/NotificationService.cpp) contains that path for both freedesktop and portal delivery. The installed 2.54.2 library imports `g_dbus_proxy_new_for_bus_sync` and has no direct libnotify dependency. Engine upgrades must run the native regression; compilation alone cannot protect this boundary.

## Actual checks

`cargo test --locked --test window_contract -- --nocapture` runs WebKit against labelled HTTP fixtures on a private display, D-Bus session and OS home. The notification service is a test fixture, not the user's desktop daemon. The ignored Rust service-helper test is launched as a subprocess by the native test, not skipped acceptance coverage.

The full native scenario reports 14 checks, including:

- Real native consent, refusal, separate user activation for concurrent permission requests, and protection against a page replacing the permission-request method.
- Actual OS-protocol delivery of consented title/body text; escaped bodies for markup-capable services and literal bodies for plain-text services.
- OS click → browser callback, browser close → OS close, and stop/re-enable behavior without falsifying browser permission.
- Daemon replacement with actual ID reuse: old clicks and closes do not affect the replacement daemon's unrelated notifications, while new notifications still work. A valid subsequent signal/close is the processing barrier; this is not an absence check after a guessed sleep.
- A service-worker registration activated before Stop. Both page `registration.showNotification()` and worker `self.registration.showNotification()` resolve, the worker reports permission `granted`, and neither marker reaches the private OS service. Worker messaging still works.
- The existing native IPC, authenticated/blob download, popup, launcher-recovery and quit checks remain passing.

Twelve pure consent tests cover per-window scope, concurrent prompts, stale decisions, revocation, and grant generations. The daemon-replacement regression first failed with both a misrouted click and an unrelated close, then passed after unique-owner binding. Tests run fail-fast, not retry-until-green.

Denied and granted fixtures use different loopback sites (`127.0.0.1` and `127.0.0.2`), not just different ports: WebKit can reuse worker processes and permission caches by site. The worker must be activated before testing delivery; an unready or unpermissioned worker is not evidence that the OS gate works.

## Limits

This verifies Linux WebKitGTK 2.54.2 with GIO 2.90.1 and the freedesktop protocol fixture. It is not interactive verification of every desktop notification daemon, the portal backend, Flatpak packaging, macOS or Windows. It does not establish rich-action/icon/tag parity, a human security audit, or full Electron Desktop parity. See [compatibility](compatibility.md).

The freedesktop summary/title field is sent as plain text; only the body has a negotiated markup capability. A daemon that interprets the summary as markup despite that contract can display it differently. Native presentation on those daemons is not verified. Consent is top-level-window scoped, not separate per iframe: embedded cross-origin content shares that explicitly disclosed window grant.
