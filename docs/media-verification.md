# Linux microphone consent and verification

The host now handles the existing renderer's microphone requests through WebKit. It does not add a native recording API or change the plugin SDK.

## User controls and scope

- A workspace's first microphone request opens a native **Allow microphone access?** dialog. Capture waits for the decision.
- Consent covers content in that workspace, including embedded pages. The prompt names the top-level website, not an inferred requesting frame. Recording can continue in the background.
- WebKit owns its cached grant. This preserves upstream's permission probe, track stop, and separate recording request without a duplicate host prompt.
- **Close the workspace to end its microphone access** (Ctrl+W). A different window, including one reopened at the same website, needs its own consent. Closing a workspace does not stop the Hermes gateway.
- Refusing the dialog denies the request. A dialog left open across replacement of the top-level document cannot grant the replacement document access.
- Camera, combined camera/microphone and screen capture are not authorized by this microphone prompt. The bundled launcher cannot acquire microphone access.

There is deliberately no host “Stop and revoke” toggle. In WebKitGTK 2.54.2, stopping active capture clears grants, but the public setter returns early when capture is already idle. A real isolated probe also showed that disabling media streams does not stop a retained `getUserMedia` function from reacquiring an idle cached grant. Hiding an API is not revocation; closing the workspace is the tested boundary. Cookies and sign-in data are not cleared.

## Implementation

`src-tauri/src/gui/media.rs` installs native permission handling before untrusted navigation. Pending request objects remain on GTK's thread; only the dialog's Boolean result crosses a Tokio oneshot. A top-level document generation, current top-level origin and window-liveness check reject stale decisions. The requesting frame is not identified by the native API. There is no second grant registry, permission polyfill or hosted-page Tauri IPC.

The safe WebKit binding exposes audio/video request flags. Display requests carry no eligible audio input devices in the checked engine path, including display-with-audio requests, so they fail the microphone-only condition. The native regression exercises this distinction. Recheck it on engine upgrades; no unsafe binding was added to reach an unavailable Rust API.

Relevant engine source at the investigated revision:
- [UserMediaPermissionRequestManagerProxy](https://github.com/WebKit/WebKit/blob/webkitgtk-2.54.2/Source/WebKit/UIProcess/UserMediaPermissionRequestManagerProxy.cpp)
- [RealtimeMediaSourceCenter](https://github.com/WebKit/WebKit/blob/webkitgtk-2.54.2/Source/WebCore/platform/mediastream/RealtimeMediaSourceCenter.cpp)
- [WebKitUserMediaPermissionRequest](https://github.com/WebKit/WebKit/blob/webkitgtk-2.54.2/Source/WebKit/UIProcess/API/glib/WebKitUserMediaPermissionRequest.cpp)

## Native evidence

Run `cargo test --locked --test window_contract -- --nocapture` on Linux with the documented native-test prerequisites.

The new consent regression first failed because capture immediately returned `NotAllowedError` instead of waiting for a decision. With the handler installed, the expanded scenario passed **19 checks** on local WebKitGTK 2.54.2, retaining all previous notification/download/window checks. Media-specific assertions cover:

- Native consent before capture, followed by the upstream-style probe/stop/reacquire sequence with no duplicate prompt.
- Actual nonempty `MediaRecorder` output (WebM/Opus locally), the `Mock audio device 1` label, and ended tracks after recording.
- Camera-only, mixed camera/audio and display-with-audio refusal even after microphone consent.
- Launcher refusal. The launcher is made visible first because the engine can defer hidden-page media requests.
- A separate same-origin window requiring its own decision; a stale dialog after same-origin document replacement granting no access; explicit refusal reaching the page.
- Closing a workspace with a live mock track, observing its inspectable page disappear, and a fresh same-origin window requiring consent again. Another window's denial does not end the already-consented window's track.

The hidden Linux `--test-mock-media` option substitutes WebKit's synthetic devices only. It does **not** grant permission. The normal default uses system devices. The test runs the actual host on a private display/bus with disposable HOME/XDG and unreachable Pulse/PipeWire endpoints; it never starts Hermes or opens the user's microphone.

A separate rootless Ubuntu 24.04 run with WebKitGTK 2.52.6 also passed all 19 checks, encoding MP4/AAC through the existing MIME fallback. Before installing `gstreamer1.0-plugins-bad`, recording failed with `NotSupportedError` and no supported candidate MIME type. That package supplies the `voaacenc` encoder; `gstreamer1.0-plugins-good` supplies other required media components. Both are explicit Ubuntu prerequisites. Subsequent [CI verification](https://github.com/willbohn/hermes-desktop-rust/actions/runs/41046540862) passed the complete native scenario after separately fixing launcher webview keyboard focus.

## Limits

This verifies engine consent and capture/encoding with synthetic devices. It does not establish physical-device behavior, audio quality, provider transcription, wake-word accuracy, full live-voice acceptance, or macOS/Windows media support. Encoding depends on the system WebKit/GStreamer codecs. Native CI results apply only to the engine and commit reported by that run. Automated review is not a human privacy audit.
