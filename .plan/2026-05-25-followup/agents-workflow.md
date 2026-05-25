# Per-phase agent dispatch

> All ChatGPT/gpt-pro tooling is explicitly **excluded** for this follow-up. Reviews go through Codex (`ccc-review-cx`), Codex investigators (`ccc-cx-investigator`), Codex coders (`ccc-coder-cx`), or Claude subagents.

## Dispatch table

| Phase | Implementation | Review |
|---|---|---|
| Pre-F1 design lock | `ccc-cx-investigator` on the `WriteOp` + writer ownership question (prompt anchored to `persistence.md` §3, §4, §6) | Parallel Claude subagent code-review of the same question for a second opinion |
| F0 | Single Claude implementer subagent | Claude spec-reviewer + Claude code-quality reviewer |
| F1 sub-phases | `superpowers:subagent-driven-development` — one implementer per sub-phase (F1.0 through F1.10). F1.3, F1.4, F1.6, F1.8, F1.9 can parallelise after F1.0–F1.2 land. | After each sub-phase: Claude spec-reviewer + Claude code-quality reviewer. At F1 boundary: parallel `ccc-review-cx` + Claude subagent review of the cumulative F1 diff. |
| F2 | Single Claude implementer subagent | Claude spec + quality reviewers |
| F3 | Single Claude implementer subagent | Claude spec + quality reviewers |
| F4 | `superpowers:dispatching-parallel-agents` — two batches (5 + 4 tests) | `ccc-review-cx` on the cumulative test diff |
| F5 | `ccc-coder-cx` per area module | `ccc-review-cx` after each commit ("structural-only refactor; flag any behavioural drift") + Claude subagent review of cumulative |
| F6 | `investigate-and-fix` (codex-driven hypothesis loop) | Result review by Claude subagent; `ccc-review-cx` on the final diff |
| F7 | Single Claude implementer subagent | Claude subagent reviewer (no gpt-pro visual-review step — we substitute with Claude visual-review on screenshots) |
| F8.1–F8.5 | Parallel Claude implementer subagents via `superpowers:dispatching-parallel-agents` | Per-sub-task: Claude spec + quality reviewers |
| F8.6 | `frontend-design` skill drives the CSS-var design; then Claude implementer subagent | Visual review via Claude subagent against the paired snapshots |
| Final verification | Leader (current session) | Parallel `ccc-review-cx` + Claude subagent code review of the full `2efafbb..HEAD-after-F8` diff |

## Standing dispatch contracts

### Implementer subagent prompt skeleton

```
Read this task and implement it. You are operating as a Claude subagent
inside the topic-tree-with-qav repo. Follow TDD: red test → minimum code →
green → refactor → commit.

Task: <full text from phases.md>

Context (curated for you — do not re-derive):
- Current repo state: <commit SHA, branch>
- Files you will touch: <list>
- Files you will NOT touch unless absolutely necessary: <list>
- Existing related tests: <list>
- Plan docs you may consult: <relative paths>
- Acceptance criteria: <copy from phases.md DoD>

Conventions:
- `just lint` must pass.
- `just test-server` must pass for backend tasks; `just test-web` for frontend.
- Commit message style: feat/fix/test/refactor/chore(scope): ...
- No new comments unless they explain WHY something non-obvious is happening.
- No new dependencies without explicit justification.

Return DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED.
```

### Reviewer subagent prompts

- **Spec compliance reviewer**: "Verify the implementation matches this task's spec exactly. Flag anything missing AND anything extra (over-build)."
- **Code quality reviewer**: "Review for correctness bugs, race conditions, error-handling gaps, dead code, API misuse. Sort findings by severity."

### `ccc-cx-investigator` prompt for the F1 design lock

```
Investigate the following design question and return a decision matrix
plus a recommendation:

Question: How should the single-writer SQLite task own its connection in
the topic-tree-with-qav server?

Constraints:
- The :memory: test mode forces pool size = 1 (in-memory SQLite databases
  are not sharable across connections). See server/src/db.rs and
  .plan/2026-05-24-amber-falcon/data-model.md §3.
- The read pool is r2d2-backed and serves all read-path handlers.
- The writer task must hold its connection long-term to enable batched
  transactions per .plan/2026-05-25-followup/persistence.md §1.
- The WriteOp enum is defined in persistence.md §3.

Options to evaluate:
(1) Db retains the path; writer opens a fresh Connection. Simple but
    doubles file handles and breaks :memory: mode.
(2) Db exposes clone_for_writer() -> Connection. In :memory: mode returns
    the pool's single connection; in file mode opens a fresh one.
(3) Writer holds an r2d2 pool checkout for its lifetime. Simplest but
    blocks one pool slot forever; breaks :memory: size-1.

Decision criteria:
- Correctness in :memory: mode.
- Surface area of the change (LOC, files touched).
- Implications for shutdown drain (must finish in-flight tx + close).
- Read throughput impact when the writer holds a long-running tx.

Output:
- Verdict: pick one of (1), (2), (3), or propose a 4th if warranted.
- Rationale: 3-5 sentences.
- Edge cases to test.
```

### `ccc-coder-cx` prompt skeleton for F5 mechanical splits

```
Extract <area> intent handlers from server/src/ws.rs into
server/src/intents/<area>.rs.

Constraints:
- No behavioural change. F4 integration tests are the regression net.
- Each handler returns Result<(), IntentError> per ws-refactor.md.
- Use the SessionCtx pattern per ws-refactor.md §SessionCtx.
- Move per-area rate-limit calls with their intent.
- Preserve broadcast/ack ordering exactly.

Steps:
1. Create the new file with the area's handlers.
2. Update ws.rs to dispatch into the new module.
3. Run `just test-server -j 2` and ensure all tests pass.
4. Commit: refactor(ws): extract <area> intent handler.

Do not touch unrelated code.
```

## Review loop at phase boundaries

At the end of F1, F2+F3 (combined), F4, F5:

1. Dispatch in parallel:
   - `ccc-review-cx` against the phase's cumulative diff.
   - Claude subagent code reviewer with the same diff + the relevant load-bearing doc.
2. Merge findings with provenance (which reviewer raised which item).
3. Dispatch a Claude `review-and-fix` subagent to walk the merged list.
4. Loop until both reviewers PASS or two consecutive rounds produce no new items.

## No-go list

- `gpt-pro-run-review-dc` — **excluded** by user instruction.
- `gpt-pro-send-prompt`, `gpt-pro-run-prompt`, `gpt-pro-initialize` — **excluded** by user instruction.
- `kimi-review` — not configured for this follow-up; omit unless the user later asks.

## Heartbeat

For long-running phases (F1, F4, F8), start `heartbeat-monitor` to keep the prompt cache warm.
