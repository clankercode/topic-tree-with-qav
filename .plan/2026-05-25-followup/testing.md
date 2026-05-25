# Testing — F0 harness, F4 named tests, F7 visual-regression contract

## 1. F0 harness contract

`server/tests/common/mod.rs` exposes:

```rust
pub struct TestApp {
    pub addr: std::net::SocketAddr,
    pub state: server::AppState,
    pub server_handle: tokio::task::JoinHandle<()>,
}

impl TestApp {
    /// Spawn an Axum server on a random `127.0.0.1:0` port backed by an
    /// in-memory SQLite database. Caller can `drop(app)` and re-`spawn()`
    /// on the same `state.db` to simulate a restart.
    pub async fn spawn() -> Self;

    /// Spawn over a specific `Db` (for restart tests — pass the same Db
    /// to the new TestApp).
    pub async fn spawn_with_db(db: server::Db) -> Self;

    /// HTTP: POST /api/rooms; returns (room_id, admin_token).
    pub async fn create_room(&self, title: Option<&str>) -> (String, String);

    /// WS: connect with the given role; for `Host` pass the admin token,
    /// for `Guest` pass a guest_id. Returns a `WsClient` that wraps the
    /// underlying `WebSocketStream<MaybeTlsStream<TcpStream>>`.
    pub async fn connect_ws(
        &self,
        role: ClientRole,
        room_id: &str,
        token_or_guest_id: &str,
        display_name: &str,
    ) -> WsClient;
}

pub struct WsClient { /* … */ }
impl WsClient {
    pub async fn send_json(&mut self, msg: serde_json::Value);
    pub async fn recv_json(&mut self) -> serde_json::Value;
    /// Drain until a message matching a JSON pointer + predicate arrives, or timeout.
    pub async fn await_msg(
        &mut self,
        timeout: std::time::Duration,
        matcher: impl Fn(&serde_json::Value) -> bool,
    ) -> serde_json::Value;
}

pub enum ClientRole { Host, Guest }
```

### Test-mode DB helpers (in `common/mod.rs`)

```rust
pub fn read_questions_for_test(conn: &rusqlite::Connection, room_id: &str) -> Vec<Question>;
pub fn read_topics_for_test(conn: &rusqlite::Connection, room_id: &str) -> Vec<Topic>;
pub fn read_boards_for_test(conn: &rusqlite::Connection, room_id: &str) -> Vec<Board>;
pub fn read_pen_strokes_for_test(conn: &rusqlite::Connection, board_id: &str) -> Vec<PenStrokeSummary>;
pub fn read_excalidraw_scene_for_test(conn: &rusqlite::Connection, board_id: &str) -> Option<ExcalidrawSceneState>;
```

These let F4 assert against the persisted DB row without going through `Welcome`/`GetSnapshot`.

### Smoke test (`ws_smoke.rs`)

```rust
#[tokio::test]
async fn client_receives_welcome_after_hello() {
    let app = TestApp::spawn().await;
    let (room_id, admin) = app.create_room(None).await;
    let mut ws = app.connect_ws(ClientRole::Host, &room_id, &admin, "host").await;
    let welcome = ws.await_msg(Duration::from_secs(2), |m| m["type"] == "Welcome").await;
    assert_eq!(welcome["roomId"], room_id);
}
```

## 2. F4 — the 9 named integration tests

Each test owns its own `TestApp`. Tests are independent; no shared state. Anti-flake: every `recv_json` / `await_msg` carries an explicit timeout, never an unbounded wait.

### `ws_room_lifecycle.rs`

**`create_room_returns_admin_token_and_room_id`**
- Setup: `TestApp::spawn()`.
- Action: `POST /api/rooms` with `{"title": "Plenary"}`.
- Assert: HTTP 201; response has 12-char `roomId` (b32 charset), `adminToken` ≥ 16 bytes b64url, `adminUrl` matches `/r/<roomId>?admin=<adminToken>`, `joinUrl` matches `/r/<roomId>`.

**`hello_with_invalid_admin_token_returns_error`**
- Setup: `TestApp::spawn()`; create a room.
- Action: connect ws as Host with an admin token of the right shape but not the room's actual token.
- Assert: server sends `Error{code:"auth_failed"}` and closes the socket within `2s`.

### `ws_questions.rs`

**`submit_question_broadcasts_to_all_clients_in_room`**
- Setup: spawn; create room; connect Host + Guest1 + Guest2.
- Action: Guest1 sends `SubmitQuestion{text:"foo"}`.
- Assert: Guest1 receives `Ack`, all three clients receive `QuestionAdded{text:"foo"}` within `2s`, ordering doesn't matter.

**`vote_question_dedups_by_guest_id`**
- Setup: spawn; create room; connect Guest1 + Guest2 + Guest3.
- Action: Guest1 submits a question (capture its `id`). Guest2 sends `VoteQuestion{questionId, vote:true}` twice. Guest3 sends `VoteQuestion` once.
- Assert: final broadcast count is 2 (one per distinct guest_id); the DB row count via `read_questions_for_test` returns the same.

### `ws_topics.rs`

**`set_active_topic_marks_previous_active_as_done`**
- Setup: spawn; create room; connect Host.
- Action: Host adds Topic A, Host sets A active, Host adds Topic B, Host sets B active.
- Assert: B's `status="active"`; A's `status="done"`; `rooms.active_topic_id` (via DB read) equals B's id.

### `ws_pen.rs`

**`pen_stroke_lifecycle_persists_and_replays_on_reconnect`**
- Setup: spawn; create room; connect Host; add a pen board; capture `boardId`.
- Action: Host sends `PenStrokeBegin{id:S, color, size, point}` → `PenStrokeAppend{id:S, point}` × 3 → `PenStrokeEnd{id:S, finalPoints}`.
- Assert (write path): within `2s` `read_pen_strokes_for_test(conn, boardId)` returns one stroke whose points match the `finalPoints`.
- Assert (replay path): drop the Host client; connect a new Host; the new `Welcome` includes the stroke.

### `ws_excalidraw.rs`

**`excalidraw_update_from_guest_is_rejected_when_view_mode`**
- Setup: spawn; create room; Host adds an excalidraw board; connect Guest with `viewMode=true` (server-side default for guests).
- Action: Guest sends `ExcalidrawUpdate{elements:…}`.
- Assert: Guest receives `Error{code:"forbidden"}`; the DB excalidraw scene row is unchanged; no broadcast to Host or other clients.

### `ws_rate_limit.rs`

**`cursor_messages_exceeding_rate_limit_are_dropped`**
- Setup: spawn; connect Host + Guest. **Use `tokio::time::pause` + `advance`** so the test is deterministic.
- Action: Guest sends `CursorMove` at 100 Hz for 1 simulated second (well above the configured ceiling).
- Assert: Host observes ≤ (configured rate × duration) `CursorMoved` broadcasts; the rest are dropped silently (no `Error` echoes back to Guest for cursors per protocol rules).
- Note: depends on `RateLimiter::with_clock(impl Clock)` — if not yet plumbed, this test introduces the clock-injection seam.

### `ws_moderation.rs`

**`kicked_guest_cannot_reconnect_until_room_unblocks`**
- Setup: spawn; create room; connect Host + Guest.
- Action: Host sends `KickGuest{guestId}`.
- Assert: Guest receives `KickNotice` and the socket closes; Guest reconnect with same `guestId` triggers `Error{code:"kicked"}` and close. After Host sends `UnkickGuest{guestId}`, Guest reconnect succeeds with `Welcome`.

## 3. Test naming + organisation

- Test files match area: `ws_<area>.rs`.
- Fn names match `testing.md` §3 in `../2026-05-24-amber-falcon/`.
- Asserts use `assert_eq!` with custom messages; helpers don't `panic!` silently.
- All ws `recv_json` calls go through `WsClient::await_msg(timeout, matcher)` — never raw `recv` in tests.
- A test that depends on a phase not yet landed: ship with `#[ignore = "blocked on F{n}"]` and a tracking comment.

## 4. CI

- `just test-server-integration` (new recipe if not present): `cargo test --tests --test ws_smoke --test ws_room_lifecycle …` — or simply `cargo test --tests` if the binary handles all of them.
- F4 tests run in parallel by default (each `TestApp` is isolated). If flakes appear, gate to single-threaded with `RUST_TEST_THREADS=1` only on the flaking test (mark with `#[serial]` via `serial_test` crate if it becomes necessary).

## 5. F7 — visual-regression infrastructure contract

### Anti-flake primitives

- **`TEST_FIXED_NOW=<epoch_ms>`** read once at server startup via `OnceLock` in `server/src/api.rs::now_ms()`. When set, `now_ms()` returns the fixed value (or `fixed + monotonic_delta` if reviewers prefer; default: simple fixed return).
- **`data-testid="app-ready"`** rendered by `web/src/App.tsx` once the initial `Welcome` snapshot has applied.
- **`.hide-in-snapshots`** CSS class in `web/src/index.css`. Elements that animate or that carry presence counters (toasts, cursors, presence indicator) get `data-testid="hide-in-snapshots"`. Playwright applies a per-screenshot stylesheet that hides them.

### Helpers (`e2e/utils/snapshot.ts`)

```ts
export async function awaitAppReady(page: Page) {
  await page.waitForLoadState('networkidle');
  await page.waitForSelector('[data-testid="app-ready"]', { state: 'attached' });
  await page.emulateMedia({ reducedMotion: 'reduce' });
}

/**
 * Takes a screenshot in both light and dark themes within a single
 * call, naming the files `<name>-light.png` and `<name>-dark.png`.
 */
export async function expectThemedScreenshot(page: Page, name: string) {
  await page.evaluate(() => localStorage.setItem('theme', 'light'));
  await page.reload();
  await awaitAppReady(page);
  await expect(page).toHaveScreenshot(`${name}-light.png`, { maxDiffPixelRatio: 0.005 });

  await page.evaluate(() => localStorage.setItem('theme', 'dark'));
  await page.reload();
  await awaitAppReady(page);
  await expect(page).toHaveScreenshot(`${name}-dark.png`, { maxDiffPixelRatio: 0.005 });
}
```

### Playwright projects

`e2e/playwright.config.ts`:

```ts
projects: [
  {
    name: 'chromium-light',
    use: { ...devices['Desktop Chrome'], colorScheme: 'light' },
    metadata: { theme: 'light' },
  },
  {
    name: 'chromium-dark',
    use: { ...devices['Desktop Chrome'], colorScheme: 'dark' },
    metadata: { theme: 'dark' },
  },
],
```

Specs that want a paired snapshot use `expectThemedScreenshot`; specs that want one project's run use the project filter.

### Pairing rule

`scripts/check-snapshot-pairs.sh` must exit 0 against the committed baseline. Today it fails on 7 PNGs under `e2e/screenshots/_docs/`. The fix:

- Rename `dark-empty-room.png` → `empty-room-dark.png`, `empty-room.png` → `empty-room-light.png`.
- `dark-mid-session.png` → `mid-session-dark.png`, `mid-session-topics.png` → `mid-session-light.png`.
- `dark-qa-active.png` → `qa-active-dark.png`, `qa-active.png` → `qa-active-light.png`.
- `pen-board-content.png` → produce a paired `pen-board-content-light.png` + `pen-board-content-dark.png` via the new infra (F8.6 supplies the dark mode parity needed for the dark variant to look right).

### CI

`.github/workflows/ci.yml` already runs `scripts/check-snapshot-pairs.sh`. After F7 lands, the check becomes meaningful instead of always-failing-but-tolerated.

## 6. E2E specs touched by this follow-up

- `whiteboard.spec.ts` — fixed by F6.
- `docs-screenshots.spec.ts` — rewritten by F7 to use `expectThemedScreenshot`.
- New snapshot spec covering the pen palette in light/dark (G.6) — added after F7 lands.

## 7. Out of scope

- Property tests (covered by `../2026-05-24-amber-falcon/testing.md` §7; no new prop tests in this follow-up).
- Mobile viewports — `../2026-05-24-amber-falcon/testing.md` mentions them but they are not in the F0–F8 scope.
- Cross-browser e2e — Chromium only, matching the current CI baseline.
