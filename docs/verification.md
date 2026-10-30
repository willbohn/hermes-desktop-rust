# Verification and limits

These results belong to the experimental Linux host and the pinned upstream source. They do not establish complete Electron feature parity or upstream acceptance.

## Actual Hermes execution

In a disposable OS HOME/XDG/Hermes home, the pinned Hermes runtime launched its canonical gateway and Webapp. The Rust/WebKit window rendered the real workspace, created a session and received a real reply from the already-downloaded local `nvidia/nemotron-3-nano-4b` model. A read-only query of that disposable state database returned the **assistant** row `RUST_WEBKIT_LIVE_OK`; the screenshot visibly shows the reply. This is not the user prompt or a synthesized API response.

The unmodified `@hermes/plugin-sdk` fixture registered a pane, used the host React/Button bindings, reacted to the active session, and changed Count 0 → Count 1 on a fresh click. The pane was checked visually. This proves that representative SDK path, not every third-party plugin or Electron-only bridge method.

![Actual isolated Rust/WebKit workspace](images/workspace.png)

A later live view displayed **Backend ready**, with no visible wake overlay. The [health investigation](webapp-health-verification.md) records why no speculative status patch was made.

## Focused local checks

- `cargo test --locked`: passed, including 30 local-launch behavior tests and the actual Linux native window check.
- `cargo clippy --locked --all-targets -- -D warnings` and `cargo fmt --all -- --check`: passed.
- `npm test`: launcher and inspector ownership/close regressions passed.
- `python3 -m unittest discover -s tests -p 'test_*.py' -v`: 3 isolation tests passed.
- Export patch: regression observed failing before the fix; 8 export/download tests in 2 files passed afterward. ESLint, project TypeScript and formatting checks passed.
- Accessibility patch: affected upstream suites and source checks are recorded in [a11y verification](a11y-verification.md).
- The [native window test](download-verification.md) checks real engine/permission behavior using labelled HTTP/blob fixtures. It is separate from the real Hermes inference above.
- Linux [notification checks](notification-verification.md) exercise native consent, concurrent requests, actual private-bus delivery, clicks/close, daemon replacement, revocation and both persistent entry points. The owner-replacement regression was observed failing before the fix; the native gate runs fail-fast, not retry-until-green.
- Linux [microphone checks](media-verification.md) exercise native consent, probe/reacquire, actual synthetic-device encoding, non-microphone/launcher refusal, per-window scope, stale decisions and close/reopen. The initial consent case was observed failing before the handler was added. No physical microphone or transcription provider was used.
- Inspector discovery retains one connection per page until teardown. Awaiting the transport close fixed an observed overlap race, but did not resolve Ubuntu's handoff timeout; a client close does not acknowledge server-side detach. Node regressions cover connection reuse and close completion. The actual native scenario also passes with a guard that rejects any repeated attachment (observed failing before ownership was fixed). No native assertions were skipped or retried.
- The combined patched renderer build and affected upstream suites passed via `node scripts/build-renderer.mjs --test`. Existing upstream ineffective-dynamic-import warnings remain.
- The optimized Linux host built with `cargo build --locked --release` and its actual binary successfully executed `--help`. The GUI evidence is from the debug host; this does not claim a signed/distributed release.
- Consent recovery, stale callback rejection, connection-launcher close/reopen, and explicit quit passed after observed regression failures. The final full Rust suite, Node/Python checks and release rebuild passed afterward.

## Microphone CI history and recovery regression

[Verify run 41046540862](https://github.com/willbohn/hermes-desktop-rust/actions/runs/41046540862) passed all four jobs at `a930f671ad68f7430fadec4852f1338ce0735abb`: Ubuntu native behavior, Windows/macOS host checks and the patched renderer suites/build. The native scenario contains 19 checks, including the microphone boundaries above. Local WebKitGTK 2.54.2 recorded WebM/Opus; a separate Ubuntu 24.04/WebKitGTK 2.52.6 run recorded MP4/AAC.

Earlier microphone-era CI runs failed before reaching capture: the reopened launcher had a focused DOM button and an active native window, but received no keyboard activation. Explicitly focusing the embedded webview produced the green run above, but [docs-only follow-up 41049956782](https://github.com/willbohn/hermes-desktop-rust/actions/runs/41049956782) reproduced the same failure. The focus change was withdrawn. Later GTK tracing showed the key reaching the focused webview without reaching DOM `keydown`; native focus was not the missing step. Ubuntu's recording codecs were diagnosed separately in an isolated container and added as explicit prerequisites.

The keyboard failure is now reproducible: a real modifier key's DOM handler delays its web-process reply while native Ctrl+W dismisses the launcher. GTK3 WebKit's two-pass propagation can leave `shouldForwardNextKeyEvent` set on that hidden view. On reopening, Return reaches GTK's post-default handler and DOM `keyup`, but not `keydown`. The relevant source is `webkitWebViewBaseKeyPressEvent` / `webkitWebViewBasePropagateKeyEvent` in [WebKit's GTK view implementation](https://github.com/WebKit/WebKit/blob/webkitglib/2.52/Source/WebKit/UIProcess/API/gtk/WebKitWebViewBase.cpp).

The regression failed on both WebKitGTK 2.54.2 and Ubuntu's 2.52.6 before the lifecycle change. Retiring/recreating the guarded launcher makes it pass without a retry, dummy key, scripted submission or production delay. A blank app-lifetime owner keeps the shared download policy valid after user views close. The expanded **22-check scenario passed on both engines**, including first-key recovery, workspace limits and authenticated downloads after the first workspace closes. These are local results, not a claim that every historical or current CI run passed; [the workflow](https://github.com/willbohn/hermes-desktop-rust/actions/workflows/verify.yml) records commit-specific CI. Temporary GTK tracing has been removed.

[Run 41080027563](https://github.com/willbohn/hermes-desktop-rust/actions/runs/41080027563) at `8cf21481fe03c90424b95315912adf64e3c90ef9` passed renderer, macOS and Windows jobs but failed Ubuntu's inspector discovery after the second fixture workspace loaded. Forcing a stale target snapshot reproduced the timeout: the helper kept querying a retired launcher on a closed transport. Pending/future calls now reject promptly; discovery skips only a disconnected target confirmed absent from a fresh list, without reconnecting or replaying input. Unrelated and still-live-target failures remain fatal.

Further Ubuntu validation exposed a distinct native-menu failure. A recycled GTK widget address had a previously visible menu cache entry but no menu on the new launcher; Ctrl+W reached DOM while native menu activation never ran. Muda 0.20.0 retains that address-keyed entry unless detached; Tauri 2.12.5 ignores the resulting `AlreadyInitialized` error. Windows now close through the normal request path, removing their Linux menu before destruction. The **23-check scenario passed locally on both engines**, including eight native launcher closes with recreation between closes. Menu traces were removed; the original missing-menu runs remain failures, not credited as successful trials. An additional X11 traversal race is handled as an invalidated read, never an absent consent dialog; see [native discovery guarantees](download-verification.md).

Failure screenshots come only from the private fixture display; they do not capture the user's desktop. The test's labelled HTTP fixtures and synthetic media remain distinct from the real Hermes runtime execution above.

Native input tracing also exposed a separate teardown defect in the test fixture: its replacement notification daemon could panic in GIO's name-loss callback after the private bus closed. The new no-panic assertion failed before that fixture fix. The harness now reaps the owned daemon before Quit closes the bus and reports success only after cleanup. This teardown fix is distinct from the launcher keyboard lifecycle change above.

## Not established

- Complete native bridge parity: see [compatibility](compatibility.md).
- Native macOS/Windows usability, accessibility, OAuth flows and OS permissions. CI compilation cannot prove these.
- Performance superiority, long-transcript benchmarks or universal plugin compatibility.
- Signed/notarized distribution, auto-update, a human security audit, or approval by Nous Research.

The source is AI-assisted. Independent automated review is not human review. Live user credentials were not copied into the runtime smoke profile; the separate investigative probe's isolation limitation is disclosed in the health record.
