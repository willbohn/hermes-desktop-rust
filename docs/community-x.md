# Constructive feedback from X

Collected 2026-10-29 through X Search, then checked against the publicly retrievable post text and available replies. These are qualitative reports, **not a representative survey or independently reproduced bugs**. We do not infer a consensus from engagement counts. Search results alone were not used to declare fixes missing.

## Signals and migration decisions

| Signal | Public evidence | Decision / verification needed |
|---|---|---|
| A calmer, clearer workspace | @witcheer's community-feedback summary says users want “a clearer line between bots and sessions” and “a calmer screen.” It also reports improved stability and daily-driver use, and points to **existing** Simple mode, context meter and popout windows.[1] | Preserve those existing features rather than inventing competing controls. Keep the new connection launcher small. Treat bot/profile clarity as an information-architecture concern, not a reason to change backend identity. |
| Navigation must preserve the conversation | @otium33 reports slow bot switching and a conversation apparently missing after switching away and back.[2] | Exercise switching/resuming against the canonical session backend. Do not implement another session-pointer cache in Rust. This post does not establish data loss or identify the current root cause. |
| Sound controls must be discoverable | @rchitectopteryx reports a distracting end-of-turn chime despite notifications being off.[9] A reply by @YiJingMan quotes an **agent-generated** workaround, not a verified product specification.[3] @witcheer's reply points to a separate Sounds setting and a “None” option under review.[10] | Check current source before adding another control. The pinned source already exposes `muteSounds` in the settings manifest and notification settings; preserve and test it. Do not repeat the historical “no mute UI” claim as a current defect. |
| Bots and profiles need clearer terminology | @nolstate specifically asks for a clearer distinction between profiles and bots.[4] | Keep upstream's rule that a bot is a profile with a canonical chat. Improve discoverability/copy only with real workflow testing; do not introduce a new identity model. |
| Plugin/toolset loading must fail usefully | @zetaperu reports a 30-second `/api/tools/toolsets` timeout and plugin-management hangs on Windows 11 with v0.22.5, while CLI works.[5] | Preserve the existing plugin framework; show bounded connection recovery instead of hiding failure. Windows plugin loading remains an explicit platform regression scenario, not claimed fixed by Rust. |
| Provider routing must not create surprise spend | @gllm2311 reports attempting ChatGPT integration but being routed to Nous; a reply asks whether it connected to GPT first and rerouted.[6] | This is an unconfirmed routing report, not proof of its cause. The host must not change providers or silently fall back. Verify actual model/provider identity in authenticated end-to-end tests before claiming parity. |
| Browser location must be understandable | @ElDeFiante reports asking for the embedded browser but seeing server-browser tools used, plus navigation failures; the reply thread includes a maintainer asking which browser was meant.[7] | Keep client-vs-server capability boundaries explicit. Do not disguise the missing Electron embedded-browser implementation as full parity. |
| Focus beats feature accumulation | @sacheeperera praises the CLI but says Desktop overcomplicates the experience and tries to do too much.[8] | Reuse the working core and SDK. Avoid a new bot framework, account system, cloud service, or ornamental UI redesign. This is product feedback, not a benchmark. |

## Scope limits

Public extraction does not necessarily include every reply, edit or deleted post. The posts above are source-attributed reports as retrieved; no claim is made that all remain reproducible on the pinned commit. In particular, the sound-control discussion has subsequent context and a current-source check, so it is a **do-not-regress** item rather than a new fix here. The X sample did not establish detailed accessibility requirements; the concrete accessibility regressions come from the linked GitHub reports in [community-feedback.md](community-feedback.md).

## Sources

[1] https://x.com/witcheer/status/2110077323252806131
[2] https://x.com/otium33/status/2107790476845932590
[3] https://x.com/YiJingMan/status/2112286998379536701
[4] https://x.com/nolstate/status/2114522248472138200
[5] https://x.com/zetaperu/status/2115225029956821211
[6] https://x.com/gllm2311/status/2114365369439703205
[7] https://x.com/ElDeFiante/status/2115146965990130069
[8] https://x.com/sacheeperera/status/2114503874983887291
[9] https://x.com/rchitectopteryx/status/2112262905966006297
[10] https://x.com/witcheer/status/2112459236332761536
