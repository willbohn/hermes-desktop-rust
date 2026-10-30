# Architecture decision: retain the workspace, replace the host

Baseline: [`NousResearch/hermes-agent@f2cc013158ce4b89f50154c09486a885835133b9`](https://github.com/NousResearch/hermes-agent/tree/f2cc013158ce4b89f50154c09486a885835133b9), current `main` at the start of this work (2026-10-29). The upstream submodule is pinned; we do not silently follow `main`.

## Choice

Use Tauri 2 and system WebView2/WKWebView/WebKitGTK to host the upstream **Webapp Desktop renderer**, not the old dashboard or a terminal in a web page. Hermes already implements the browser Desktop adapter in `apps/desktop/src/lib/browser-desktop-bridge.ts`. Its RPC client, session authority, plugin loader and React UI stay upstream.

The Rust host owns the connection launcher, bounded CLI-client lifecycle, window chrome and navigation policy. It does not reimplement model selection, tools, approval rules, sessions, memory, terminal RPC, MCP, plugin installation or authentication.

## Why retain a web-based UI rather than rewrite every widget in Rust?

The objective is a better native desktop experience, not a particular percentage of Rust. After explicitly considering an all-native Rust renderer, the chosen boundary remains **Rust host + upstream web UI + existing Hermes agent**.

- Existing desktop plugins export ESM/React components and share the live `@hermes/plugin-sdk` and React bindings (`src/contrib/runtime-loader.ts`). A Rust widget renderer cannot consume that contract directly. Keeping plugin compatibility would require a webview anyway, plus a second UI/state integration to maintain.
- Rich streamed Markdown, code blocks, terminals, artifact HTML and interactive plugin panes already have implementations in the shared renderer. Rebuilding them would create a large regression surface for text input, selection, IME, accessibility, streaming and layout.
- System webviews remove the need for this host to ship Electron's own Chromium/Node runtime. They do **not** remove JavaScript layout/painting costs; Windows WebView2 is itself Chromium-based. Performance improvements still need measurements and targeted fixes.
- Native Rust remains appropriate for window/process lifecycle, bounded connection work and OS integration. Move additional expensive work there only when a measured bottleneck and a concrete consumer justify it; do not add a speculative native-service framework.
- Smaller host changes and separable renderer fixes are easier to review and keep aligned with upstream than a replacement UI toolkit. Upstream acceptance is still a maintainer decision, not guaranteed by the stack.

Revisit individual native widgets or a different rendering backend only if a representative benchmark or an unsolved webview limitation warrants it **and** extension compatibility, accessibility and feature parity can be demonstrated. This decision does not waive the native-parity gaps below.

```mermaid
flowchart LR
  L[Bundled connection launcher] -->|narrow local IPC| R[Rust host]
  R -->|bounded CLI, no shell| C[Installed Hermes CLI]
  C -->|upstream local control IPC| G[Canonical Hermes gateway]
  R -->|private one-use launch URL| W[Unprivileged system webview]
  W -->|upstream HTTP + WebSocket transport| G
  W --> P[Existing renderer + ESM plugin SDK]
  G --> A[Existing agent, tools, skills, MCP, memory]
  R -->|Linux app lifetime| D["Blank download-context owner; no native IPC"]
  W -->|original engine downloads| D
  D -->|session folder policy| R
```

**Trust boundary:** only the bundled launcher window can invoke native commands. Remote pages and same-origin workspace popouts get no Tauri capabilities. One-use launch tickets stay out of logs, native window titles and persisted settings. External websites cannot cause an OS-browser launch without native confirmation. Plugins still have upstream application authority: this does **not** turn trusted ESM plugins into a sandbox.

Connection windows are retired on dismissal and recreated through the guarded builder. GTK3 WebKit can leave an unhandled-key replay flag set when a web-process reply arrives after its window hides; reusing that view can discard the next real key. No dummy input, timed recovery or page keyboard suppression is used. Automatic connection runs only once; recovered forms use CLI defaults, not unsaved edits. A workspace's first completed load retires only the launcher handle that initiated it; popups and later reloads cannot dismiss a newer form.

On Linux, a first-created, hidden `about:blank` view owns the shared download callback for the app lifetime. Wry's context callback survives its originating view, while Tauri refuses it once that view's label is gone. The blank owner prevents that stale callback from cancelling transfers in surviving workspaces. It has no content or native IPC authority, denies navigation away from `about:blank`, receives media/notification denials, and does not count as a user window. All user views still install the same download policy. This does not change engine cookies, authentication or blob ownership.

## Why not port every Electron IPC handler now?

The upstream Electron host spans filesystem, SSH, cloud authentication partitions, installers, overlays, preview browser guests and OS integrations. Porting the whole bridge before a usable result creates a second large host implementation to keep in sync. Keeping an Electron subprocess to fake parity would defeat the purpose of this project.

The Webapp bridge is an existing upstream compatibility seam, not an invented universal adapter. It gets a native workspace running without forking the agent or plugin SDK. Small renderer fixes are separately reviewable patches, applied only to a disposable build workspace; the pinned submodule stays pristine.

## Consequences

This is an **experimental alternative host**, not yet a feature-complete replacement for Electron Desktop. The browser bridge intentionally refuses native-only operations. In particular, Electron's isolated embedded Chromium browser and its guest preload cannot be translated faithfully to a plain system webview. See [compatibility.md](compatibility.md).

Local launch attaches through `hermes webapp --no-open --skip-build`. The CLI owns gateway startup and authentication. Closing this app does not terminate the canonical gateway. Builds require an explicit user action; the host never silently enables Webapp, changes provider assignments, installs an agent or copies credentials.

A shell swap is not evidence of faster React rendering. Existing transcript scheduling/virtualization stays intact; renderer issues need renderer fixes and measurements. No automatic update feed, cloud service, telemetry or parallel extension framework is added.

## Linux notification boundary

```mermaid
flowchart LR
  P[Page Notification API] -->|engine signal| C[Native consent and current grant]
  C -->|approved text only| A[Separate GIO connection]
  A -->|unique daemon owner| OS[OS notification service]
  W[Unattributed engine and worker notifications] --> S[Shared GIO connection]
  S -->|notification mutations blocked| G[App-session outgoing guard]
```

The guard is installed before webviews; shared-bus loss fails closed. Every page still receives its native permission/delivery handlers, with explicit denial on the launcher. No workspace IPC or browser-permission polyfill is added. This is an engine-integration boundary, not a sandbox against other processes under the same OS user. [Native evidence and limitations](notification-verification.md) describe the dependency on WebKit's shared GIO backend.

## Linux microphone boundary

```mermaid
flowchart LR
  P[Page and embedded content] -->|WebKit media request| C[Native microphone-only consent]
  C -->|current document and window only| E[Engine-owned grant and capture]
  W[Close workspace] -->|destroy page and its grant| E
```

The existing browser media API stays intact. Rust checks the request and asks for native consent; WebKit retains the grant for subsequent capture in that workspace. Closing the workspace is the revocation boundary because an idle cached grant cannot be reliably reset through the available safe API. No recording IPC, second permission registry or codec shim is added. [Media verification](media-verification.md) separates synthetic engine coverage from physical audio and provider testing.

## Upstream route

Discuss a native-host migration with maintainers before proposing it as a replacement. Small accessibility fixes can be reviewed independently. This standalone repository preserves upstream attribution and avoids assuming that an independent product belongs in the core tree. No endorsement or likelihood-of-acceptance claim is made.
