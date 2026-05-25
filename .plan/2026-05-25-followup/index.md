# 2026-05-25 follow-up — close remaining gaps after the review-and-fix landing

## Status snapshot

- Parent plan: `.plan/2026-05-24-amber-falcon/` (`amber-falcon`) — covers architecture, protocol, data model, frontend, whiteboards, deployment, original phases.
- Anchor commit: `c14487c` (last commit of the 2026-05-25 review-and-fix session). Cumulative diff for this follow-up is `2efafbb..HEAD-after-F8`.
- Codex re-review at session close: **PASS**, no CRITICAL/HIGH findings.
- This tree owns the deferred-but-required work: persistence, idle reaper, integration tests, whiteboard React loop, visual-regression infra, frontend polish bundle, and the `ws.rs` refactor.

## Doc map

| Doc | Purpose | Load-bearing |
|---|---|---|
| `phases.md` | F0–F8 spec with full TDD task lists per phase | yes |
| `persistence.md` | Canonical `WriteOp` enum (source of truth — any new intent adds a row here first), batching policy, shutdown handling, writer-connection ownership decision, V0005/V0006 schema deltas, hydration query, sequencing diagram | **yes** |
| `testing.md` | F0 harness contract; 9 named integration tests with names, setup, assertions; F7 visual-regression contract | yes |
| `risks.md` | R21 status update (reaper scheduled); new risks (write-loss on shutdown, hydration latency on cold room, refactor regression, React-loop recurrence) | yes |
| `ws-refactor.md` | Target module layout, helper signatures, before/after LOC math, ordering relative to F1 | yes |
| `frontend-followups.md` | G.1–G.6 details + per-sub-task acceptance | yes |
| `agents-workflow.md` | Per-phase dispatch table | reference |

No new `architecture.md` or `data-model.md` — both live in `2026-05-24-amber-falcon/` and `persistence.md` references them by relative path.

## Phase table

| Phase | Goal | Size | Critical path | Sequence with |
|---|---|---|---|---|
| F0 | Integration harness skeleton (ws smoke test using `tokio-tungstenite` against real Axum app) | S | yes | — |
| F1 | Persistence write path (single-writer task + `WriteOp` enum, V0005/V0006 migrations) | L | yes | parallel: F6, F7, F8 |
| F2 | Rehydration in `RoomRegistry::get_or_create` | M | yes | after F1 |
| F3 | Idle reaper + `last_activity_at` | S | yes | after F2 |
| F4 | Backend integration tests (9 named per `testing.md`) | L | yes | parallel: F5, F7, F8 |
| F5 | `ws.rs` refactor — split `handle_text` into `intents/*` modules | M | cleanup | after F1 |
| F6 | Whiteboard React #185 (`investigate-and-fix`) | M | parallel | runs from F1 |
| F7 | Visual-regression infrastructure | M | parallel | runs from F1 |
| F8 | Frontend polish bundle (G.1 – G.6) | L | parallel | runs from F1 |

Sizes: S = ≤1 day, M = ~2 days, L = multi-day.

## Critical path

`F0 → F1 → F2 → F3 → F4`. The other phases (F5, F6, F7, F8) are independent enough to start in parallel once F0 is in.

## Done-when

End-to-end checks before declaring the plan done:

1. `just lint` clean.
2. `just test-web` 100% green.
3. `just test-server` covers existing unit tests **plus** the 9 named integration tests from F4, all green.
4. `just test-e2e` green including `whiteboard.spec.ts` (currently 5 pre-existing failures, must be 0 after F6).
5. `scripts/check-snapshot-pairs.sh` passes against committed baselines.
6. Manual smoke: `just serve-test`, create a room, add topics + questions + a pen stroke, restart the binary, reconnect — state survives. Idle 11 min with no clients — room is reaped on the next reaper tick (or force-reap via a test-only endpoint).
7. Final review cycle: parallel `ccc-review-cx` + `gpt-pro-run-review-dc` + Claude subagent code review of the cumulative `2efafbb..HEAD-after-F8` diff. All return PASS.

## Source-of-truth note

This file is the authoritative roadmap for the follow-up. If a phase's scope or acceptance criteria change mid-implementation, update **this** file and the relevant load-bearing doc in the same commit as the code change.
