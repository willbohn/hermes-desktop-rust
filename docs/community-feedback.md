# Community feedback: what changes here

Research date: 2026-10-29. Public issue reports are evidence of user experience, not automatic proof of current defects. We checked linked fixes and current source at the pinned upstream revision. This is a representative set, not a claim that every community concern has been solved.

Also see [constructive feedback from X](community-x.md): direct post excerpts, reply context, positive and critical feedback, and source checks to avoid repeating already-addressed complaints.

| Concern | Source and current disposition | This project |
|---|---|---|
| Sluggish typing and long transcripts | [#95033](https://github.com/NousResearch/hermes-agent/issues/95033), [#68467](https://github.com/NousResearch/hermes-agent/issues/68467): upstream fixes already merged | Retain upstream typing-burst scheduling and transcript mounting. Do not advertise them as new Rust fixes. |
| Idle CPU/GPU and streaming cost | [#127647](https://github.com/NousResearch/hermes-agent/issues/127647), [#145979](https://github.com/NousResearch/hermes-agent/issues/145979): open renderer/performance reports | No Electron runtime in the new host, but no claim that Rust fixes React cost. Native measurements and long-session regression work remain necessary. |
| Opaque startup failures | [#146002](https://github.com/NousResearch/hermes-agent/issues/146002): generic gateway deadline hides distinct failures | Bounded startup with immediate progress, explicit retry, sanitized error categories and recovery actions; no raw output/token dump. It does not claim to fix the upstream Electron diagnostic path. |
| Repeated connection storms | [#146148](https://github.com/NousResearch/hermes-agent/issues/146148): SSH retry/lockout report | Local launch is single-flight and does not auto-retry. SSH itself is not ported; no claim that upstream SSH is repaired. |
| Pane tabs lack keyboard/selection semantics | [#145836](https://github.com/NousResearch/hermes-agent/issues/145836) | Focused renderer patch with behavior tests, separately reviewable from the host. |
| Long CJK memory-graph tooltips | [#146215](https://github.com/NousResearch/hermes-agent/issues/146215) | Focused grapheme-safe renderer fix; preserve normal word wrapping. |
| Reduced motion / animation overhead | [#72844](https://github.com/NousResearch/hermes-agent/issues/72844): compositor fix already merged | Keep upstream fix. The new launcher has no polling and honors reduced motion. |
| Windows microphone startup stalls | [#146291](https://github.com/NousResearch/hermes-agent/issues/146291) | Not fixed by changing shell language. Native audio capture is a separate migration item requiring Windows measurements. |
| Lost remote session resume / duplicated optimistic message | [#139818](https://github.com/NousResearch/hermes-agent/issues/139818), [#145965](https://github.com/NousResearch/hermes-agent/issues/145965) | Reuse canonical backend session authority; these issues remain upstream work, not claimed resolved. |
| Local folders unavailable to remote backend | [#120449](https://github.com/NousResearch/hermes-agent/issues/120449) | Document the host boundary explicitly; no fake local-path attachment. A safe folder-staging pipeline is not yet implemented. |
| Plugin settings/install correctness | [#146031](https://github.com/NousResearch/hermes-agent/issues/146031), [#145835](https://github.com/NousResearch/hermes-agent/issues/145835), [#145911](https://github.com/NousResearch/hermes-agent/issues/145911) | Keep the existing SDK and installer rather than bypassing checks or creating another format. These reported defects are not fixed by this host. |
| Linux and Intel Mac distribution | [#145966](https://github.com/NousResearch/hermes-agent/issues/145966), upstream [BUILDING.md](../upstream/hermes-agent/apps/desktop/BUILDING.md) | Make Linux a real tested development target. Do not promise signed universal Mac/Windows releases based on Linux tests. |

## UX choices beyond the reported defects

- **Recover without deleting state:** the native Workspace menu remains reachable even if the renderer cannot load.
- **No surprise setup:** local connection never silently enables a listener, changes provider assignments or installs dependencies. Missing assets have an explicit build action.
- **Keep work alive:** close/reopen the window without killing the shared gateway.
- **Readable failures:** distinguish operation progress from failure; keep retry usable, avoid secret-bearing logs, and describe the next action.
- **Accessibility at the entry point:** semantic forms, keyboard focus, live announcements, system theme and reduced motion; no elaborate onboarding wizard.

Complete native parity, platform releases and upstream acceptance are open work, not implied by this document. See [compatibility.md](compatibility.md).
