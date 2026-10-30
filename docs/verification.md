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
- `npm test`: 2 launcher tests passed.
- `python3 -m unittest discover -s tests -p 'test_*.py' -v`: 3 isolation tests passed.
- Export patch: regression observed failing before the fix; 8 export/download tests in 2 files passed afterward. ESLint, project TypeScript and formatting checks passed.
- Accessibility patch: affected upstream suites and source checks are recorded in [a11y verification](a11y-verification.md).
- The [native window test](download-verification.md) checks real engine/permission behavior using labelled HTTP/blob fixtures. It is separate from the real Hermes inference above.
- Linux [notification checks](notification-verification.md) exercise native consent, concurrent requests, actual private-bus delivery, clicks/close, daemon replacement, revocation and both persistent entry points. The owner-replacement regression was observed failing before the fix; the native gate runs fail-fast, not retry-until-green.
- The combined patched renderer build and affected upstream suites passed via `node scripts/build-renderer.mjs --test`. Existing upstream ineffective-dynamic-import warnings remain.
- The optimized Linux host built with `cargo build --locked --release` and its actual binary successfully executed `--help`. The GUI evidence is from the debug host; this does not claim a signed/distributed release.
- Consent recovery, stale callback rejection, connection-launcher close/reopen, and explicit quit passed after observed regression failures. The final full Rust suite, Node/Python checks and release rebuild passed afterward.

## Not established

- Complete native bridge parity: see [compatibility](compatibility.md).
- Native macOS/Windows usability, accessibility, OAuth flows and OS permissions. CI compilation cannot prove these.
- Performance superiority, long-transcript benchmarks or universal plugin compatibility.
- Signed/notarized distribution, auto-update, a human security audit, or approval by Nous Research.

The source is AI-assisted. Independent automated review is not human review. Live user credentials were not copied into the runtime smoke profile; the separate investigative probe's isolation limitation is disclosed in the health record.
