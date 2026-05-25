# Persistence — write path, hydration, schema deltas

> **Source of truth**: any new state-changing intent must add a `WriteOpKind` row here **before** wiring code. References to data-model: `../2026-05-24-amber-falcon/data-model.md`.

## 1. Design summary

One single-writer tokio task per process, owned by `AppState`, with a clone-able `tokio::sync::mpsc::UnboundedSender<WriteOp>`. Every state-changing intent enqueues a `WriteOp`; the writer drains in batches into one transaction per wake-up to amortise WAL fsync.

```
┌────────────────────────────┐         ┌────────────────────────────┐
│  ws connection handlers     │         │  REST handlers (api.rs)    │
│  (clone WriteSender)        │         │  (clone WriteSender)       │
└──────────────┬─────────────┘          └──────────────┬─────────────┘
               │ enqueue WriteOp                       │
               ▼                                       ▼
        ┌─────────────────────────────────────────────────┐
        │  mpsc::UnboundedSender<WriteOp>                 │
        └────────────────────────┬────────────────────────┘
                                 ▼
                ┌──────────────────────────────────┐
                │ writer task (single owner of a   │
                │ dedicated Connection)            │
                │   loop { drain → 1 tx → fsync }  │
                └──────────────────────────────────┘
```

### Why a dedicated connection (not a pool checkout)

SQLite WAL is happiest with a single writer connection holding its own statement cache. The read pool (`r2d2`, size = `num_cpus`) keeps read handlers fast. Pool size = 1 in `:memory:` mode is the exception — see §6.

### Why `mpsc::UnboundedSender`

Backpressure is undesirable for the ws-side: a slow disk should not stall protocol handling. Memory ceiling is the bounded room population × bounded LOC per message. If unbounded ever bites, swap to `bounded(8192)` + `try_send` fallback that logs `op_dropped` and degrades to best-effort.

### Batching

The writer wakes on the first `WriteOp`, then drains everything else available in the channel (`try_recv` loop with a 4 ms upper bound). All drained ops go into one `Transaction`. `transaction.commit()` once. This amortises fsync over bursts (e.g. a `PenStrokeEnd` followed quickly by `PenClear`).

## 2. WriteOp envelope

`WriteOp` is **always** addressed by `room_id` since most proto types (`Topic`, `Board`, `Question`, `PenStroke`) don't include a `room_id` field. Define:

```rust
pub struct WriteOp {
    pub room_id: String,
    pub kind: WriteOpKind,
}
```

`kind` is the exhaustive enum below. This also enables future per-room batching if/when that becomes useful.

## 3. WriteOpKind — exhaustive

> Add new variants by editing this file first, then wiring the writer arm, then the intent handler. Out-of-order edits will be rejected in review.

### Explicitly non-persistent intents

The following client→server intents intentionally have **no** `WriteOpKind` variant. They are documented here so a future reader doesn't accidentally add one:

- `SetDisplayName` — display name is per-connection presence state, surfaced via `PresenceUpdate`. There is no `guests` table in the data model. A guest who reconnects re-supplies their name in `Hello`.
- `RaiseHand` / `LowerHand` / `CallOnHand` / `DismissHand` — `hands` are ephemeral, cleared on disconnect (`../2026-05-24-amber-falcon/data-model.md` §3).
- `CursorMove` / `Click` — pure presence, never persisted.
- `GetSnapshot` / `Pong` / `Hello` — read-only / handshake / heartbeat.
- `rooms.last_active_at` — bumped lazily by `Db::set_kicked` / `set_muted`; not on every intent. Idle reaping uses the in-memory `Room::last_activity_at` atomic instead.

### Rooms

| Variant | Notes |
|---|---|
| `UpsertRoomMeta { title }` | Already covered by `POST /api/rooms`; included here so v1.x admin renames have a path. |
| `SetActiveTopic { topic_id: Option<String> }` | Writes `rooms.active_topic_id`. |
| `SetFocusedBoard { board_id: Option<String> }` | Writes `rooms.focused_board_id`. |

### Topics

| Variant | Notes |
|---|---|
| `UpsertTopic { topic: Topic }` | Insert or update. `ord` already computed client-side via fractional indexing. |
| `RenameTopic { id, title }` | |
| `MoveTopic { id, parent_id: Option<String>, ord }` | |
| `SetTopicStatus { id, status }` | `status ∈ {pending, done}`. |
| `DeleteTopic { id }` | FK cascade handles descendants. |

### Questions + votes

| Variant | Notes |
|---|---|
| `UpsertQuestion { question: Question }` | First insert is the common case; upsert covers idempotent re-emits. |
| `SetQuestionAnswered { id, answered }` | |
| `DeleteQuestion { id }` | |
| `AddVote { question_id, guest_id }` | `INSERT OR IGNORE` to honor dedup PK. |
| `RemoveVote { question_id, guest_id }` | |
| `PromoteQuestionToTopic { question_id, topic: Topic }` | **Atomic**: one tx that does `UpsertTopic` + `DeleteQuestion`. Modelling as two ops would risk crash-time inconsistency (see `../2026-05-24-amber-falcon/phases.md:272`). |

### Boards

| Variant | Notes |
|---|---|
| `UpsertBoard { board: Board }` | |
| `RenameBoard { id, title }` | |
| `DeleteBoard { id }` | FK cascade pen_strokes/pen_texts/pen_actions/excalidraw_scenes. |

### Pen

Strokes are written **once per stroke**, on `PenStrokeEnd`, with the fully-formed stroke (points + final `ord`). Intermediate `PenStrokeBegin`/`PenStrokeAppend` stay in-memory only. Rationale: per-point writes would explode WAL volume and risk persisting truncated strokes if the connection drops mid-stroke.

> **Same-transaction invariant (load-bearing)**: every pen variant below writes **both** the data mutation (`pen_strokes` / `pen_texts`) **and** the corresponding `pen_actions` row + `payload_json` inside *one* `rusqlite::Transaction`. Splitting them would corrupt undo: a crash between the two writes leaves an action row whose `payload_json` doesn't match the current data state, and `PenUndo` would then apply the inverse against the wrong base. Reviewers must verify this in F1.7.

> **`pen_actions.ord` allocation**: monotonic per board. The writer **must** allocate `ord` at apply time (read `MAX(ord) + 1 WHERE board_id = ?`) inside the same transaction, not at enqueue time on the ws side. Two parallel handlers enqueueing two strokes would otherwise collide on the same `ord`.

| Variant | Notes |
|---|---|
| `InsertCompletedPenStroke { stroke: PenStrokeSummary, action_id }` | Emitted only on `PenStrokeEnd`. Writes `pen_strokes` row + a `pen_actions` row (`kind="stroke_add"`, `target_id=stroke.id`, `payload_json=NULL`). |
| `UpsertPenText { text: PenText, action_id, before_json: Option<String> }` | `None` on first insert, `Some(prev_json)` when overwriting. `pen_actions.payload_json` stores `before_json`. |
| `DeletePenText { text_id, action_id, before_json: String }` | `payload_json = Some(before_json)`. |
| `PenClear { board_id, action_id, before_strokes_json: String, before_texts_json: String }` | `payload_json = Some({"strokes": …, "texts": …})`. |
| `PenUndo { board_id, target_action_id }` | Writer reads the action row + `payload_json`, applies the inverse (reinsert stroke / restore text / restore cleared board / undelete text), then deletes the action row — all inside one transaction. |

### Excalidraw

| Variant | Notes |
|---|---|
| `UpsertExcalidrawScene { board_id, scene_version, elements_json, app_state_json, updated_at }` | Requires V0005 migration (see §5). |

### Moderation

| Variant | Notes |
|---|---|
| `SetKicked { guest_id, kicked }` | Mirrors the existing `Db::set_kicked` semantics: preserves the other flag. The envelope's `room_id` carries the room dimension. |
| `SetMuted { guest_id, muted }` | Mirrors `Db::set_muted` semantics: preserves the other flag. |

> A combined `UpsertModeration { guest_id, kicked, muted }` would regress the contract validated by tests at `server/src/db.rs:243`. Keep them split.

## 4. Writer connection ownership decision

**Required pre-F1 design lock** (dispatched to `gpt-pro-run-review-dc` before code lands). The decision space:

1. **`Db` retains the path**, the writer reopens its own `Connection`. Simple but doubles file handles and breaks `:memory:` mode (separate connection ⇒ separate in-memory DB).
2. **`Db` exposes `clone_for_writer() -> Connection`** that under `:memory:` returns a clone from the existing pool's single connection; under file mode opens a fresh one. Honors the data-model.md §3 invariant.
3. **The writer holds a pool checkout** (`r2d2::PooledConnection`) for its lifetime. Simplest but blocks one pool slot forever — fine when pool size is `num_cpus`, breaks `:memory:` size-1 mode.

**Decision (recorded after pre-F1 review)**: option 2, but framed accurately for each mode. `r2d2_sqlite::SqliteConnectionManager::memory()` creates a **fresh anonymous database on every connect**, which is exactly why `open_in_memory()` forces `max_size(1)`. That has two implications the original framing glossed over:

- In `:memory:` mode, the writer **cannot** hold the pool's only connection for the lifetime of the loop — readers would deadlock. The writer **borrows** the connection per batch: `checkout → drain → commit → return`. The loop owns the `mpsc` receiver, not the connection.
- In file mode, the writer **owns** its own connection on spawn (a fresh handle via `SqliteConnectionManager::file(path)`) and holds it for the process lifetime. Readers continue to use the r2d2 pool.

The two modes are encapsulated behind a single API:

```rust
impl Db {
    /// Acquire the writer's connection handle.
    ///
    /// File mode: returns a freshly-opened, configured `Connection` that
    /// the writer owns for the rest of its life.
    ///
    /// `:memory:` mode: returns a borrowed `PooledConnection` from the
    /// size-1 pool. The writer must drop it between batches so reads
    /// can run.
    pub fn acquire_writer_conn(&self) -> Result<WriterConn, DbError>;
}

pub enum WriterConn {
    Owned(rusqlite::Connection),       // file mode
    Pooled(DbConn),                    // :memory: mode (DbConn = r2d2 PooledConnection)
}
```

The writer loop calls `acquire_writer_conn()` **once per batch** (cheap in file mode if the `Owned` variant is wrapped in an `Option<Connection>` cache; cheap in `:memory:` mode because `r2d2.get()` on a size-1 pool is just a mutex). The drain-then-commit pattern stays the same in both modes; only the connection lifecycle differs.

To detect mode, store a `mode: DbMode` discriminator on `Db` at construction time (`open_path` → `DbMode::File`, `open_in_memory` → `DbMode::Memory`). Cheaper than `PRAGMA database_list`.

## 5. Schema delta — required new migrations

Two migrations land **before** any writer arm references them.

### `server/migrations/V0005__excalidraw_scenes.sql`

Matches data-model.md §1. The in-memory model already references this table but the migration was never written.

```sql
-- Phase F1 schema: excalidraw scenes. See .plan/2026-05-25-followup/persistence.md §5.
CREATE TABLE excalidraw_scenes (
  board_id        TEXT PRIMARY KEY REFERENCES boards(id) ON DELETE CASCADE,
  scene_version   INTEGER NOT NULL DEFAULT 0,
  elements_json   TEXT NOT NULL DEFAULT '[]',
  app_state_json  TEXT NOT NULL DEFAULT '{}',
  updated_at      INTEGER NOT NULL
);
```

### `server/migrations/V0006__pen_action_payloads.sql`

Extends `pen_actions` with `payload_json TEXT NULL`. Today `pen_actions` only stores `kind` + `target_id` (`server/src/room.rs:65`, `V0004__pen_whiteboard.sql:39`), which is insufficient for durable `PenUndo`. Without this column the `before_json` / `before_strokes_json` / `before_texts_json` payloads in the WriteOp variants have nowhere to land.

```sql
-- Phase F1 schema: per-action undo payloads. See .plan/2026-05-25-followup/persistence.md §5.
ALTER TABLE pen_actions ADD COLUMN payload_json TEXT;
```

`ALTER TABLE ... ADD COLUMN` is forward-only/additive and safe under refinery. No backfill needed: existing rows get `NULL`, and the writer treats `NULL` as "no undo payload available" (pre-V0006 actions cannot be undone after a restart, which is acceptable since pre-F1 undo was already memory-only).

## 6. Test-mode constraint (`:memory:`)

`Db::open_in_memory()` forces `max_size(1)` because in-memory SQLite databases are not shareable across connections. The writer task must reuse that single connection or it sees an empty database. With option 2 above, `clone_for_writer()` returns the **same** pool's connection in `:memory:` mode.

If the writer holds the connection long-term, reads stall while the writer holds the tx. Mitigate by:

- Keeping the writer's transactions tight (open → drain → commit → release).
- For tests, use `:memory:` for unit tests of the writer itself; use file-backed `tempfile::TempDir` for integration tests so the read pool size > 1.

## 7. Hydration query

`RoomRegistry::get_or_create` runs a single read transaction on DashMap miss:

```rust
pub struct RoomHydrationBundle {
    pub topics: Vec<Topic>,
    pub active_topic_id: Option<String>,
    pub questions: Vec<QuestionWithVotes>,
    pub boards: Vec<Board>,
    pub focused_board_id: Option<String>,
    pub pen_per_board: HashMap<BoardId, PenBoardState>,
    pub excalidraw_per_board: HashMap<BoardId, ExcalidrawSceneState>,
    pub moderation: Vec<(GuestId, bool /*kicked*/, bool /*muted*/)>,
}

pub fn load_full_room_state(
    conn: &rusqlite::Connection,
    room_id: &str,
) -> Result<RoomHydrationBundle, DbError> { /* … */ }
```

Implementation lives in `server/src/db.rs`. **Must wrap all queries in a single `conn.transaction()?`** (`BEGIN DEFERRED` is fine — we hold the read snapshot until commit). Without an explicit transaction the six `SELECT`s run on autocommit and a concurrent writer batch can interleave, producing a half-old / half-new bundle.

Queries (one tx, one transaction-level snapshot):

1. `SELECT id,title,active_topic_id,focused_board_id FROM rooms WHERE id=?1`
2. `SELECT * FROM topics WHERE room_id=?1 ORDER BY parent_id, ord`
3. `SELECT q.*, COUNT(v.guest_id) AS votes, GROUP_CONCAT(v.guest_id) AS voter_ids FROM questions q LEFT JOIN question_votes v ON v.question_id=q.id WHERE q.room_id=?1 GROUP BY q.id ORDER BY q.created_at`
4. `SELECT * FROM boards WHERE room_id=?1 ORDER BY ord`
5. Per board kind, in batch: pen_strokes + pen_texts + pen_actions OR excalidraw_scenes
6. `SELECT guest_id, kicked, muted FROM moderation WHERE room_id=?1`

Hold the DashMap entry write-lock across the load to prevent dup-loads on first access. Wrap in `tracing::info_span!("hydrate", room_id)`.

Cost: data-model.md says <500 KB / room — defer optimisation until proven necessary.

## 8. Shutdown handling

`main.rs` signal handler:

1. Stops accepting new ws/http connections (axum's `with_graceful_shutdown`).
2. Closes the `WriteSender` (drops the held clone in `AppState`).
3. `join` the writer task with a bounded timeout (default 10 s).
4. Log `pending_ops_at_shutdown` if the timeout fires.

The writer task's drain loop sees the channel close, finishes the in-flight batch, commits, and exits. Implement as `while let Some(op) = rx.recv().await { drain_batch_then_commit() }` (no separate close signal needed — channel closure is the signal).

## 9. Risks

- **Write loss on crash before commit**: a batch that hasn't been committed at the moment of crash is lost. Mitigation: keep batch windows tight (≤4 ms). Document in `risks.md`.
- **Cold-room hydration latency**: a room with 1000+ questions might take >100 ms to hydrate on first access. Mitigation: deferred. The 500 KB / room ceiling makes this a non-issue at expected scale.
- **`:memory:` connection contention**: writer holding the single connection blocks reads. Mitigation: keep tx tight; if tests flake, switch the writer to a separate `Mutex<Connection>` from the pool.
- **`PenUndo` payload mismatch**: if a writer crash leaves a `pen_actions` row with the new state already applied but the inverse never persisted (impossible if we keep `payload_json` write same-tx as the original mutation). Ensure the `WriteOpKind` variants for `UpsertPenText` / `DeletePenText` / `PenClear` write both the data mutation **and** the action row + `payload_json` inside the same transaction.

## 10. Sequencing diagram (per intent)

```
client → ws frame → handler                ┐
                       │ validate          │
                       │ apply to in-mem   │  ← optimistic, broadcast goes out now
                       │ broadcast         │
                       │ enqueue WriteOp   ┘
                                  ↓ mpsc
                       ┌─────────────────────────┐
                       │ writer task: drain → tx │
                       └─────────────────────────┘
                                  ↓ commit
                                 done
```

In-memory state is authoritative within a process lifetime; the database is authoritative across restarts. Write-back is asynchronous.
