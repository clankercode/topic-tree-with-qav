# Phases F0–F8 — TDD task lists

Anchor: `c14487c`. Cumulative diff range when this plan completes: `2efafbb..HEAD-after-F8`.

Every phase follows red → green → refactor → commit. Each task names the failing test that drives the production code.

---

## Phase F0 — integration harness skeleton (S)

**Goal**: one end-to-end ws test (`tokio-tungstenite` client → real Axum app → assert `Welcome`) *before* persistence so F4 has a tested harness pattern to extend.

**Files**:
- `server/tests/common/mod.rs` (new) — shared harness helper.
- `server/tests/ws_smoke.rs` (new) — single ws smoke test.

**TDD tasks**:

1. **Red**: `ws_smoke::client_receives_welcome_after_hello` fails because the harness doesn't exist.
2. **Green**: write `common::TestApp::spawn()` that:
   - Builds an `AppState` with `Db::open_in_memory()`.
   - Binds a `tokio::net::TcpListener` to `127.0.0.1:0`, captures the resolved `SocketAddr`.
   - Spawns the axum server on the listener; returns `(TestApp { addr, state, server_handle, ws_url })`.
   - Exposes `TestApp::create_room()` → `(room_id, admin_token)` via the HTTP API.
   - Exposes `TestApp::connect_ws(role, room_id, token_or_guest_id)` → `WsClient` wrapping `tokio-tungstenite`.
3. **Refactor**: extract `WsClient::send_json` / `recv_json` helpers (no behaviour change).
4. **Commit**: `feat(test): add ws integration harness skeleton`.

**Verify**: `cargo test --test ws_smoke -- --nocapture` green; `cargo test --test http_smoke` still green; `just lint` clean.

**Notes**: do not hard-code ports. The harness is the foundation for F1's writer-persistence test and F4's nine named tests.

**Dispatch**: single implementer subagent.

---

## Phase F1 — persistence write path (L)

> See `persistence.md` for the canonical `WriteOpKind` enum, batching policy, writer-connection ownership decision, V0005/V0006 schema deltas, hydration query, and shutdown handling.

**Pre-work — design lock**: dispatch `ccc-cx-investigator` with the explicit prompt covering (a) the full `WriteOp` enum, (b) writer connection ownership in light of the `:memory:` test mode constraint from `data-model.md` §3, and (c) whether `Db` needs to retain the database path or expose a `clone_for_writer()` connection getter. Pair with a Claude subagent code-reviewer for a second opinion. Land the decision in `persistence.md` §4 before any code is written.

**Files**:
- `server/migrations/V0005__excalidraw_scenes.sql` (new) — see `persistence.md` §5.
- `server/migrations/V0006__pen_action_payloads.sql` (new) — see `persistence.md` §5.
- `server/src/writer.rs` (new) — writer task + batching loop + `WriteOp` apply functions.
- `server/src/db.rs` — `WriteOpKind` is declared here; `clone_for_writer()` getter and the `RoomHydrationBundle` reader (the F2 query stub lands here so F2 only adds the call site).
- `server/src/state.rs` — holds `pub writer_tx: WriteSender`.
- `server/src/room.rs` — every mutator either returns the op for the caller or persists itself via the writer; in-memory model unchanged.
- `server/src/ws.rs` — every state-changing intent enqueues; refactored further in F5.
- `server/src/main.rs` — spawn the writer task at boot; drain on shutdown.

**TDD tasks** (one commit per group):

### F1.0 — schema migrations

1. Add `V0005__excalidraw_scenes.sql` + `V0006__pen_action_payloads.sql`.
2. **Red**: write `db::tests::migrations_create_v5_and_v6_tables_and_columns` — assert `excalidraw_scenes` exists with the four columns, and `pen_actions` has a `payload_json` column.
3. **Green**: ship the SQL.
4. **Refactor**: no.
5. **Commit**: `feat(db): add V0005 excalidraw_scenes + V0006 pen_actions.payload_json`.

### F1.1 — writer-connection ownership

1. **Red**: `db::tests::clone_for_writer_in_memory_shares_connection` — open `:memory:` `Db`, call `clone_for_writer()`, write a row through the writer-handle, observe the row via the read pool.
2. **Green**: implement `Db::clone_for_writer()`.
3. **Refactor**: collapse `set_kicked` / `set_muted` paths to go through the same connection if convenient (do not regress the existing isolation tests).
4. **Commit**: `feat(db): add clone_for_writer for single-writer task`.

### F1.2 — `WriteOp` envelope + writer skeleton

1. **Red**: `writer::tests::writer_drains_and_commits_batch` — send three `UpsertTopic`s, then close `tx`, await the writer; assert all three rows present.
2. **Green**: define `pub struct WriteOp { room_id, kind: WriteOpKind }`, add `WriteOpKind::UpsertTopic` only, spawn the writer in a test helper, implement drain-then-commit loop.
3. **Refactor**: extract `apply_kind(&Transaction, &WriteOpKind)`.
4. **Commit**: `feat(writer): single-writer task + WriteOp envelope`.

### F1.3 — topics intents

1. **Red**: per intent (`UpsertTopic`, `RenameTopic`, `MoveTopic`, `SetTopicStatus`, `DeleteTopic`, `SetActiveTopic`), unit tests on `apply_kind`.
2. **Red**: ws-level harness test `topics_persist_after_round_trip` — submit `AddTopic` over ws → wait `timeout(2s)` for the row to appear → assert.
3. **Green**: wire the writer arms + the ws handler enqueues.
4. **Refactor**: extract `WriteOp::topic_*` constructors if the call-sites are repetitive.
5. **Commit**: `feat(writer): persist topic mutations`.

### F1.4 — questions + votes intents

1. **Red**: per intent (`UpsertQuestion`, `SetQuestionAnswered`, `DeleteQuestion`, `AddVote`, `RemoveVote`).
2. **Red**: `submit_question_persists` ws-level test.
3. **Green**: wire writer arms + handlers.
4. **Commit**: `feat(writer): persist question + vote mutations`.

### F1.5 — atomic PromoteQuestionToTopic

1. **Red**: `apply_kind_promote_question_to_topic_is_atomic` — feed a kind that inserts a topic + deletes the question, simulate failure mid-tx (use a poisoned `Transaction` via `rusqlite::Error::ExecuteReturnedResults`); assert the original question row is still present, no orphan topic row.
2. **Green**: writer arm wraps both rows in a single `Transaction`.
3. **Commit**: `feat(writer): atomic promote_question_to_topic`.

### F1.6 — boards intents

1. **Red**: per intent (`UpsertBoard`, `RenameBoard`, `DeleteBoard`, `SetFocusedBoard`).
2. **Red**: ws-level `add_pen_board_persists`.
3. **Green**: writer arms + handlers.
4. **Commit**: `feat(writer): persist board mutations`.

### F1.7 — pen intents (write-on-stroke-end)

1. **Red**: `insert_completed_pen_stroke_writes_stroke_and_action` — feed an `InsertCompletedPenStroke`, assert one `pen_strokes` row + one `pen_actions` row (`kind="stroke_add"`, `payload_json=NULL`).
2. **Red**: `pen_undo_restores_before_json_for_text_upsert` — pre-seed an action with `payload_json` holding the previous text, feed `PenUndo`, assert the text row reverts and the action row is deleted.
3. **Red**: `pen_undo_restores_cleared_strokes_and_texts` — pre-seed a `clear` action with `payload_json={strokes, texts}`, feed `PenUndo`, assert rows restored.
4. **Green**: writer arms for `InsertCompletedPenStroke`, `UpsertPenText`, `DeletePenText`, `PenClear`, `PenUndo`.
5. **Refactor**: extract `apply_pen_inverse` helper for `PenUndo`.
6. **Commit**: `feat(writer): persist pen mutations with durable undo`.

### F1.8 — excalidraw intent

1. **Red**: `upsert_excalidraw_scene_writes_row`.
2. **Green**: writer arm.
3. **Commit**: `feat(writer): persist excalidraw scenes`.

### F1.9 — moderation intents

1. **Red**: `set_kicked_does_not_clobber_muted` (replicates `db::tests:243` semantics).
2. **Red**: `set_muted_does_not_clobber_kicked`.
3. **Green**: writer arms for `SetKicked`, `SetMuted`. **Do not** collapse into a single `UpsertModeration`.
4. **Commit**: `feat(writer): persist moderation flags via SetKicked + SetMuted`.

### F1.10 — shutdown drain

1. **Red**: `writer_drains_on_close_within_timeout` — push 100 ops, close `tx`, assert join completes within 1 s and all 100 rows are in the DB.
2. **Green**: graceful shutdown in `main.rs`: close `tx`, join writer with bounded timeout.
3. **Commit**: `feat(server): drain writer on shutdown`.

**Phase-local DoD**:

- All unit tests in `writer.rs` green.
- F0 ws-smoke harness + the test-only DB-read helpers (`read_questions_for_test` / `read_topics_for_test` / `read_boards_for_test`) confirm that issuing each intent over ws results in the row appearing within `tokio::time::timeout(2s)`.
- The restart-and-rehydrate end-to-end test belongs to F2; F1 ships without it.

**Risks** — see `risks.md` R26, R27.

**Dispatch**: `superpowers:subagent-driven-development` driving an implementer + spec-reviewer + code-quality reviewer per sub-phase. F1.3, F1.4, F1.6, F1.8, F1.9 can run in parallel after F1.0–F1.2 land (they touch disjoint files in `room.rs`/`ws.rs`).

---

## Phase F2 — rehydration on `get_or_create` (M)

**Goal**: on DashMap miss, materialise a `RoomHydrationBundle` via one read tx and feed it to the existing `load_*` setters on `Room`.

**Files**:
- `server/src/room.rs` — rewrite `RoomRegistry::get_or_create`; ensure DashMap entry write-lock is held across the load.
- `server/src/db.rs` — `pub fn load_full_room_state(conn, room_id) -> RoomHydrationBundle` (stub landed in F1.2; fill it in here).
- `server/src/room.rs` — add `load_excalidraw_scenes` setter on `Room` (if not already present after F1).

**TDD tasks**:

1. **Red**: `room::tests::get_or_create_hydrates_from_db` — seed the DB directly with a topic + question, drop the registry, call `get_or_create`, assert the room exposes both.
2. **Green**: implement `load_full_room_state` (all six queries from `persistence.md` §7) + wire into `get_or_create`.
3. **Red**: `room::tests::get_or_create_holds_lock_across_load` — call `get_or_create` from two tasks concurrently with `tokio::join!`; assert exactly one DB read transaction was issued (use a counter wrapped around the db handle).
4. **Green**: hold the DashMap entry write-lock across the hydration call.
5. **End-to-end** (replaces the F1.10 placeholder): `restart_and_reconnect_preserves_question` — submit a question over ws via `TestApp`, await persistence, drop `AppState`, build a new `AppState` on the same DB file, connect, assert the question is in the new `Welcome`.
6. **Refactor**: wrap the hydration in `tracing::info_span!("hydrate", room_id)`.
7. **Commit**: `feat(server): rehydrate room state on first access`.

**Verify**: existing unit tests (`load_topics_replaces_existing` etc.) keep passing; new end-to-end test green.

**Notes**: < 500 KB / room — defer hydration-cost optimisation.

**Dispatch**: single implementer + reviewer pair.

---

## Phase F3 — idle reaper + `last_activity_at` (S)

**Goal**: rooms with no clients and no activity for >10 minutes are dropped from the registry.

**Files**:
- `server/src/room.rs`:
  - Add `last_activity_at: AtomicI64` to `Room`.
  - `Room::touch(now_ms)` updates the atomic on every ws message in/out + on `get_or_create`.
  - `RoomRegistry::reap_idle(now_ms, idle_threshold_ms) -> Vec<Arc<Room>>` walks the DashMap, removes entries where `clients.is_empty() && now_ms - last_activity_at > idle_threshold_ms`, returns the removed handles for graceful shutdown.
- `server/src/main.rs` — spawn `tokio::time::interval(Duration::from_secs(60))` next to the existing scene-reset task; call `reap_idle(now, 10 * 60 * 1000)` each tick.

**TDD tasks**:

1. **Red**: `room::tests::reap_idle_drops_truly_idle_rooms` — three rooms: A idle 11 min no clients (reaped), B idle 5 min no clients (kept), C idle 11 min with one client (kept).
2. **Green**: implement `Room::touch`, `RoomRegistry::reap_idle`.
3. **Red**: `room::tests::touch_updates_last_activity_under_lock_free_contention` — spawn 10 tasks calling `touch` in a loop; assert the final value is monotonically the latest.
4. **Green**: confirm `AtomicI64::store(now, Ordering::Relaxed)` is enough (it is — monotonic-of-monotonic).
5. **End-to-end**: `idle_room_is_reaped_and_rehydrates_on_reconnect` — connect, submit a topic, disconnect, force-reap via a test-only `AppState::__force_reap`, reconnect, assert the topic is in the new `Welcome` (this proves F2 hydration + F3 reaping work together).
6. **Commit**: `feat(server): idle-room reaper + last_activity_at`.

**Verify**: existing tests green; new tests green.

**Risks** — see `risks.md` R21 update + R28.

---

## Phase F4 — backend integration test suite (L)

**Goal**: implement the 9 named tests from `testing.md` §3, all on top of the F0 harness.

**Files** (one test per area-file):

- `server/tests/ws_room_lifecycle.rs`:
  - `create_room_returns_admin_token_and_room_id`
  - `hello_with_invalid_admin_token_returns_error`
- `server/tests/ws_questions.rs`:
  - `submit_question_broadcasts_to_all_clients_in_room`
  - `vote_question_dedups_by_guest_id`
- `server/tests/ws_topics.rs`:
  - `set_active_topic_marks_previous_active_as_done`
- `server/tests/ws_pen.rs`:
  - `pen_stroke_lifecycle_persists_and_replays_on_reconnect`
- `server/tests/ws_excalidraw.rs`:
  - `excalidraw_update_from_guest_is_rejected_when_view_mode`
- `server/tests/ws_rate_limit.rs`:
  - `cursor_messages_exceeding_rate_limit_are_dropped`
- `server/tests/ws_moderation.rs`:
  - `kicked_guest_cannot_reconnect_until_room_unblocks`

Each test follows: spawn harness → connect required clients → send message → assert broadcast or rejection → drop.

**Rate-limit test caveat**: the existing `rate_limit.rs` uses `Instant::now()`. If clock injection isn't already wired, add a `RateLimiter::with_clock(impl Clock)` constructor and use `tokio::time::pause` + `advance` in the test. Alternative: add a test-only env knob `TT_RATE_TEST_FAST=1` that shortens the window — but clock injection is cleaner; prefer that.

**Verify**:

- `cargo test --tests` green (existing + 9 new).
- `just test-server` green.
- Add a `just test-server-integration` recipe if not already present that runs `cargo test --tests`.

**Dispatch**: `superpowers:dispatching-parallel-agents` — the 9 tests touch independent areas. Dispatch in two batches:

- **Batch 1** (5 tests, low-risk): `ws_room_lifecycle.rs` + `ws_questions.rs` + `ws_topics.rs`.
- **Batch 2** (4 tests, higher-risk): `ws_pen.rs` + `ws_excalidraw.rs` + `ws_rate_limit.rs` + `ws_moderation.rs`.

**Risks** — see `risks.md` R29 (rate-limit flake).

---

## Phase F5 — `ws.rs` refactor into intents (M)

> See `ws-refactor.md` for the full target module layout and helper signatures.

**Goal**: split the 2,375-LOC `ws.rs` into a thin lifecycle/match layer plus `intents/*` modules. **No behavioural change**.

**Dependency**: lands **after** F1. F1 already wires writer enqueues into every intent; refactoring the file first would double the merge cost.

**Files**:
- `server/src/ws/mod.rs` — replaces `server/src/ws.rs` (connection lifecycle + match dispatch). Target ~600 LOC.
- `server/src/ws/helpers.rs` — `ensure_host`, `ensure_not_muted`, `ack_if_id`.
- `server/src/intents/mod.rs` (new directory) — re-exports.
- `server/src/intents/topics.rs`, `questions.rs`, `pen.rs`, `excalidraw.rs`, `moderation.rs`, `raise_hand.rs`, `presence.rs` — each exposes `pub async fn handle(ctx, intent) -> Result<()>`.

**TDD tasks**:

1. F4 integration suite is the regression net. Run `just test-server` before starting; capture the green baseline.
2. **Refactor step 1**: extract `ensure_host` / `ensure_not_muted` / `ack_if_id` helpers; tests still green.
3. **Refactor step 2**: extract one area at a time (topics first), move tests if any per-area unit tests exist, re-run `just test-server`.
4. **Refactor step 3**: repeat for questions, pen, excalidraw, moderation, raise_hand, presence.
5. **Refactor step 4**: shrink `ws.rs` to lifecycle + match.
6. **Commit per area**: `refactor(ws): extract <area> intent handler`.

**Verify**: `just test-server` green after each commit; LOC math in `ws-refactor.md` matches the realized split (rough); reviewer scope is "structural diff only".

**Dispatch**: `ccc-coder-cx` to drive the mechanical split, `ccc-review-cx` to verify semantics are preserved across each commit boundary.

---

## Phase F6 — whiteboard React #185 (M, parallel from F1)

**Goal**: `e2e/tests/whiteboard.spec.ts` passes. Currently React error #185 (max-update-depth) fires ~90 ms after clicking "Create" in `CreateBoardDialog`, leaving the page blank.

**Files** (suspected): `web/src/components/BoardPanel.tsx`, `PenBoard.tsx`, `CreateBoardDialog.tsx`, possibly `web/src/store/*` selectors.

**Hypothesis ladder** (record live in this file as we work):

1. `BoardPanel` `useEffect` reacting to a `boards[]` array whose identity changes on every store update.
2. `PenBoard` mount setter triggers a parent re-render that re-mounts it.
3. `CreateBoardDialog` close-side-effect → focused-board-set → dialog re-renders → close again loop.

**TDD tasks**:

1. **Red**: `pnpm -C e2e exec playwright test whiteboard.spec.ts` — fails today; capture the console error and the call stack of #185.
2. **Investigate**: dispatch `investigate-and-fix` (codex-led disciplined hypothesis loop). The skill itself handles red/green/refactor per hypothesis.
3. **Red, focused**: add a vitest regression in `web/src/components/__tests__/<component>.test.tsx` that fails today for the same root cause (probably a `renderCount` assertion or a `useEffect` infinite-loop detector via `@testing-library`).
4. **Green**: ship the fix.
5. **Verify**: whiteboard.spec.ts both tests pass; vitest regression green; no new React warnings in dev console.
6. **Commit**: `fix(whiteboard): break #185 update-depth loop on board create`.

**Dispatch**: `investigate-and-fix`.

---

## Phase F7 — visual-regression infrastructure (M, parallel)

> See `testing.md` §F7 for the contract.

**Goal**: paired light/dark snapshot infrastructure as described in `../2026-05-24-amber-falcon/testing.md` §5. Cleans up the 7 unpaired PNGs under `e2e/screenshots/_docs/` that currently fail `scripts/check-snapshot-pairs.sh` in CI.

**Files**:
- `server/src/api.rs` — extend `pub(crate) fn now_ms()` (line 112) to honour a `TEST_FIXED_NOW` env var read once at startup via `OnceLock`. Project-wide clock helper.
- `web/src/App.tsx` — render `<div data-testid="app-ready" />` once the initial `Welcome` snapshot has been applied.
- `web/src/index.css` — `.hide-in-snapshots { … }` rule; flagged elements (toasts, cursors, presence indicator) get the `data-testid="hide-in-snapshots"`.
- `e2e/utils/snapshot.ts` (new) — `awaitAppReady(page)` + `expectThemedScreenshot(page, name)` covering light + dark per call.
- `e2e/playwright.config.ts` — projects for `chromium-light` and `chromium-dark`, each setting the matching localStorage theme before tests.
- `e2e/tests/docs-screenshots.spec.ts` and `e2e/screenshots/_docs/` — rename existing screenshots to `<step>-light.png` / `<step>-dark.png` pairs (preferred — gives docs a dark-mode story) OR move them out of `e2e/screenshots/` to `e2e/.docs-screenshots/`. Choose rename; record the decision below.

**Decision**: **rename** to pairs. Docs benefit from a dark-mode story; reviewer feedback agrees.

**TDD tasks**:

1. **Red**: `scripts/check-snapshot-pairs.sh` fails today (7 unpaired PNGs in `_docs/`).
2. **Red**: write a tiny vitest unit test for `awaitAppReady` against a mocked DOM — fails until the util exists.
3. **Green**: ship `now_ms` env override + `app-ready` testid + `.hide-in-snapshots` CSS + `e2e/utils/snapshot.ts` + playwright projects.
4. **Green**: rerun `docs-screenshots.spec.ts` against the new infra; capture the renamed pairs.
5. **Verify**: `scripts/check-snapshot-pairs.sh` exits 0.
6. **Verify**: at least one **new** test spec produces a `<step>-light.png` + `<step>-dark.png` pair to prove the infra works end-to-end.
7. **Commit (per step)**:
   - `feat(server): TEST_FIXED_NOW env override for deterministic snapshots`
   - `feat(web): app-ready testid + hide-in-snapshots class`
   - `feat(e2e): theme-paired snapshot helpers and playwright projects`
   - `chore(e2e): rename docs-screenshots to paired light+dark`

**Sequencing caveat (F1 ↔ F7)**: F1 also touches `api.rs` (indirectly, via `WriteSender` threading). Land F7's `api.rs` patch separately (one extra env read + `OnceLock` initialiser) so it cherry-picks around F1 cleanly.

**Dispatch**: single implementer; final visual review via Claude subagent (no gpt-pro).

---

## Phase F8 — frontend polish bundle (L, sub-tasks landable independently)

> See `frontend-followups.md` for full per-sub-task spec.

Sub-tasks are independent; pick any order. Each one has its own red test, green fix, and commit.

| ID | Scope | Files | Verify |
|---|---|---|---|
| G.1 | `TopicTree` recursive children | `TopicTree.tsx`, `TopicNode.tsx` | existing topic e2e + new nested-add case |
| G.2 | `HandsQueue.tsx` — wire into HostSession or delete (default: wire) | `HandsQueue.tsx`, `HostSession.tsx` | new vitest covering visibility |
| G.3 | Modal a11y for `CreateBoardDialog` + `RaiseHandButton` modal | `CreateBoardDialog.tsx`, `RaiseHandButton.tsx`, `useModalFocus.ts` | vitest + e2e keyboard nav |
| G.4 | Raise-hand word-count regex parity | `web/src/lib/validation.ts` (new), `server/src/proto.rs`, both unit suites | edge-case parity tests (NBSP, multi-space, ZWJ) |
| G.5 | `QuestionComposer` rollback on error | `QuestionComposer.tsx` | vitest forcing `Ack`/`Error{rate_limit|muted}` paths |
| G.6 | Dark-mode parity (pen palette, pen text input bg, ClickPing/Cursor) via CSS vars | `index.css`, `PenToolPalette.tsx`, `PenTextLayer.tsx`, `ClickPingLayer.tsx`, `CursorLayer.tsx` | paired light/dark snapshots after F7 lands |

**Dispatch**: G.1, G.2, G.3, G.4, G.5 can be parallelised via `superpowers:dispatching-parallel-agents`. G.6 sequences **after** F7 so paired snapshots are available.

---

## Cross-phase sequencing risks

1. **F1 ↔ F5 coordination** — both touch every intent. F5 lands after F1; merge cost is mechanical but expensive otherwise.
2. **F1 ↔ F7 file overlap** — both touch `server/src/api.rs`. Land F7's `now_ms` patch separately so it cherry-picks around F1.
3. **F4 ↔ F1/F2/F3** — F4 tests assume persistence works. If a test is written ahead of its backing functionality, ship it `#[ignore = "blocked on F{n}"]` with a comment naming the phase.
4. **F7 ↔ F8.6** — F7 lays the dark-mode scaffold; F8.6 produces the first pen-board paired baselines. Order F8.6 after F7.
5. **F2 ↔ F3** — reaper assumes hydration works on next access. Order preserved.
6. **F1 writer batching** — pick batched-by-default up-front; unbatched would slow F4's persistence tests significantly.
7. **F1 schema** — V0005 `excalidraw_scenes` and V0006 `pen_actions.payload_json` ship **first** so subsequent writer arms have valid tables to write to.
