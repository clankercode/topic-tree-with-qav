# Testing strategy

Four layers, each with a clear scope. **TDD is the default**: red → green → refactor on every task.

| Layer | Scope | Tooling | Speed budget |
|---|---|---|---|
| Unit (frontend) | Pure logic, reducers, hooks, throttle, idb helpers | Vitest + jsdom + @testing-library/react | < 5s full suite |
| Unit (backend) | Pure logic, parsers, auth helpers, validators | `cargo test` | < 10s full suite |
| Integration (backend) | One full Axum server in-process + a fake ws client | `cargo test` w/ `axum-test`, `tokio-tungstenite` | < 30s full suite |
| E2E | Real chrome(ium) driving real built UI against real binary | Playwright Test, multi-browser-context | < 5 min full suite |
| Visual regression | Pixel-diff vs baseline screenshots | Playwright `toHaveScreenshot` | included in e2e budget |

## 1. TDD discipline

Per `superpowers:test-driven-development` (and `ultra-test-driven-development` if planning depth warrants):

1. Write a failing test that asserts the *behavior* you want.
2. Run it. Confirm it fails for the *right reason* (not a compile error).
3. Write the minimum production code to pass.
4. Run. Confirm pass.
5. Refactor for clarity; tests stay green.
6. Commit.

Never write production code without a failing test first. If a refactor risks breaking something the tests don't cover, add the test before the refactor.

## 2. Frontend unit tests

Examples (each gets a real test file):

- `store/questions.ts`
  - "adding a question appends it to the list"
  - "voting toggles user's vote and updates count"
  - "sortByVotes ranks higher-vote first; ties broken by createdAt asc"
- `lib/throttle.ts`
  - "calls leading + trailing only"
  - "drops mid-window calls"
- `ws/reducer.ts`
  - "QuestionAdded merges into store"
  - "TopicTreeUpdated replaces tree entirely"
- `qa/AutoscrollLock.tsx`
  - "engages when scrolled away from bottom"
  - "disengages when scrolled within 50px of bottom"

## 3. Backend integration tests

For each protocol message: spin up the server with an in-memory SQLite (`:memory:`), connect a real ws client, send the message, assert the broadcast.

Examples:

- `create_room_returns_admin_token_and_room_id`
- `hello_with_invalid_admin_token_returns_error`
- `submit_question_broadcasts_to_all_clients_in_room`
- `vote_question_dedups_by_guest_id`
- `set_active_topic_marks_previous_active_as_done`
- `pen_stroke_lifecycle_persists_and_replays_on_reconnect`
- `excalidraw_update_from_guest_is_rejected_when_view_mode`
- `cursor_messages_exceeding_rate_limit_are_dropped`
- `kicked_guest_cannot_reconnect_until_room_unblocks`

## 4. E2E (Playwright)

Each spec opens N browser contexts (one per simulated user) against the built binary. Two patterns:

**Pattern A — bring-up per spec**: `webServer` in `playwright.config.ts` runs `just serve-test` (random port, in-memory SQLite, debug logging). Each spec gets a fresh server.

**Pattern B — bring-up shared**: a global setup boots one server, specs run in parallel against it. Use only for read-only specs. Default to pattern A.

### Key scenarios

1. **Room lifecycle** — host creates room, sees admin URL, reloads page, still authed; opens guest URL in another context, joins with name, both see each other in presence.
2. **Topic tree** — host adds topic, renames, moves; guest sees the changes <500ms after host commit; host sets active topic; guest sees badge update.
3. **Q&A flow** — guest asks question, second guest votes, third guest asks anonymously, host marks answered; resort-by-votes button reorders correctly; autoscroll lock engages when guest scrolls away and disengages on return.
4. **Pen whiteboard** — host draws a curve; guest sees the same curve appear with bounded latency; host adds text, undoes; guest sees the text disappear.
5. **Excalidraw whiteboard** — host creates rectangle and arrow; guest sees them; guest's Excalidraw is read-only (assertion on toolbar absence + cannot drag elements).
6. **Cursors + clicks** — three guests + host; all visible cursors update; click pings render on all clients.
7. **Focused board + follow** — host switches focused board; following guests follow; non-following guest stays.
8. **Moderation** — host kicks guest; that guest's connection closes; rejoin attempt fails.
9. **Reconnect** — drop ws (network simulator), confirm client reconnects and resyncs snapshot.

### Multi-client setup helper

`e2e/helpers/room.ts`:

```ts
export async function newRoom(browser: Browser) {
  const host = await browser.newContext();
  const hostPage = await host.newPage();
  // ... create room, capture roomId + adminToken from network
  return { host, hostPage, roomId, joinUrl };
}

export async function joinGuest(browser: Browser, joinUrl: string, name: string) {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await page.goto(joinUrl);
  await page.getByLabel('Display name').fill(name);
  await page.getByRole('button', { name: 'Join' }).click();
  return { ctx, page };
}
```

## 5. Visual regression

- Snapshots: `e2e/screenshots/<spec>/<step>.png`.
- Granularity: per-feature key views (landing, host view empty, host view loaded, guest view, mobile guest view), in both light and dark theme.
- Tolerance: `maxDiffPixelRatio: 0.005`, animations disabled via `await page.emulateMedia({ reducedMotion: 'reduce' })`.
- Update workflow: `just snapshot-update` runs Playwright with `--update-snapshots`; commit changed PNGs.

### Anti-flake

- Wait on network idle + an explicit "ready" data-attr on the root component.
- Freeze time: server respects `TEST_FIXED_NOW=<epoch_ms>` env to produce deterministic timestamps in snapshots.
- Hide ephemeral text (presence count animation) under `data-testid="hide-in-snapshots"` and add a Playwright stylesheet that hides those.

## 6. Multi-agent review loop — visual reviewers **AND** code reviewers in parallel

The user-mandated polish gate. Run at the end of every UI-touching phase, and whenever a phase introduces non-trivial code surface that benefits from independent review.

Two review tracks run in parallel each round:

### Track A — visual review (UI-touching phases only)

Dispatch in parallel:
- **Opus 4.7 visual reviewer** (subagent_type=general-purpose, model=opus): screenshots + design language doc → punch list ranked by severity. Prompt: "be specific (low contrast on X, inconsistent padding on Y), reference design doc, avoid taste-only nits."
- **GPT visual reviewer** via `gpt-pro-run-review-dc` skill: same inputs, ChatGPT Pro with decisive-criticism framing → PASS / PARTIAL-PASS / FAIL + punch list.

Synthesize both punch lists; items raised by both = top priority; one-source items tie-broken by leader.

### Track B — code review (every phase)

Dispatch in parallel against the current diff:
- **`/ccc-review-cx`** — Codex (gpt-5.5 class) via the `ccc` tool, reviews the diff.
- **Kimi review** — Moonshot Kimi via the `kimi` CLI, same prompt, same diff. Invocation pattern:
  ```bash
  kimi --print --yolo --thinking -p "Review the current branch diff against main for correctness, bugs, race conditions, error handling gaps, accessibility regressions, dead code, and API misuse. Produce a punch list ranked by severity. Spec: .plan/2026-05-24-amber-falcon/index.md" > .review/kimi-<phase>-<round>.md
  ```
  Wrapped by `just kimi-review` (script in `scripts/`). Runs in background so it doesn't block the leader.
- Both A-track agents (when applicable) are also added to the same background fan-out so all reviewers complete concurrently.

### Fix-and-loop

After both tracks return:

1. Merge all punch lists with provenance (which reviewer raised which item).
2. Drop pure-taste items, demote single-source items, promote multi-source items.
3. **Spawn an Opus 4.7 subagent running `review-and-fix`** with the merged list as input. The subagent walks each item, applies a fix, regenerates snapshots / re-runs tests, and re-checks.
4. After fixes commit, run the next round (back to step 1 of the loop) until exit.

### Exit conditions

- All reviewers (Opus visual, GPT-pro decisive, ccc/codex, kimi) report PASS or no new actionable items, OR
- Two consecutive rounds produced no new actionable items, OR
- User override.

### Why four reviewers

- **Diverse model families**: Anthropic (Opus), OpenAI (gpt-5.5 + Codex), Moonshot (Kimi). Independent failure modes; agreement = signal.
- **Visual + code split**: a visual reviewer can't catch race conditions; a code reviewer can't catch low contrast.
- **Decisive-criticism framing** on `gpt-pro-run-review-dc` aggressively surfaces issues a sycophantic reviewer would gloss over.

### Cost-management

- Code-only phases skip Track A.
- For small diffs (<200 LOC changed), drop to one code reviewer (ccc) — kimi as backup if ccc reports clean.
- Visual review batched per phase, not per task.

## 7. Functional / property tests

Where applicable (most useful for: vote dedup, fractional-index reorder, topic-tree status transitions, throttle correctness):

- Frontend: `fast-check` for prop tests.
- Backend: `proptest` crate.

Examples:

- `prop_topic_tree_set_active_makes_at_most_one_active`
- `prop_vote_count_equals_count_distinct_guest_ids`
- `prop_fractional_index_insert_between_always_strictly_between`

## 8. Manual playwright agent runs

For exploratory UI testing — agent uses `mcp__plugin_playwright_playwright__browser_*` tools to:

- Drive multi-tab flows that are awkward to encode as specs.
- Take screenshots for ad-hoc review.
- Probe error states by malforming inputs.

Trigger as needed; not part of CI.

## 9. CI

`.github/workflows/ci.yml`:

- `lint` job: `pnpm -C web typecheck && pnpm -C web lint && cargo clippy --all-targets -- -D warnings && cargo fmt --check`.
- `test-frontend`: `pnpm -C web test --run`.
- `test-backend`: `cargo test --workspace`.
- `test-e2e`: `just ci-e2e` (boots binary, runs Playwright headed=false on chromium, uploads diffs as artifacts).
- All four run in parallel. Required for merge.
