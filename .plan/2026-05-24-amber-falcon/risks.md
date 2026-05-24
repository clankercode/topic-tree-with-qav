# Risks + opinionated callouts

What can go wrong, what we'll do about it, and (where applicable) where I disagree with the spec.

## R1. ExcalidrawDelta correctness under rapid edits

**Risk**: We debounce the host's Excalidraw `onChange` to 150ms and broadcast the whole `elements` array. If a guest is mid-receive when a newer broadcast arrives, the messages can interleave with `updateScene` in unpredictable ways for older deltas.

**Mitigation**:
- Include `sceneVersion` (monotonic counter) on every `ExcalidrawUpdate`. Guests track the highest version seen and ignore deltas with `sceneVersion` ≤ theirs (out-of-order ws is rare on a single TCP connection but possible if we ever fan-out across instances).
- Periodic "scene reset" message every 60s containing the canonical scene as a snapshot, in case any guest drifted.

**Open**: Even with this, two concurrent host edits *cannot* exist in our model — only one host per room. So we accept "last-write-wins" semantics. If we ever allow co-host editing, switch to Yjs.

## R2. Whiteboard backpressure under heavy strokes

**Risk**: Host on a poor connection draws fast; ws send buffer fills; latency to guests spikes; UI feels laggy.

**Mitigation**:
- Client-side: batch points per requestAnimationFrame, never more than one in-flight per board.
- Server: `broadcast` channel capacity 256; drop-oldest on lag; clients fetch snapshot on suspected gap.
- Test: e2e scenario that fires 60 strokes/s for 5s and asserts max guest latency < 500ms median.

## R3. Visual regression flakiness

**Risk**: Playwright `toHaveScreenshot` is notoriously flaky for any animated, fonts-loading-async, or time-displaying UI.

**Mitigation**:
- `prefers-reduced-motion` forced on test contexts.
- Fonts preloaded via `<link rel="preload">` and a "ready" attr set only after `document.fonts.ready`.
- Server respects `TEST_FIXED_NOW` env for deterministic timestamps.
- Per-test ability to mask volatile regions with a CSS hook (`[data-testid="hide-in-snapshots"]`).
- `maxDiffPixelRatio: 0.005` ≈ allows AA shift on text edges without flagging.

## R4. Railway volume = single instance

**Risk**: Stated above. Out of v1 scope; documenting the boxed-in path.

**Mitigation**:
- Spec doesn't require multi-instance.
- Upgrade path: swap rusqlite → tokio-postgres + a Redis pubsub for cross-instance broadcasts. ~2 weeks of focused work; not committed.

## R5. AdminToken-in-URL footgun

**Risk**: User shares the admin URL (`?admin=...`) thinking it's the join URL → guests inherit admin.

**Mitigation**:
- Client strips token from URL bar within 50ms of load via `history.replaceState`.
- Admin UI clearly labels which URL is which: a banner with two big copy-buttons, "Join link" and "Admin re-entry link" (the latter explicitly described as "for you only").
- We never *display* the admin URL once the token is in IDB; we display only the room link + a "Copy admin link" button which materializes it on demand.

## R6. Anonymity vs moderation tension

**Risk**: A spammer posts anonymous abuse; host wants to kick them but can't see who they are.

**Mitigation**:
- Server keeps the real `guest_id` on the question row but never returns it to clients (including host). Host's "delete this question" works without revealing identity.
- Host can mute/kick by *presence* (the user is known by display name in presence even if anonymous in Q&A). If they later post abuse anonymously, the kick still removes their session.
- A future v1.1 idea: "anon-but-traceable" — host has a `show_author` button protected by a one-time confirmation if the question crosses moderation thresholds. Not in v1.

## R7. Bundle size for Excalidraw

**Risk**: `@excalidraw/excalidraw` is large (~ MB). It can dominate first paint.

**Mitigation**:
- Lazy-load: `const ExcalidrawBoard = lazy(() => import('./ExcalidrawBoard'))`. Only loaded when a user focuses an Excalidraw board.
- Acceptable: an Excalidraw-using session pays the cost once; non-Excalidraw rooms never load it.

## R8. SQLite single-writer

**Risk**: With WAL we get concurrent reads but only one writer at a time. Heavy stroke-persistence + concurrent question writes could contend.

**Mitigation**:
- All DB writes funnel through a single tokio task with an mpsc queue (already in architecture). Throughput is bounded by SQLite write speed; 5k writes/s sustained on consumer disk is easy and far exceeds expected load.
- Persist strokes *after* broadcasting — broadcast latency is unaffected by DB lag.
- If contention shows up in profiling, batch stroke persistence per board (write one row per stroke at `PenStrokeEnd`, not per point batch).

## R9. Subagent prompt drift

**Risk**: Different per-task subagents interpret "follow the plan" inconsistently and produce inconsistent code style.

**Mitigation**:
- Maintain `CLAUDE.md` at the repo root summarizing: stack, conventions, where to find what, how to run tests, how to commit, how to invoke the review loop. Every subagent reads it.
- Standardized subagent prompt skeleton in `agents-workflow.md` §3.
- End-of-phase code review (Track B) catches stylistic drift.

## R10. Visual reviewers overstate severity

**Risk**: Both Opus and gpt-pro will sometimes flag taste preferences as issues; review-and-fix subagent dutifully implements them; we get worse over time.

**Mitigation**:
- Synthesize step demotes single-source items.
- Leader reviews the merged punch list before dispatching the fix subagent; can strike obvious taste items.
- "Pure taste" rejection criterion is explicit in the visual review prompt.

## R11. The full review loop is expensive

**Risk**: Four parallel reviewers per phase × 10 phases × multiple rounds is *a lot* of tokens.

**Mitigation**:
- Track A skipped on code-only phases.
- Single-reviewer fast loop for tiny diffs (under 200 LOC); full quad-review for substantial UI work.
- Visual review batched per phase, not per task.
- We're spending tokens on quality deliberately; the user has explicitly opted in.

## R12. Opinion calls — RESOLVED

All confirmed with user 2026-05-24:

- **Raise hand**: ADOPTED. Per-raise carries a 1-10 word topic string. Implemented in Phase 6.5 (`phases.md`). Ephemeral — not persisted. State + protocol in `protocol.md`.
- **Q&A ↔ topic linking**: DROPPED as free-form association. Replaced with a one-click **"promote question to topic"** button that creates a topic-tree node from the question and removes it from Q&A. Implemented in Phase 6.5.
- **Retention**: retain forever; only admin deletion. No retention scaffolding.
- **Multiple hosts**: NO — single host. SQLite single-writer remains a non-issue. Webserver is multi-threaded (Tokio worker pool) and serves both HTML + API (no separate Node sidecar) — already in the design.
- **Rust vs OCaml**: Rust confirmed at decision lock.

## R13. Excalidraw read-only enforcement

**Risk**: `viewModeEnabled` on Excalidraw is the JS toggle; a determined guest could re-enable it via devtools and send `ExcalidrawUpdate` to our ws. Our server *also* rejects non-admin `ExcalidrawUpdate`, so this is fine — but only because we have defense in depth. Don't drop the server check.

## R14. Light/dark parity drift

**Risk**: Designs evolve; the dark mode lags.

**Mitigation**:
- Every visual regression baseline has a paired dark-mode shot. Drift trips CI.
- Phase 8 explicitly closes parity before lock-in.
