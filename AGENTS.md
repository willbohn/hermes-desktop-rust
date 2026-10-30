# Hermes Desktop Rust

A Rust/Tauri host for the pinned upstream Hermes Webapp renderer. This is an experimental host, not a second agent or a feature-complete Electron replacement.

## Work here
- Native host/CLI: `src-tauri/src/`; behavioral tests: `src-tauri/tests/`.
- Bundled connection UI: `ui/`; `npm test` runs its Node tests.
- Upstream: `upstream/hermes-agent` gitlink. Do not edit it in place. Keep renderer fixes in reviewable `patches/*.patch`; `scripts/build-renderer.mjs` builds a disposable checkout.
- Setup: `README.md`. Scope: `docs/compatibility.md`. Contribution/AI rules: `CONTRIBUTING.md`. Security: `SECURITY.md`. Record user-visible changes in `CHANGELOG.md`.

## Checks (repository root)
- `cargo test --locked --no-default-features`: CLI/policy without GTK.
- `cargo fmt --all -- --check`; `cargo clippy --locked --all-targets -- -D warnings`.
- `cargo test --locked`: includes actual Linux WebKit behavior. Linux additionally needs Node 26, Xvfb, xdotool, D-Bus, and openbox (existing kwin_x11 also supported). The test owns a separate display, bus and home.
- `npm test`; `python3 -m unittest discover -s tests -p 'test_*.py' -v`.
- `node scripts/build-renderer.mjs --test`: affected upstream suites, then renderer build. `cargo build --locked` builds the host.

## Invariants
- The installed Hermes CLI owns gateway lifecycle. Closing/switching a viewer never stops the canonical owner. Isolate OS HOME/XDG and gateway locks in tests, not just HERMES_HOME.
- Only the bundled launcher receives native IPC. Hosted workspaces must not receive filesystem, process, shell or opener capabilities. Recheck caller window identity and origin in commands.
- Launch tickets stay inside Rust/the target webview, never launcher JS, titles, Debug, logs or screenshots. Never expose raw child output as errors.
- Every webview installs the download policy, including the launcher: WebKit context handlers are shared. Folder consent is native, per app session, and revocable for new transfers. Preserve engine authentication/blob transfers; never re-fetch them in another browser.
- Keep `app.windows` empty; only the guarded Rust builders create views. Revoke must invalidate pending picker callbacks as well as clear folder permission. The real native test uses a valid OS default download directory to catch permissive-context regressions.
- Retire views through `close()`, not direct `destroy()`: Linux detaches Muda menus before GTK can reuse a window address. This host has no close-veto listeners; adding one requires preserving the menu on cancellation.
- No retry-masked tests or source-string tests in place of behavior. CI compilation is not native macOS/Windows verification; automated review is not human review.
