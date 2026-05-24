# Agents + skills workflow

How a Claude Code agent should execute this plan, what skills to invoke, and where the review loops sit.

## 1. Skill stack

| Skill | When to invoke |
|---|---|
| `superpowers:brainstorming` | At the *very* start, if any product question is still open. We've already locked decisions in `index.md` §2; skip if the user agrees no design questions remain. |
| `superpowers:writing-plans` | Already produced this plan tree. Re-invoke if a major change in scope. |
| `superpowers:subagent-driven-development` *(preferred)* **or** `use-subagents-impl` **or** `ultra-implementing-team` | Phase-by-phase execution. Dispatch one subagent per task. |
| `superpowers:test-driven-development` (or `ultra-test-driven-development`) | On every code-producing task. Red → green → refactor. |
| `superpowers:requesting-code-review` | Internal review request between subagent and leader for non-trivial changes. |
| `review-and-fix` | After visual+code reviewers return, an Opus 4.7 subagent runs this loop on the punch list. |
| `gpt-pro-run-review-dc` | Visual review track — ChatGPT Pro with decisive-criticism framing. |
| `ccc-review-cx` | Code review track — Codex via `ccc`. |
| `kimi --print --yolo --thinking -p "..."` | Code review track — Moonshot Kimi. Run via `just kimi-review`. |
| `frontend-design` | At start of phase 1 (initial visual identity) and again at phase 8 (polish). |
| `verification-before-completion` | Before any "done" claim. Evidence before assertions. |
| `postcommit-status-and-continue` | After every commit. Decides whether to continue, switch, or stop. **REQUIRED** per spec. |
| `heartbeat-monitor` | Long-running work (e2e suites, multi-agent reviews) — keep cache warm. |
| `repeat-via-checklist-after-commit` | When operating in a continuous-work loop. |
| `using-git-worktrees` | Optional — isolate exploratory phases. Not required for the linear plan. |

## 2. Per-phase rhythm

```
For each phase in phases.md:
  1. Read phase intro + acceptance criteria.
  2. Brainstorm only if unknowns remain (skip otherwise).
  3. Decompose phase into tasks (already done in phases.md).
  4. For each task:
       a. Dispatch a subagent (subagent-driven-development).
          - Subagent runs TDD: write failing test → minimum impl → green → refactor.
          - Subagent commits when task is green.
       b. Leader does a quick sanity diff review.
       c. After the commit: invoke postcommit-status-and-continue.
  5. End-of-phase review gate:
       a. Run full e2e suite locally.
       b. Track A (visual) — only if UI changed. Dispatch in parallel:
            - Agent(opus, "visual review against design language")
            - Skill(gpt-pro-run-review-dc, screenshots)
       c. Track B (code) — every phase. Dispatch in parallel:
            - Skill(ccc-review-cx)
            - just kimi-review (background)
       d. When all reviewers have returned, merge punch lists.
       e. Spawn an Opus 4.7 subagent running review-and-fix on the merged list.
            - It loops fix → re-test → re-screenshot → re-check until clean or stuck.
       f. Re-run all reviewers. Repeat until exit condition met.
  6. Final commit for the phase.
  7. postcommit-status-and-continue → next phase (or stop).
```

## 3. Subagent dispatch pattern

Standard prompt skeleton for a per-task subagent (subagent_type=general-purpose, model varies):

```
You are implementing one task from the topic-tree-with-qav plan.

Plan root: /home/xertrov/src/topic-tree-with-qav/.plan/2026-05-24-amber-falcon/
Read first: index.md, architecture.md, protocol.md, data-model.md (only the sections relevant to this task).

Task to implement:
  <task name + acceptance criteria + files to touch + test stub>

Rules:
- TDD discipline: red test first, then minimum impl. Confirm red, confirm green, refactor, commit.
- Use the justfile recipes for build/test/serve. Do not invent ad-hoc commands.
- Stay in scope of this task only. Do not refactor unrelated code.
- Do not introduce dependencies not approved in plan.
- On completion: confirm tests green via `just test-<scope>`, run `just lint`, commit with the message style in the plan.
- Return a one-paragraph report: what you did, files changed, what you noticed, anything surprising.
```

## 4. Parallel review dispatch — concrete shape

End-of-phase review (Phase N, round R):

```
Single message with N parallel Agent / Skill calls:
  1. Agent(opus, subagent_type=general-purpose, run_in_background=true,
           description="Visual review phase N round R",
           prompt="Review these screenshots against design language doc...")
  2. Skill(gpt-pro-run-review-dc, args="screenshots in .review/<phase>/<round>/")
  3. Skill(ccc-review-cx)              # blocking, fast
  4. Bash(just kimi-review PHASE=N ROUND=R, run_in_background=true)
```

Then wait (via Monitor or background notification) for the slow ones; merge punch lists.

## 5. Visual review prompt shape

Reused for both Opus and gpt-pro variants:

```
You are reviewing screenshots of a web app, "topic-tree-with-qav", a host-audience interaction tool.

Design language:
  - Modern, calm, focused. Content forward, chrome minimal.
  - Inter (UI) + JetBrains Mono (IDs). Comfortable density on desktop.
  - Neutral grayscale base, single indigo accent. No glassmorphism.
  - 150ms ease-out motion. Subtle shadows light / inner-stroke dark.

For each screenshot, identify issues in these categories (label each):
  1. Hierarchy + scan-ability
  2. Contrast + readability (call out WCAG concerns; AA target)
  3. Spacing + alignment
  4. Theming consistency (light + dark must match in structure)
  5. Distinctive design quality (does it look generic-AI or considered?)
  6. Interaction affordance (does the user know what's clickable?)
  7. Mobile (if a mobile shot is provided)

Output as a punch list. Each item:
  - severity: blocker | major | minor | nit
  - screenshot: filename + region
  - issue: one sentence
  - suggested fix: one sentence
Skip pure taste preferences.
```

## 6. Code review prompt shape

For ccc-review-cx and kimi:

```
Review the diff on the current branch against main for:
  - Correctness bugs (race conditions, off-by-one, lifetime issues, broken assumptions)
  - Error handling gaps at trust boundaries (user input, network)
  - Accessibility regressions (semantic HTML, focus, contrast, ARIA)
  - Dead code
  - API misuse of: axum, tokio, rusqlite, perfect-freehand, @excalidraw/excalidraw, zustand
  - Test coverage gaps for changed behavior

Reference docs:
  - .plan/2026-05-24-amber-falcon/index.md
  - .plan/2026-05-24-amber-falcon/protocol.md
  - .plan/2026-05-24-amber-falcon/data-model.md

Output as a punch list. Each item: severity, file:line, issue, suggested fix.
Skip nits unless they cluster.
```

## 7. Communication conventions

- The leader announces at each phase boundary: "Starting Phase N: <name>". And at each task: "Dispatching subagent for Task N.M".
- Reports from subagents are summarized into a one-line entry in `.plan/STATUS.md` (one line per task: phase, task, status, key note).
- Punch lists from reviewers persist in `.review/phase-N/round-R/<source>.md`.
- After phase end, status is collapsed: phase summary line in `.plan/STATUS.md`, punch-list dirs preserved.

## 8. Stop / continue decision (postcommit-status-and-continue)

After every commit, invoke the skill. Decision tree (the skill encapsulates this; here for reference):

- Are tests green? If no → **stop**.
- Are there uncommitted changes? If yes → resolve before continuing.
- Is the current phase complete? If no → next task.
- Is the next phase blocking on user input? If yes → **stop**.
- Otherwise → next phase.

### Stop output channel

When the decision is "stop", produce three artifacts in this order:

1. **Append a line to `.plan/STATUS.md`**: `YYYY-MM-DD HH:MM | phase=N | status=stopped | reason=<one phrase> | next=<what's needed>`. This is the durable audit log.
2. **Write the full handoff context** to `.plan/STATUS-DETAIL.md` (overwrite): the failing test output, the blocking question, the next concrete step. Future agent picks this up first when resuming.
3. **`attn "<msg>"`** via bash if and only if the user is configured for it (per `~/CLAUDE.md`). Phrase the message as: who you are, what phase/task, what's blocking, what you need. No symbols, no jargon — TTS-friendly.

For "continue" decisions, just write the STATUS.md line (no detail file, no `attn`).
