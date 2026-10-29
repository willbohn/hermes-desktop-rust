# Webapp startup/health investigation

Baseline: upstream `f2cc013158ce4b89f50154c09486a885835133b9`.

**No health patch was justified.** A later read of the actual Rust/WebKit workspace showed **Backend ready** and the completed assistant reply. The screenshot showed no visible “Waking up” overlay. That text still existed in the DOM, demonstrating why text presence alone was not sufficient evidence of an obstructing overlay.

## What the source says

- `apps/desktop/src/lib/gateway-health-pill.ts` derives “Backend checking” from `runtimeReadinessDisplay(null)`: no authoritative readiness verdict yet. It is not a browser-bridge stub.
- The chat swap overlay reads `$gatewaySwapTarget` and `$hydrationSyncProfile`; activation paths in `store/profile.ts` clear the swap target in `finally` blocks.
- `interpretRuntimeReadiness` accepts a boolean from either runtime readiness or setup/provider configuration. `use-status-snapshot.ts` repeats readiness checks on its tick while no verdict exists, subject to the view's focus/visibility gate.

## Investigation and limits

An independent automated investigation used a standalone GTK/WebKit focus probe and an instrumented upstream Webapp boot **test harness**. These did not reproduce a stuck state. They do not prove that every live Tauri focus, reconnect or profile-switch path works; the boot harness is not a live authenticated gateway.

A Python readiness probe used synthetic authority/transport objects, but its profile resolution escaped its intended fixture home. Its returned configuration boolean is **not counted as isolated live-gateway evidence**. No configuration or credential values were reported. Do not repeat that probe without isolating OS HOME/XDG/profile-root paths before importing Hermes, not merely setting HERMES_HOME.

The later parent verification used the actual isolated workspace. It was unfocused but visible, displayed Backend ready, and had a durable assistant message containing exactly `RUST_WEBKIT_LIVE_OK`. A visual check confirmed the reply and SDK pane were usable. No status was hidden or forced to “ready.”

## Sibling-profile lead, not a fix

`gateway/session_readiness.py` rejects a profile resolving outside the actor/authority's home, while host-scoped routing elsewhere can use `AuthorityConnection._sibling_for`. A readiness request stamped with a sibling profile could therefore fail even when other operations route successfully. The investigation did not reproduce this as the original live symptom.

Any proposed fix needs a focused multi-profile reproduction and authorization review. Do not relax profile-bound ticket checks or replace missing readiness information with a successful status. There is no `desktop-webapp-health.patch`.
