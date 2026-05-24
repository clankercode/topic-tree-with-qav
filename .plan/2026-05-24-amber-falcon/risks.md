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

## R15. Host loses admin access if IndexedDB is cleared

**Risk**: Admin token only lives in the host's browser IndexedDB. Clearing site data, switching browsers, or losing the device = loss of admin access to all previously created rooms.

**Mitigation**:
- Show an explicit **"Save your admin link"** modal once on room creation, with copy button + email-to-self affordance. Frame as "the only way back in."
- The admin URL itself (`/r/:id?admin=<token>`) is the recovery key. Document this prominently in the docs site (Phase 9.5 host usage).
- Out of scope: server-side recovery via email/SMS — would require auth infrastructure we don't have.

## R16. `guestId` clearing bypasses vote dedup + mute + kick

**Risk**: Vote dedup, mute, kick all key off the client-side-generated `guestId` in localStorage. A determined guest can clear storage and re-join with a fresh id, evading mute/kick and double-voting.

**Mitigation**:
- This is an **accepted no-auth limitation** of the design. Documented.
- Defense: rate-limit per-IP at the ws layer (cheap; mitigates the bot case but not the human-with-incognito case).
- v1.x add (if abuse becomes real): server-issued per-room cookie containing a signed `participant_id`; binds across localStorage clears. Not implemented in v1.

## R17. SQLite WAL corruption on hard shutdown

**Risk**: Railway can SIGKILL containers during deploy or instance failure. WAL mode is robust against this in modern SQLite, but a corrupted `-wal` is possible in pathological cases.

**Mitigation**:
- `PRAGMA synchronous=NORMAL` (already in spec) — durable across power loss; not against media corruption.
- Periodic backup: `just db-dump RAILWAY` run from a cron (Railway's scheduler or external).
- On boot, if `app.db` fails integrity check (`PRAGMA integrity_check`), the server writes a `.broken-<ts>` snapshot aside and starts fresh. Logged loudly. Accept data loss for v1.

## R18. Railway WSS proxy idle timeout

**Risk**: Some PaaS proxies drop idle WebSocket connections after 60-120s. If clients aren't actively sending, the connection silently dies.

**Mitigation**:
- Server-side `Ping` every 25s (already in spec) keeps the proxy from idling. Verify in Phase 9 by leaving a session idle for 10min and confirming connection stays alive against the deployed URL.

## R19. `@excalidraw/excalidraw` API drift

**Risk**: Excalidraw moves quickly. Field names on `elements[]` and `appState` can rename between minor versions; `excalidrawAPI` exposure can shift.

**Mitigation**:
- Pin `@excalidraw/excalidraw` to an exact version (no `^`) in `web/package.json`.
- Phase 5 spike: write a `web/tests/excalidraw-api-shape.spec.ts` that asserts the API surface we use (`updateScene`, `getSceneCoordsFromViewport`, `viewModeEnabled`, collaborator pointer support). Bump-then-fix when we choose to upgrade.

## R20. Broadcast channel capacity exhaustion

**Risk**: `tokio::sync::broadcast` channel capacity 256 — if a snapshot burst arrives, slow clients can fall behind and lose messages.

**Mitigation**:
- Architectural choice: drop-oldest with a warning; client falls back to `GetSnapshot` after detecting a `seq` gap. Plan-spec already covers this.
- Phase 9 e2e: simulate a slow client by routing ws frames through a delay; confirm the recovery path triggers.

## R21. Unbounded RoomActor memory if reap timeout missing

**Risk**: A room with zero connections still consuming memory forever.

**Mitigation**:
- Reap idle rooms after **10 minutes** with no active connections AND no inbound messages. Implemented in Phase 1 (added to acceptance criteria).
- Memory per idle RoomActor is small (~KB) but compounded across thousands of historical rooms it adds up.

## R22. Light/dark parity drift

**Risk**: Designs evolve; the dark mode lags.

**Mitigation**:
- Every visual regression baseline has a paired dark-mode shot. CI runs `scripts/check-snapshot-pairs.sh` and fails on any baseline missing a partner. Phase 8 explicitly closes parity before lock-in.
