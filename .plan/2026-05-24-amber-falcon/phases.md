# Implementation phases

Each phase is independently mergeable + deployable. Each ends with the full review gate (Track A + Track B per `testing.md` §6) and a `postcommit-status-and-continue` call.

## Phase 0 — Bootstrap

**Goal**: Empty-but-runnable monorepo deployed to Railway as a "Hello World".

**Tasks**

0.1 — Init repo skeleton: top-level `justfile`, `scripts/`, `web/`, `server/`, `e2e/`, `.plan/`, `.gitignore`, `.editorconfig`, `LICENSE` (MIT).

0.2 — Frontend scaffold: `pnpm create vite web --template react-ts`; add Tailwind, shadcn primitives bootstrap, lucide-react, zustand, `@excalidraw/excalidraw`, `perfect-freehand`. Test it: `pnpm -C web build` works.

0.3 — Backend scaffold: `cargo new server --bin`. Add `axum`, `tokio`, `tower`, `tower-http`, `serde`, `serde_json`, `rusqlite` (bundled), `r2d2`, `r2d2_sqlite`, `refinery`, `tracing`, `tracing-subscriber`, `argon2`, `uuid`, `dashmap`. Hello-world `/healthz` route returning 200. `cargo run` works.

0.4 — Static-embed wiring: `rust-embed` includes `web/dist/`. Build-time skip if `web/dist/` absent (dev mode). Serve at `GET /`. `/api/*` and `/ws` higher precedence.

0.5 — Vitest scaffolding: one trivial passing test in `web/tests/`. `cargo test` scaffolding: one trivial passing backend test.

0.6 — Playwright scaffolding: install in `e2e/`, write a single test that visits the bound server URL and asserts the H1.

0.7 — `justfile` + scripts:
  - `just dev` (concurrent vite dev + cargo run)
  - `just build` (web build then cargo build --release)
  - `just test` (web + cargo + e2e)
  - `just test-web`, `just test-server`, `just test-e2e`
  - `just lint`
  - `just serve` (release binary)
  - `just serve-test` (release binary, in-memory db, debug logging, random port)
  - `just kimi-review` (script under `scripts/kimi-review.sh`)
  - `just snapshot-update`, `just snapshot-baseline`

0.8 — Dockerfile (multi-stage Node-build → Rust-build → distroless), `.dockerignore`.

0.9 — `railway.toml` minimum config (builder=DOCKERFILE, healthcheck=/healthz).

0.10 — GitHub org repo create: `gh repo create clankercode/topic-tree-with-qav --public --source . --remote origin --push`. Confirm `gh auth status` shows clankercode access first.

0.11 — Railway deploy: `railway login` if needed; `railway init`; `railway up`; capture URL; visit; assert hello world live.

0.12 — `.github/workflows/ci.yml` for lint + test parallel jobs.

**Acceptance**

- `just dev` shows working full-stack locally.
- CI green on PR.
- Live Railway URL serves the hello page.
- Empty repo committed and pushed to `clankercode/topic-tree-with-qav`.

**Review gate**: Track B only (no UI surface yet beyond hello). Skip kimi if diff tiny.

---

## Phase 1 — Room create + join + ws hello

**Goal**: Create a room (server-side), get admin URL, store in IndexedDB. Join a room as guest, ws handshake, see other party.

**Key tasks**

1.1 — DB migration 0001: `rooms`, `moderation` tables.

1.2 — `POST /api/rooms` route: generate id + adminToken, hash, persist, return `{roomId, adminUrl, joinUrl}`.

1.3 — Frontend: Landing page CTA → POST → store `{roomId, adminToken, role:'admin'}` in IDB → redirect to `/r/:id/host`.

1.4 — Frontend: `/rooms` dashboard listing IDB entries.

1.5 — `/ws` upgrade route, `Hello` message handler, role assignment based on `adminToken` validity (admin tokens hashed-compare via argon2).

1.6 — Welcome snapshot for empty room (just `room` + `you` + empty arrays).

1.7 — Guest entry flow: `/r/:id` → name prompt → ws connect with role=guest + guestId from localStorage.

1.8 — Presence: PresenceUpdate broadcast on connect/disconnect.

1.9 — Heartbeat (Ping/Pong) + auto-reconnect on client with exponential backoff.

1.10 — Admin-claim flow: `?admin=<token>` strips query, stores token, redirects.

**Tests (TDD)**

- Server unit: argon2 verify, room id format, snapshot shape.
- Server integration: create → hello → welcome roundtrip; bad token rejected; guest-then-host order; reconnect resyncs.
- Frontend unit: IDB roomRecord CRUD, ws reducer for Welcome + PresenceUpdate.
- E2E: full create→join→see-presence flow in two contexts.

**Acceptance**

- Host creates a room, sees admin URL banner, can reload and still be admin.
- Guest enters name, joins, host sees guest in presence list.
- Both have working heartbeat (verified by closing host tab; guest's presence updates within 5s).

**Review gate**: Track A (landing + dashboard + entry + empty session view), Track B.

---

## Phase 2 — Topic tree

**Goal**: Host edits a tree of topics; active/done state syncs to all guests.

**Key tasks**

2.1 — Migration 0002: `topics` table + index.

2.2 — Server intents: `AddTopic`, `RenameTopic`, `MoveTopic` (fractional `ord`), `DeleteTopic`, `SetActiveTopic` (auto-marks prior as done), `MarkTopicDone`.

2.3 — Server outbound: `TopicTreeUpdated` (full tree replace — simpler than diffs for this size).

2.4 — Frontend: `TopicTree` component, `TopicNode`, drag-to-reorder (using `@dnd-kit/sortable` — approved dep).

2.5 — Active-topic badge in topbar; pressing `j` / `k` (host only) advances.

2.6 — Done topics: muted style + check; clickable to undo done.

**Tests**

- Property: `set_active → at_most_one_active`.
- Property: fractional index insert-between always strictly between.
- E2E: host adds 3 topics, sets active on the second, all guests see badge + first/third pending. Host re-orders via drag; all guests see new order.

**Acceptance**: spec from `frontend.md` §7.

**Review gate**: Track A + B.

---

## Phase 3 — Q&A

**Goal**: Submit, list, vote, resort, autoscroll lock, anonymous, mark answered.

**Key tasks**

3.1 — Migration 0003: `questions`, `question_votes`.

3.2 — Server intents: `SubmitQuestion`, `VoteQuestion`, `MarkQuestionAnswered`, `DeleteQuestion`.

3.3 — Server outbound: `QuestionAdded`, `QuestionUpdated`, `QuestionDeleted`, `VoteUpdated`.

3.4 — Vote dedup by `(question_id, guest_id)` PK.

3.5 — Anonymous handling: `Question.author_guest_id` blanked in outbound payload when `anonymous=true`.

3.6 — Frontend: `QAPanel` with `QuestionComposer`, `QuestionList`, `VoteButton`, `SortToggle`, `AutoscrollLock`, jump buttons, "↑ New questions" pill.

3.7 — Sort modes: chronological asc default; vote-desc + createdAt-asc tiebreak when toggled.

3.8 — Composer anonymous checkbox.

**Tests**

- Property: `votes = count distinct guests`.
- E2E: 3 guests; one anon question, votes from two others, sort toggle reorders, mark answered hides into "answered" section (or toggles muted style — confirm UX choice in this phase).
- E2E: scroll guest's list up, new question arrives, "↑ New" pill appears; click jumps to bottom; lock disengages.

**Review gate**: Track A + B.

---

## Phase 4 — Pen whiteboard (drawing + text)

**Goal**: Host draws and types on a pen board; all clients see it smoothly; undo/clear works.

**Key tasks**

4.1 — Migration 0004: `boards`, `pen_strokes`, `pen_texts`.

4.2 — Server intents per `protocol.md` §pen.

4.3 — Server outbound per `protocol.md` §pen.

4.4 — Server replay: on `Welcome`, board's strokes + texts included.

4.5 — Frontend: `PenBoard` + `PenCanvas` (HTMLCanvasElement) + `PenTextLayer`.

4.6 — Stroke pipeline using `perfect-freehand` for smooth outlines. Batch points per `requestAnimationFrame`.

4.7 — Text tool: click-to-place input; commit sends `PenTextSet`.

4.8 — `PenToolPalette` (host only): color picker (preset 8 colors + custom), size slider, text tool, undo, clear.

4.9 — Undo: server-side last-50 stack per board; broadcasts `PenUndone`.

4.10 — Clear with confirm dialog.

**Tests**

- Unit: stroke-points → outline render is stable (snapshot of rendered polygon path).
- Integration: stroke lifecycle, persistence, replay.
- E2E: host draws diagonal line; guest sees same line within 250ms; host adds text "Hello"; guest sees; host undoes; guest sees text removed.
- Visual: pen board sample stroke render matches baseline (deterministic seed for points).

**Review gate**: Track A + B.

---

## Phase 5 — Excalidraw whiteboard

**Goal**: Host creates Excalidraw boards; full edit; guests see read-only sync.

**Key tasks**

5.1 — Migration 0005: `excalidraw_scenes`.

5.2 — Server intents: `ExcalidrawUpdate` (admin only). Persist + broadcast `ExcalidrawDelta`.

5.3 — Frontend: `ExcalidrawBoard` mounts `<Excalidraw>` with `viewModeEnabled={!isHost}`.

5.4 — Host onChange debounced to 150ms → `ExcalidrawUpdate`.

5.5 — Guest receives `ExcalidrawDelta` → `excalidrawAPI.updateScene({elements, appState})`.

5.6 — `CreateBoard` UI: host picks kind in a dialog.

5.7 — `BoardTabs` strip on top of board area; tabs show kind icon + title.

5.8 — `SetFocusedBoard` + `Follow host` toggle behavior.

**Tests**

- Integration: guest's ExcalidrawUpdate is rejected.
- E2E: host draws rect, arrow; guest sees both. Guest's UI has no toolbar (assert by selector absence). Host switches focused board; following guest follows; unfollowing guest stays.

**Review gate**: Track A + B.

---

## Phase 6 — Cursors + click pings

**Goal**: Live cursor positions + click ping animations across both board types.

**Key tasks**

6.1 — Server: `Cursor`, `Click` intents; broadcasts `CursorMoved`, `Clicked`.

6.2 — Rate-limit per `protocol.md` §rate-limits.

6.3 — Frontend: `CursorLayer` with name-labeled cursors, 50ms position interpolation, 5s timeout.

6.4 — `ClickPingLayer` with 1.2s expand-fade ring + name label.

6.5 — Excalidraw collaborator API integration for cursors on Excalidraw boards.

**Tests**

- Integration: rate-limit drops excess; presence-derived cursor cleanup on disconnect.
- E2E: 3 guests move cursors; all clients see all cursors; click ping appears on all clients including clicker.

**Review gate**: Track A + B.

---

## Phase 6.5 — Raise hand + promote Q→topic

**Goal**: Guests can raise hand with a short topic (1-10 words). Host sees a queue and can call on or dismiss. Host can promote any Q&A item into a new topic-tree node with one click.

**Key tasks**

6.5.1 — Server: `RaiseHand`, `LowerHand`, `CallOnHand`, `DismissHand` intents. State lives in-memory in `RoomState.hands: BTreeMap<GuestId, RaisedHand>`; not persisted (ephemeral). Broadcast `HandsUpdated` on every change.

6.5.2 — Server: word-count + char-length validation on raise topic (≤10 words, ≤80 chars).

6.5.3 — Server: `PromoteQuestionToTopic {questionId, parentTopicId?, afterTopicId?}` — atomic transaction: create topic (title = question text, truncated to 80 chars), delete question. Broadcast `TopicTreeUpdated` + `QuestionPromotedToTopic` (clients use the latter to animate the removal-then-add).

6.5.4 — Frontend: `RaiseHandButton` in topbar for guests; opens a small input ("In ≤10 words, what would you like to ask?") with live word/char counter. Disabled if hand already raised; shows "Lower hand" instead. Reasoning: hand-raise should be deliberate.

6.5.5 — Frontend: `HandsQueue` panel for host only — vertical list of `{name, topic}` with "Call on" + "Dismiss" buttons each. Sorted by `raisedAt`. Empty state: "No raised hands."

6.5.6 — Frontend: "Promote to topic" button on each host-side Q&A item. Click → confirm popover with target parent selector (default: root) → fires intent. UI optimistically removes question and inserts topic; reverts on error.

**Tests**

- Server unit: word-count validator rejects 11 words; truncation on promote.
- Integration: raise lifecycle (raise → list update → host call-on → list shrinks); promote-to-topic atomicity (if topic-create fails, question stays).
- E2E: guest raises hand "demo question please"; host sees queue entry; host calls on; queue empties. Host promotes a Q&A item; Q disappears, topic appears in tree.

**Review gate**: Track A + B.

---

## Phase 7 — Moderation

**Goal**: Host can mute, kick, delete questions.

**Key tasks**

7.1 — Server: `KickGuest`, `MuteGuest`, `DeleteQuestion` (already in Phase 3 — verify).

7.2 — Server: blocked guests rejected at `Hello` if `moderation.kicked=1`.

7.3 — Frontend: per-presence menu (host only) with mute/kick.

7.4 — Frontend: kicked guest sees a friendly "removed by host" screen.

7.5 — Muted guest's `SubmitQuestion` / `VoteQuestion` are server-rejected with a polite error toast.

**Tests**

- Integration: kicked guest cannot reconnect; muted guest's submit returns error.
- E2E: kick flow from host; guest UI shows removal screen.

**Review gate**: Track A + B.

---

## Phase 8 — Visual polish + theming + accessibility

**Goal**: Apply `frontend-design` skill output; lock visual baseline.

**Key tasks**

8.1 — Invoke `frontend-design` skill with the design language brief + screenshots from phase 1-7.

8.2 — Apply recommendations to shared components, theme tokens, motion, iconography.

8.3 — Theme toggle component + system-preference detection + persistence.

8.4 — Light + dark mode coverage of every component.

8.5 — Keyboard navigation pass: every action reachable by keyboard; focus visible; modals trap focus.

8.6 — `aria-*` audit: roles, labels, live regions for Q&A list.

8.7 — Contrast audit: all text ≥ WCAG AA in both themes.

8.8 — Lock visual baseline: regenerate `e2e/screenshots/` baselines and commit.

**Tests**

- Visual regression baselines.
- a11y: integrate `@axe-core/playwright`; one e2e per page asserting zero violations of `serious`/`critical`.

**Review gate**: Track A (most intensive of any phase) + Track B. Expect multiple `review-and-fix` rounds.

---

## Phase 9 — Hardening, observability, prod sanity

**Goal**: Ship-ready production deploy.

**Key tasks**

9.1 — Structured logging review: spans per request, room_id everywhere, JSON in prod.

9.2 — `/metrics` endpoint (basic counters).

9.3 — Connection-loss UX: banner on reconnect, queue intents while disconnected (best-effort), discard if older than 10s on resume.

9.4 — Snapshot fetch on suspected desync (msg gap detection).

9.5 — Rate-limit error handling on client — toast + cooldown UI.

9.6 — Backup script: `just db-dump` writes a tarball of the SQLite db.

9.7 — Final Railway env review: PORT, DATABASE_PATH, RUST_LOG, volumes mounted, healthcheck verified.

9.8 — README with quickstart, contributor guide, deploy notes.

**Review gate**: Track B. Track A only if README/landing changed.

---

## Cross-cutting

- **Every task ends with a commit**. Commit message style: `feat(scope): summary`, `fix(scope): summary`, `test(scope): …`, `refactor(scope): …`. Scope examples: `topictree`, `qa`, `whiteboard-pen`, `ws`, `db`, `ui`.
- **Every commit triggers** `postcommit-status-and-continue`.
- **Per-phase status entry** appended to `.plan/STATUS.md`.
- **Worktree usage**: optional, recommended for phases 4-5 since they touch a lot of files; the rest can run on a single branch.
