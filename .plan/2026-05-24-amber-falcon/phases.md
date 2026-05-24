# Implementation phases

Each phase is independently mergeable + deployable. Each ends with the full review gate (Track A + Track B per `testing.md` §6) and a `postcommit-status-and-continue` call.

## Phase 0 — Bootstrap

**Goal**: Empty-but-runnable monorepo deployed to Railway as a "Hello World".

**Tasks**

0.1 — Init repo skeleton: top-level `justfile`, `scripts/`, `web/`, `server/`, `e2e/`, `docs/`, `.plan/`, `.gitignore`, `.editorconfig`, `LICENSE` (MIT). Add `pnpm-workspace.yaml` declaring `web/`, `e2e/`, `docs/` and a root `package.json`. Create `web/dist/.gitkeep` so the embed path always exists for builds.

0.2 — Frontend scaffold: `pnpm create vite web --template react-ts`; add Tailwind, shadcn primitives bootstrap, lucide-react, zustand, `@excalidraw/excalidraw`, `perfect-freehand`, `@dnd-kit/sortable`. Test it: `pnpm -C web build` works.

0.3 — Backend scaffold: `cargo new server --bin`. Add `axum`, `tokio` (rt-multi-thread), `tower`, `tower-http`, `serde`, `serde_json`, `rusqlite` (bundled), `r2d2`, `r2d2_sqlite`, `refinery`, `tracing`, `tracing-subscriber`, `argon2`, `uuid`, `dashmap`, `ts-rs`. Hello-world `/healthz` route returning 200. **Commit `server/Cargo.lock`** (binary crate convention). `cargo run` works.

0.4 — Static-embed wiring: `rust-embed` includes `web/dist/`. `server/build.rs` creates `web/dist/` if absent (dev-mode safety). Serve at `GET /*` (SPA fallback). `/api/*` and `/ws` higher precedence.

0.5 — Vitest scaffolding: one trivial passing test in `web/tests/`. `cargo test` scaffolding: one trivial passing backend test.

0.6 — Playwright scaffolding: install in `e2e/`, write a single test that visits the bound server URL and asserts the H1. The `webServer` in `playwright.config.ts` boots `just serve-test` against a per-test temp-file SQLite DB (NOT `:memory:` — see Phase 0.7).

0.7 — `justfile` + scripts. All recipes per CLAUDE.md §4–6. **`just serve-test` uses a temp-file DB** (`mktemp -d` + cleanup trap) **not `:memory:`** to avoid r2d2 multi-connection split.

0.8 — Rate-limit middleware scaffold (`server/src/rate_limit.rs`): per-client, per-message-type token-bucket. Wired into ws handler but with permissive defaults until Phase 6.5 tightens them — every intent is throttle-protected from the start.

0.9 — Dockerfile (multi-stage Node-build → Rust-build → distroless). Runtime image: **drop `USER nonroot`** (or use Railway's `RAILWAY_RUN_UID`/`RAILWAY_RUN_GID` mechanism so the volume mount at `/data` is writable by our process; default Railway volumes are owned by root, distroless `nonroot` cannot write). Entry script ensures `/data` is writable on first boot. `.dockerignore` excludes `target/`, `node_modules/`, `.git/`, `.review/`, `e2e/test-results/`.

0.10 — `railway.toml` (builder=DOCKERFILE, healthcheckPath=/healthz, **healthcheckTimeout=30** to survive cold start + migrations). Server binds `0.0.0.0:$PORT` reading the Railway-injected `$PORT`; the Dockerfile's `ENV PORT=3000` is the *local* default only and Railway will override it.

0.11 — GitHub repo create under clankercode: `gh repo create clankercode/topic-tree-with-qav --public --source . --remote origin --push`. Confirm `gh auth status` shows clankercode access first.

0.12 — Railway team + project + volume: `bash scripts/railway-init.sh` creates team `clankercode` (idempotent), project `topic-tree-with-qav`, volume mounted at `/data`. Sets env: `DATABASE_PATH=/data/app.db`, `RUST_LOG=info`.

0.13 — Railway deploy: `railway up`; capture URL; visit; assert hello world live.

0.14 — `.github/workflows/ci.yml`: lint + test parallel jobs. CI step installs `just` (via `extractions/setup-just@v2`) and `pnpm -C e2e exec playwright install --with-deps chromium`.

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

1.5 — `/ws` upgrade route, `Hello` message handler. `adminToken` verified **once** via argon2 (one hashing per connection), result cached as `role: Host | Guest` on the session. Subsequent admin-only intents are gated by role, not by re-verifying the token.

1.6 — Welcome snapshot for empty room (`room`, `you`, empty arrays, `seq=0`, `myVotes=[]`, `hands=[]`).

1.7 — Guest entry flow: `/r/:id` → name prompt → ws connect with role=guest + guestId from localStorage.

1.8 — Presence: PresenceUpdate broadcast on connect/disconnect. `SetDisplayName` (guest self) updates presence + broadcasts.

1.9 — Heartbeat (Ping/Pong) + auto-reconnect on client with exponential backoff. `seq` tracking on client; gap → `GetSnapshot`.

1.10 — Host-claim flow: `?admin=<token>` strips query within 50ms, stores token in IDB, redirects to `/r/:id/host`.

1.11 — `GetSnapshot` handler: returns a fresh `Welcome`-shaped payload over the existing connection.

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
- E2E (host only): `j` advances to next pending topic and sets it active (prior auto-marked done); `k` reverses. Asserts via key event + DOM.

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

3.7 — Sort modes: default chronological sorted by `(answered asc, createdAt asc)` — unanswered above answered. "Resort by votes" toggles to `(answered asc, voteCount desc, createdAt asc)`.

3.8 — Composer anonymous checkbox.

3.9 — Answered styling: muted text, strikethrough, faded vote count. Questions stay visible; not hidden.

**Tests**

- Property: `votes = count distinct guests`.
- E2E: 3 guests; one anon question, votes from two others, sort toggle reorders (answered always at bottom regardless of sort mode), mark answered demotes the row visually (asserts muted/strikethrough class + still visible).
- E2E: scroll guest's list up, new question arrives, "↑ New" pill appears; click jumps to bottom; lock disengages.
- E2E: jump-to-top + jump-to-bottom corner buttons both work (scrolls + lock state correct).
- E2E: anonymous question doesn't surface author name to other clients (DOM inspection asserts "Anonymous"), but server retains real `guest_id` (asserted via integration test, not e2e).

**Review gate**: Track A + B.

---

## Phase 4 — Pen whiteboard (drawing + text)

**Goal**: Host draws and types on a pen board; all clients see it smoothly; undo/clear works.

**Key tasks**

4.1 — Migration 0004: `boards`, `pen_strokes`, `pen_texts`, `pen_actions`.

4.2 — Server intents per `protocol.md` §pen (PenStrokeBegin/Append/End, PenTextSet, PenTextDelete, PenClear, PenUndo).

4.3 — Server outbound per `protocol.md` §pen (PenStrokeBegun/Appended/Ended, PenTextUpserted, PenTextDeleted, PenCleared, PenUndone).

4.4 — Server replay: on `Welcome`, board's strokes + texts included as part of `boards[].content`.

4.5 — **Auto-create a default pen board** on room creation (server-side, inside `POST /api/rooms`), so Phase 4 is independently usable before Phase 5's `CreateBoard` UI lands.

4.6 — Frontend: `PenBoard` + `PenCanvas` (HTMLCanvasElement) + `PenTextLayer`.

4.7 — Stroke pipeline using `perfect-freehand` for smooth outlines. Batch points per `requestAnimationFrame`. Coordinate-space mapping per `whiteboards.md` §3.

4.8 — Text tool: click-to-place input; commit sends `PenTextSet`. Selecting an existing text + Backspace sends `PenTextDelete`.

4.9 — `PenToolPalette` (host only): color picker (preset 8 colors + custom), size slider, text tool, undo, clear.

4.10 — Undo: server pops highest-`ord` from `pen_actions`, applies inverse, broadcasts `PenUndone`. Last 50 in-memory per board.

4.11 — Clear with confirm dialog. Writes a `clear` row to `pen_actions` as sentinel.

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

5.6 — `CreateBoard` UI: host picks kind (pen | excalidraw) in a dialog.

5.7 — `RenameBoard` + `DeleteBoard` intents + UI (inline-edit on tab + ⋯ menu with delete confirm).

5.8 — `BoardTabs` strip on top of board area; tabs show kind icon + title + ⋯ menu (host) / read-only (guest).

5.9 — `SetFocusedBoard` + `Follow host` toggle behavior.

5.10 — `ExcalidrawSceneReset` server-driven anti-drift snapshot every 60s for each Excalidraw board with changes.

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
- E2E: 3 guests move cursors; all clients see all cursors with **correct display-name labels** (DOM assertion); click ping appears on all clients including clicker with the **clicker's display name** floating above.
- E2E: `PresenceHoverCard` hover lists all current participants with display names.

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

**Goal**: Host can mute / unmute, kick, delete questions.

**Key tasks**

7.1 — Server: `KickGuest`, `MuteGuest {muted:bool}` (toggle), `DeleteQuestion` (already in Phase 3 — verify).

7.2 — Server: blocked guests rejected at `Hello` if `moderation.kicked=1`. Mute rejects `SubmitQuestion`/`VoteQuestion`/`RaiseHand` with `Error{code:"muted"}`.

7.3 — Frontend: per-presence menu (host only) with mute (with toggle to unmute) + kick.

7.4 — Frontend: kicked guest sees a friendly "removed by host" screen.

7.5 — Muted guest's intents are server-rejected with a polite error toast.

**Tests**

- Integration: kicked guest cannot reconnect; muted guest's submit returns error; unmute → submit succeeds.
- E2E: full moderation matrix — host kicks guest A (UI removal screen); host mutes guest B then unmutes (vote works after); host deletes a question (gone from all clients).

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

8.8 — Lock visual baseline: regenerate `e2e/screenshots/` baselines and commit. **Every baseline shot is paired light + dark** (`<name>-light.png` + `<name>-dark.png`); CI fails if a screenshot exists without its theme pair.

8.9 — Mobile (<900px) e2e: viewport 390×844, tab bar appears, switches between Tree / Board / Q&A panels, no horizontal overflow, light + dark snapshots locked.

**Tests**

- Visual regression baselines.
- a11y: integrate `@axe-core/playwright`; one e2e per page asserting zero violations of `serious`/`critical`.

**Review gate**: Track A (most intensive of any phase) + Track B. Expect multiple `review-and-fix` rounds.

---

## Phase 9 — Hardening, observability, prod sanity

**Goal**: Ship-ready production deploy.

**Key tasks**

9.1 — Structured logging review: spans per request, room_id + client_id everywhere, JSON in prod.

9.2 — `/metrics` endpoint (basic counters). *Scope add beyond original spec — keep behind a `--features metrics` flag if it costs us anything.*

9.3 — Connection-loss UX: banner on reconnect, queue intents while disconnected (best-effort in-memory), discard if older than 10s on resume.

9.4 — Snapshot fetch on `seq`-gap desync via `GetSnapshot`.

9.5 — Rate-limit error handling on client — toast + cooldown UI driven by `Error{code:"rate_limit"}`.

9.6 — Backup: `just db-dump LOCAL` writes a tarball of `./dev.db`; `just db-dump RAILWAY` uses `railway run cat /data/app.db ...` (or volume-mount snapshot) for a prod backup.

9.7 — Final Railway env review: `$PORT` honored, `DATABASE_PATH=/data/app.db`, `RUST_LOG=info`, volume mounted writable, healthcheck verified, no `nonroot` write errors.

9.8 — README with quickstart, contributor guide, deploy notes. Links to `/docs` for end-user docs.

**Tests / acceptance**

- E2E: forced ws drop (Playwright `route` abort) → client reconnects + banner shows + state matches host within 2s.
- E2E: server returns `rate_limit` → cooldown UI appears, intent retried after cooldown.
- Integration: `seq` gap injection → client calls `GetSnapshot` automatically.
- Smoke: `/healthz` 200 in <100ms under load (k6 or `hey` with 100 RPS).
- Backup: `just db-dump LOCAL` produces a non-empty tarball and `tar -tzf` lists `dev.db`.

**Review gate**: Track B. Track A only if landing/dashboard pages changed.

---

## Phase 9.5 — GitHub Pages docs site

**Goal**: A polished public docs site at `https://clankercode.github.io/topic-tree-with-qav/` (or repo-pages equivalent) covering deployment and usage, with screenshots harvested from the e2e suite. Sourced from `docs/`; built and published via CI.

**Key tasks**

9.5.1 — Pick docs static-site generator. Options: (a) Vitepress (Vue, but it's just a build tool), (b) plain GitHub Pages with Jekyll, (c) Astro Starlight. **Default**: Vitepress (best DX for nav, search, code blocks, and dark mode out-of-the-box). Add to `pnpm-workspace.yaml` as a `docs/` package.

9.5.2 — `docs/` content scaffold:
  - `index.md` — landing: what it is + screenshot of host view
  - `usage/host.md` — creating a room, the admin link, tree, Q&A, whiteboards, raise-hand, moderation (each with screenshots)
  - `usage/guest.md` — joining, asking questions, voting, raising hand, viewing boards
  - `deployment/railway.md` — one-click style: fork → connect Railway → set env → deploy
  - `deployment/self-host.md` — Dockerfile + volume + reverse proxy notes
  - `architecture.md` — link to the planning tree + high-level system diagram
  - `contributing.md` — dev quickstart, TDD norms, review workflow

9.5.3 — **Screenshot harvest pipeline**: an e2e suite named `docs-screenshots.spec.ts` drives the app through curated states (empty room, mid-session, Q&A active, pen board with content, Excalidraw board, raise-hand queue, dark mode of each). It saves PNGs to `e2e/screenshots/_docs/`. `scripts/docs-build.sh` copies them to `docs/public/screenshots/` (or framework equivalent) before the build.

9.5.4 — `just docs-dev` (vitepress dev server) and `just docs-build` (static output to `docs/.vitepress/dist/`).

9.5.5 — `.github/workflows/pages.yml`:
  - Triggers on push to `main` *and* on manual dispatch.
  - Runs: `just build` (so the app exists for the screenshot suite), `just test-e2e-only docs-screenshots.spec.ts` to refresh screenshots, `just docs-build`, then `actions/upload-pages-artifact` + `actions/deploy-pages`.
  - Sets the repo's Pages source to "GitHub Actions" (one-time setup; documented in `scripts/gh-repo-meta.sh`).

9.5.6 — Add a "Docs" link to the app's landing page topbar pointing at the Pages URL.

**Tests**

- Build: `just docs-build` produces non-empty `dist/`.
- E2E (already part of `docs-screenshots.spec.ts`): assert each curated screenshot is produced and non-zero size.
- CI: `pages.yml` is dry-runnable locally via `act` for sanity.

**Review gate**: Track A (docs site is a fresh UI surface — full visual review) + Track B.

---

## Phase 9.9 — Final steps: repo metadata via `gh`

**Goal**: Polished GitHub repository — description, homepage URL, topics, social preview, default branch protection.

**Key tasks**

9.9.1 — `scripts/gh-repo-meta.sh` (idempotent):
  ```
  gh repo edit clankercode/topic-tree-with-qav \
    --description "Real-time host-audience interaction: topic tree, Q&A with voting, smooth whiteboards, raise-hand. Single Rust binary + React + SQLite, runs anywhere." \
    --homepage "https://clankercode.github.io/topic-tree-with-qav/" \
    --add-topic real-time --add-topic webrtc-alternative --add-topic websockets \
    --add-topic axum --add-topic rust --add-topic vite --add-topic react \
    --add-topic excalidraw --add-topic whiteboard --add-topic q-and-a \
    --add-topic presentations --add-topic teaching --add-topic open-source
  ```
  Wrap and call from `just gh-meta`.

9.9.2 — Social preview image: render a 1280×640 PNG via Playwright from a docs-only page; upload via the GitHub API (the `gh` CLI lacks a direct social-preview command — use `gh api -X PATCH /repos/...` with the right field, or set it once in the GitHub web UI and document the manual fallback).

9.9.3 — Default branch protection: require CI green + 1 review on PRs. `gh api -X PUT /repos/clankercode/topic-tree-with-qav/branches/main/protection` with the standard JSON body. Skip if working solo; gate behind a `--with-protection` flag on `just gh-meta`.

9.9.4 — Issue templates + PR template under `.github/`. Minimal — one bug, one feature, one PR template.

9.9.5 — Confirm Railway production URL + GitHub Pages URL are both in the README, the docs site, and the repo description.

**Acceptance**

- `gh repo view clankercode/topic-tree-with-qav` shows the description, homepage, and topics.
- Repo's "About" sidebar on github.com lists all topics.

**Review gate**: none — pure metadata. Just sanity-check the rendered repo page in a browser.

---

## Cross-cutting

- **Every task ends with a commit**. Commit message style: `feat(scope): summary`, `fix(scope): summary`, `test(scope): …`, `refactor(scope): …`. Scope examples: `topictree`, `qa`, `whiteboard-pen`, `ws`, `db`, `ui`.
- **Every commit triggers** `postcommit-status-and-continue`.
- **Per-phase status entry** appended to `.plan/STATUS.md`.
- **Worktree usage**: optional, recommended for phases 4-5 since they touch a lot of files; the rest can run on a single branch.
