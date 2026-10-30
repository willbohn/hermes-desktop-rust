# Compatibility with upstream Hermes Desktop

Baseline: `f2cc013158ce4b89f50154c09486a885835133b9` (2026-10-29). **Reuse is not proof of complete parity.** This matrix separates shared implementation from verified execution. Native smoke results are recorded in verification.md when available.

## Shared, not reimplemented

| Area | Integration | Qualification |
|---|---|---|
| Sessions, streaming, tools and approvals | Existing gateway + Desktop React renderer | Real inference must be checked with an authenticated provider; no fake model output is acceptance evidence. |
| Profiles, projects, memory, skills, capabilities, MCP, cron | Existing backend and renderer | Backend owns state and policy. Existing authentication remains required. |
| Terminal, files and Git review | Upstream Webapp browser bridge and gateway RPC | Files and terminal are **on the gateway host**, not automatically the user's laptop when remote. |
| Desktop ESM plugins | Existing `@hermes/plugin-sdk`, React bindings and runtime loader | No new SDK. Plugins using native-only bridge methods remain limited by the Webapp host. |
| Agent plugins, custom tools and memory providers | Existing installed Hermes runtime | Not loaded or reimplemented in Rust. Upstream trust and approval policies remain authoritative. |
| Theme, session UI, command palette, transcript scheduling | Existing Desktop renderer | A system webview is not Chromium; web-platform and layout behavior require native tests. |
| Local authentication | Supported `hermes webapp` launch-ticket handoff | No auth-file copying; ticket is not surfaced to launcher JS. Webapp must already be enabled. |
| Remote authentication | Existing HTTPS Webapp sign-in | No SSH tunnels, implicit plaintext fallback, or custom OAuth implementation. |
| New workspace windows | Webview popup + upstream window/session handoff | Bounded window count; native handoff must be exercised per OS. |

## Native features implemented by this host

- Bounded, single-flight local CLI launch with actionable, credential-safe failure states.
- Native windows and menus; connection/reload recovery remains reachable when the renderer fails. Reopened-launcher keyboard activation has an unresolved intermittent Ubuntu CI failure; see [verification](verification.md).
- Window size/position restoration; browser drag-and-drop remains enabled.
- Native folder consent for downloads, retaining authenticated requests and blob exports; actual Linux engine/permission tests pass.
- Linux page/completion notifications with native per-window session consent, bounded text, click/close handling and revocation; no renderer or plugin SDK replacement. See [notification evidence](notification-verification.md).
- Linux microphone consent through the existing `getUserMedia`/`MediaRecorder` path, with workspace-lifetime access and camera/screen refusal. Synthetic capture/encoding and permission boundaries are tested; see [media evidence](media-verification.md).
- HTTPS/loopback address validation and origin-aware native chrome.
- A local launcher that follows system light/dark and reduced-motion preferences, uses real form labels, keyboard focus indicators and live progress/error announcements.

## Known gaps — do not call this full Desktop parity

| Feature | Current disposition |
|---|---|
| Electron embedded Chromium preview/browser + guest preload | Not ported. The Webapp's browser/preview behavior is the baseline. Chromium automation/CDP and authenticated preview partitions are not provided by this host. |
| Native SSH connection management and remote install | Not ported; use an already configured HTTPS Webapp. No silent retry loop or experimental SSH implementation. |
| Cloud-manager partitions and desktop-specific OAuth brokers | Upstream browser-compatible sign-in only; no compatibility promise for Electron-specific flows. |
| Global hotkeys, HUD/floating overlay, tray/minimize-to-tray, screenshots, OS-specific workspace transfer | Not ported. Keyboard shortcuts inside the app are not global shortcuts. |
| Physical audio, provider transcription, live-voice/wake-word acceptance | Linux engine consent and synthetic capture/encoding are tested, not physical audio or the full provider flow. System codecs and devices still need validation. |
| Windows/macOS media permissions and notifications | Native permission UX and codecs need per-platform verification. Do not infer support from Electron or CI compilation. |
| Persistent/service-worker OS notifications on Linux | Suppressed even when page notifications are allowed: the engine supplies no consented-window attribution. Service workers and their browser APIs remain available. |
| Local folders attached to a remote gateway | Not implemented as a folder-staging pipeline. A local path is not a remote attachment. Use explicit file uploads or an archive. |
| Auto-update, signed/notarized installers | Not implemented. Deliberate source/binary updates only; no unsigned update feed. |
| Windows/macOS releases | Source targets, not verified production support. A Linux launch is not evidence of these platforms working. |

The broader migration is **not complete** until native gaps are resolved or explicitly agreed with users and upstream maintainers. The experimental host is kept useful and small rather than hiding those gaps behind stubs.
