# Frontend follow-ups — G.1 through G.6

Per `phases.md` §F8. Each sub-task is independently landable; pick any order. G.6 sequences after F7.

## G.1 — `TopicTree` recursive children

**Problem**: `TopicTree.tsx` only renders root topics (no `parent_id`). Nested topics live in the store but never reach the DOM.

**Files**:
- `web/src/components/TopicTree.tsx`
- `web/src/components/TopicNode.tsx`

**Approach**:
- `TopicTree` builds a `Map<parentId, Topic[]>` from the flat topic list and renders only root entries (`parent_id == null`).
- `TopicNode` recursively renders its own children by looking up `parent_id == node.id` in the map (pass the map down via a context, not via props, to avoid prop-drilling).

**TDD**:
1. **Red**: `TopicTree.test.tsx`: render a tree with `A` root, `B` child of A, `C` child of B; assert all three labels visible and `C` is nested under `B`.
2. **Green**: ship the recursive renderer.
3. **Verify**: existing topic e2e specs still pass; manual smoke shows nested topics in the Playwright trace.

**Commit**: `feat(topictree): render nested topic children recursively`.

## G.2 — `HandsQueue.tsx`

**Problem**: `HandsQueue.tsx` exists in `web/src/components/` but is not imported anywhere. Dead since phase 6.5.

**Decision**: **wire it in** to `HostSession` (the host-only view). Surfaces the raise-hand queue under the topic-tree column. If the user disagrees, fallback is to delete the file.

**Files**:
- `web/src/components/HandsQueue.tsx` (verify shape; minor cleanup if needed)
- `web/src/components/HostSession.tsx` (or wherever the host-only chrome lives — check imports of `RaiseHandButton` for the parallel guest-side surface)

**TDD**:
1. **Red**: `HandsQueue.test.tsx`: render with two raised hands; assert both visible in the order they were raised.
2. **Red**: `HostSession.test.tsx`: render in host mode; assert `<HandsQueue>` is rendered.
3. **Green**: import and render in the right slot.

**Commit**: `feat(raise-hand): wire HandsQueue into HostSession`.

## G.3 — Modal a11y for `CreateBoardDialog` + `RaiseHandButton` modal

**Problem**: `AddTopicModal` correctly uses `useModalFocus`, `role="dialog"`, `aria-modal`, Escape handler. Two other modal surfaces don't.

**Files**:
- `web/src/components/CreateBoardDialog.tsx`
- `web/src/components/RaiseHandButton.tsx` (the modal portion only)
- `web/src/components/useModalFocus.ts` (verify; no changes expected)

**Approach**: mirror `AddTopicModal`. Wrap the modal container in:

```tsx
<div role="dialog" aria-modal="true" aria-labelledby={titleId} {...modalFocusProps}>
  …
</div>
```

Where `modalFocusProps = useModalFocus({ onClose: handleClose })`. The hook handles Escape, focus trap, and initial-focus.

**TDD**:
1. **Red**: `CreateBoardDialog.test.tsx`: open dialog → press Escape → assert `onClose` called; press Tab repeatedly → focus stays inside.
2. **Red**: `RaiseHandButton.test.tsx`: same shape.
3. **Green**: apply the wrapper + props.

**Commit**: `feat(ui): modal a11y for CreateBoardDialog and RaiseHand modal`.

## G.4 — Raise-hand word-count regex parity

**Problem**: client and server independently validate the raise-hand topic's word count. Edge cases (NBSP ` `, multiple spaces, zero-width joiner `‍`) diverge.

**Files**:
- `web/src/lib/validation.ts` (new) — single source of truth for client.
- `server/src/proto.rs` — confirm the existing regex; document the canonical pattern in a doc comment.
- `web/src/lib/__tests__/validation.test.ts` (new)
- `server/src/proto.rs` — add a `#[cfg(test)] mod word_count_tests` if not already present.

**Canonical rule** (mirror the server's existing logic):

```
words = trim().split(/[\s ]+/).filter(s => s.length > 0)
require words.length >= 1 && words.length <= 10
```

Hyphenated words count as one. Zero-width joiner does not split. Treat NBSP as whitespace.

**TDD**:
1. **Red (web)**: edge-case suite — `"foo bar"` = 2; `"foo bar"` = 2; `"foo  bar"` = 2; `"foo‍bar"` = 1; `"  "` = 0; `"a b c d e f g h i j k"` = 11 (fails the validator).
2. **Red (server)**: identical cases in `proto.rs` tests.
3. **Green**: implement `web/src/lib/validation.ts::countTopicWords(input: string): number` and `proto.rs::count_topic_words`.
4. **Refactor**: replace inline word-count checks in raise-hand handlers with calls to the shared functions.

**Commit**: `feat(validation): word-count parity for raise-hand topic between client and server`.

## G.5 — `QuestionComposer` rollback on error

**Problem**: when the user submits a question and the server returns `Error{code:"rate_limit"}` or `Error{code:"muted"}`, the composer's input is already cleared. The user has to retype.

**Files**:
- `web/src/components/QuestionComposer.tsx`
- `web/src/store/questions.ts` (or wherever `submitQuestion` lives)

**Approach**:
- Keep draft text in local component state until the `Ack` matching this submission's `refId` arrives.
- On `Ack`: clear the input.
- On `Error{code:"rate_limit"|"muted"}` matching this `refId`: restore the input and surface a toast.

**TDD**:
1. **Red**: `QuestionComposer.test.tsx`:
   - submit → simulate `Ack` → input clears.
   - submit → simulate `Error{code:"rate_limit"}` → input restored + toast appears.
   - submit → simulate `Error{code:"muted"}` → input restored + toast appears.
2. **Green**: track the pending `refId` in local state, wire to the ws store's pending-acks map.

**Commit**: `feat(qa): preserve draft on rate-limit and muted errors`.

## G.6 — Dark-mode parity (CSS vars)

**Problem**: pen palette, pen text input background, ClickPing colour, Cursor colour are hard-coded for light theme. Dark mode looks wrong (low contrast on darker pen text, invisible cursor on dark background).

**Files**:
- `web/src/index.css` — add CSS custom properties for the palette, pen text background, ClickPing fill, and Cursor stroke. Default values in `:root`, override under `:root[data-theme="dark"]`.
- `web/src/components/PenToolPalette.tsx` — use `var(--pen-swatch-bg)`, `var(--pen-swatch-border)`.
- `web/src/components/PenTextLayer.tsx` — text input bg via `var(--pen-text-bg)` + text colour via `var(--pen-text-fg)`.
- `web/src/components/ClickPingLayer.tsx` — fill via `var(--click-ping-fill)`.
- `web/src/components/CursorLayer.tsx` — stroke via `var(--cursor-stroke)`.

**Sequencing**: G.6 lands **after** F7. The paired light/dark snapshot infra exists; this sub-task produces the first new paired baselines for the pen board.

**TDD**:
1. **Red (visual)**: paired snapshot of the pen board with three swatches selected, in light + dark. Fails before changes because dark mode produces the wrong colours.
2. **Green**: ship the CSS vars + per-component swaps.
3. **Verify**: `scripts/check-snapshot-pairs.sh` exits 0; both snapshots match their baseline.

**Commit**: `feat(theme): dark-mode parity for pen palette and overlay layers`.

## Cross-cutting

- All UI changes go through `frontend-design` skill for any non-trivial visual decision.
- Each sub-task gets its own commit; reviewers can land them independently.
- After G.6 (which depends on F7), kick off a paired-snapshot regen pass: `just snapshot-update` then verify the diff is exactly the intended additions.
