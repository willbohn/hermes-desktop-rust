# Accessibility verification: pane-tab keyboard navigation and CJK tooltip wrapping

Scope: two focused, test-first renderer fixes in the upstream Desktop renderer
(`apps/desktop`, reused by the Rust host with this patch). No host/Rust files, no live
profile, no new UI. Patch: [`patches/desktop-accessibility.patch`](../patches/desktop-accessibility.patch).

Pinned upstream revision for all source references and the patch base:
`f2cc013158ce4b89f50154c09486a885835133b9`.

## Reported concerns and disposition

| Report | State at pinned source | Disposition |
|---|---|---|
| [#145836](https://github.com/NousResearch/hermes-agent/issues/145836) Workflow pane tabs are DIVs without role=tab | `role="tablist"`/`role="tab"`/`aria-selected` already exist on shell tabs, but tabs are **not keyboard-operable**: no `tabIndex` (not in the tab order), no Arrow/Home/End movement, no Enter/Space activation | **Still present (partial).** Implemented the missing keyboard half of the ARIA tabs pattern. |
| [#146215](https://github.com/NousResearch/hermes-agent/issues/146215) Memory Graph tooltip leaves continuous CJK on one oversized line | `wrapText()` in `apps/desktop/src/app/starmap/text.ts` splits on whitespace only and accepts the first word unconditionally via `!line`, so an unspaced CJK run is one word | **Still present.** Rewrote to break an oversized token at grapheme boundaries. |
| [#72844](https://github.com/NousResearch/hermes-agent/issues/72844) reduced-motion / GlyphSpinner | Closed; fixed upstream by compositor animation (PR #91473) | **Already fixed — not touched.** |

## What changed

- `apps/desktop/src/components/ui/pane-tab.tsx` — for a `role="tab"` `PaneTab`:
  roving `tabIndex` (selected tab is the single tab stop), Arrow/Home/End focus
  movement within the enclosing `[role="tablist"]` (wrap-around), Enter/Space
  activation via a new optional `onActivate`, and a `focus-visible` ring.
  Non-tab `PaneTab`s (page headers) are untouched.
- `apps/desktop/src/components/pane-shell/tree/renderer/tree-group.tsx` — the
  zone tab strip (horizontal + collapsed vertical rail) passes `onActivate` so
  Enter/Space runs the same activate/restore verb as the pointer gesture.
- `apps/desktop/src/components/pane-shell/tree/renderer/narrow-overlays.tsx` —
  the narrow zone-mate tabs pass `onActivate` (reveal the pane).
- `apps/desktop/src/app/starmap/text.ts` — `wrapText()` keeps whitespace word
  wrapping and additionally breaks a token wider than the budget at grapheme
  boundaries (`Intl.Segmenter`), so unspaced CJK wraps inside the title box with
  no grapheme split and no lost text.
- New regression tests: `apps/desktop/src/components/ui/pane-tab-keyboard.test.tsx`,
  `apps/desktop/src/app/starmap/text.test.ts`.

## Evidence

Test-first baseline at the pinned source before implementation (same two new
files): **8 failed, 4 passed** — every new keyboard assertion and both CJK
assertions RED, the pre-existing behaviors (word wrap, mixed content, empty
input, non-tab header) GREEN.

After implementation:

| Check | Command (from `apps/desktop`) | Result |
|---|---|---|
| Focused regression | `vitest run --project ui src/components/ui/pane-tab-keyboard.test.tsx src/app/starmap/text.test.ts` | 12/12 pass |
| Affected suites | `vitest run --project ui src/components/pane-shell src/components/ui src/app/starmap src/app/right-sidebar` | 120 files, 664/664 pass |
| Lint | `eslint <changed files>` | exit 0 |
| Types | `tsc -p . --noEmit` | exit 0 |
| Format | `prettier --check <changed files>` | all match (the patch reverts a pre-existing, unrelated Prettier deviation in `narrow-overlays.tsx` rather than bundling it) |

Note: the count above is executed behavior, not a code-metric claim.

## Behavior asserted by the new tests

- The strip is a `tablist`; the selected tab has `tabIndex=0`, the rest `-1`.
- `ArrowRight`/`ArrowLeft` (and `ArrowDown`/`ArrowUp`) move focus and wrap;
  `Home`/`End` jump to first/last.
- `Enter` and `Space` activate the **focused** tab (not the currently active
  one) through `onActivate`.
- A `PaneTab` without `role="tab"` stays unfocusable and ignores tab keys.
- `wrapText` wraps a 360-char continuous CJK label within a 368px budget across
  >1 line, reproducing the input exactly when joined.
- A single grapheme (ZWJ family emoji) wider than the budget stays whole on its
  own line — never split.

## Build and verify

```
node scripts/build-renderer.mjs --test
```

The script checks the gitlink pin, applies the patch to a disposable checkout,
runs the affected suites and uses upstream's builder to publish `.build/webapp-dist`.
The pinned submodule stays untouched. For an upstream contribution, apply the patch
to a separate development checkout and follow that repository's contribution rules.

## Limits (not claimed)

- No live Desktop launch, real assistive-technology, or screen-reader run — these
  are jsdom behavior tests over the exact upstream components.
- `aria-controls`/`role="tabpanel"` linkage was not added: pane ids are not
  guaranteed document-unique across zones, so a guessed link could point at the
  wrong panel. The roles/selection semantics that already existed are preserved.
- The `--project ui` suite was run for the affected directories, not the whole
  repository test tree.
