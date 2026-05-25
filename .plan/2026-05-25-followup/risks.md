# Risks — updates and additions

## Status updates to risks from `../2026-05-24-amber-falcon/risks.md`

### R21. Unbounded `RoomActor` memory if reap timeout missing — **scheduled**

Phase F3 introduces `last_activity_at: AtomicI64` and a 60-second tick on `RoomRegistry::reap_idle`. Default idle threshold: 10 minutes with no clients. The reap loop returns the `Arc<Room>` so the caller can drop it cleanly. Mark R21 as scheduled for closure once F3 lands and the integration test `idle_room_is_reaped_and_rehydrates_on_reconnect` passes.

### R16. Anonymous + moderation bypass via cleared localStorage — **unchanged**

Still defended only by per-IP rate limit. No work in this follow-up changes that contract.

## New risks

### R26. Write loss between in-memory broadcast and DB commit

**What**: ws-side handler enqueues a `WriteOp` and broadcasts the new state to all clients **before** the writer batches it into a SQLite transaction. A crash in the window between enqueue and `commit()` loses the op even though clients already saw it.

**Likelihood**: low. The batch window is ≤ 4 ms; even a 100-op burst fits in one fsync. Crash within that window is rare.

**Impact**: medium. Restarted server hydrates from DB and is missing one or more ops the clients already accepted. Clients reconnect, get the older state, and their UI silently rolls back.

**Mitigation**:
- Keep batch windows tight (≤ 4 ms).
- F2's `Welcome` snapshot is the recovery point — if a client reconnects after a crash, they get the persisted state and their optimistic UI is reconciled.
- For the rare critical write (e.g. `KickGuest` already persisted via `Db::set_kicked` synchronously today), continue routing through the synchronous path until/unless we measure a real bottleneck.
- **Track**: log every shutdown-drain timeout in production logs.

### R27. `PenUndo` payload integrity

**What**: `PenUndo` reads `pen_actions.payload_json` to invert a previous mutation. If the original write committed but the `pen_actions` row landed in a different transaction (e.g. via a race), the inverse would apply against the wrong base.

**Likelihood**: low — `persistence.md` §3 prescribes that the data mutation + the `pen_actions` row + `payload_json` land in **the same transaction**.

**Impact**: high — a wrong undo could corrupt board state.

**Mitigation**:
- F1.7 sub-phase reviewer must confirm same-transaction invariant by reading the writer arm.
- Integration test `pen_undo_restores_before_json_for_text_upsert` exercises the happy path.
- Add a fuzz test (future, not in F0–F8 scope): random sequence of pen ops + random `PenUndo`s, assert final state matches a reference implementation.

### R28. Idle-room reaper races a concurrent reconnect

**What**: reaper takes the DashMap entry lock, sees `clients.is_empty()`, removes the entry; meanwhile, a client whose ws handshake started 50 ms earlier finishes auth and tries to insert itself into the room it thinks exists.

**Likelihood**: low — DashMap entry lock + `clients.is_empty()` check + remove all happen under the same lock guard.

**Impact**: low — worst case the late-arriving client gets a fresh `Room` instance via `get_or_create` and hydration kicks in. State survives because the previous instance's writes already committed to SQLite.

**Mitigation**:
- Reaper must call `clients.is_empty()` and `remove` **inside the same DashMap entry lock acquisition**.
- F3 test `reap_idle_does_not_race_concurrent_get_or_create` covers this.

### R29. Rate-limit integration test flakes on real-time clock

**What**: `ws_rate_limit.rs::cursor_messages_exceeding_rate_limit_are_dropped` is timing-sensitive. If the test reads `Instant::now()` against an unpaused tokio clock, it will flake on slow CI runners.

**Likelihood**: high if naively written.

**Impact**: low (test-only).

**Mitigation**:
- `RateLimiter::with_clock(impl Clock)` constructor with a fake clock injectable from tests.
- The F4 sub-phase that authors this test **owns** the clock-injection seam.

### R30. F1 ↔ F5 refactor merge cost

**What**: F1 modifies every intent handler in `ws.rs` to enqueue `WriteOp`s. F5 splits the same file into `intents/*` modules. Doing F5 before F1 means every F1 edit later touches dozens of new tiny files, doubling review surface.

**Likelihood**: certain if order is wrong.

**Impact**: medium (developer time, reviewer fatigue).

**Mitigation**:
- F5 sequenced after F1 in `index.md` and `phases.md`.
- F1 explicitly stays within the current `ws.rs` structure.

### R31. F6 React #185 recurrence

**What**: a future store-shape change (extra field added to `boards[]` elements that reads its identity from the upstream message) reintroduces the same infinite-render loop.

**Likelihood**: medium — store shape evolves freely today.

**Impact**: medium — production console error, dead UI on board-create flow.

**Mitigation**:
- Vitest regression added in F6 (`renderCount` ≤ N for create flow).
- Documenting the root cause in the fixing commit so future contributors don't blindly remove the guard.

### R32. Visual-regression baselines drift between machines

**What**: snapshots can subtly differ based on font hinting, GPU driver, scrollbar widths. Baselines committed on one machine fail on another.

**Likelihood**: medium.

**Impact**: low (CI noise).

**Mitigation**:
- Run baseline generation only inside the CI image (`mcr.microsoft.com/playwright`).
- Document in `testing.md`: "do not generate baselines on a developer laptop; let CI regen via `just snapshot-update` in a one-off workflow".
- Already mostly handled via the Playwright Docker images; reinforce in this plan.

### R33. `TEST_FIXED_NOW` accidentally enabled in production

**What**: the `TEST_FIXED_NOW` env var, if set in a Railway environment, freezes the server's clock and breaks every time-based check (rate limiting, moderation timestamps, idle reap).

**Likelihood**: very low.

**Impact**: high.

**Mitigation**:
- Read the env var only in `cfg(debug_assertions)` or guard via a compile-time feature flag (`#[cfg(feature = "test_clock")]`) — pick during F7 design.
- Railway envs do not set the variable; default behaviour is unchanged.
