# `ws.rs` refactor — target layout, helpers, ordering

## Status today

`server/src/ws.rs` is **2,375 LOC**. The bulk is `handle_text`, a single match arm per intent with repeated host-only / muted / ack-if-id boilerplate. Server reviewer finding #20 in the 2026-05-25 round flagged it.

## Goal

Split into:

- `server/src/ws/mod.rs` — connection lifecycle + match dispatch + the broadcast plumbing. Target ~600 LOC.
- `server/src/ws/helpers.rs` — three thin helpers below.
- `server/src/intents/` — one file per intent area, each exposing `pub async fn handle(ctx, intent) -> Result<()>`.

## Helper signatures (`server/src/ws/helpers.rs`)

```rust
/// Returns Err with a server-side `Error{code:"forbidden"}` if the
/// session is not a host. Caller short-circuits on Err.
pub fn ensure_host(session: &SessionCtx) -> Result<(), IntentError>;

/// Returns Err with `Error{code:"muted"}` if moderation has the guest
/// muted. Hosts bypass.
pub fn ensure_not_muted(session: &SessionCtx, room: &Room) -> Result<(), IntentError>;

/// If the intent payload includes an `id`/`refId`, emit `Ack{refId}` on
/// the session's sink. No-op if the intent didn't carry one.
pub async fn ack_if_id(session: &SessionCtx, intent_ref_id: Option<&str>);
```

`IntentError` is a thin newtype wrapping `Error` payload + a flag indicating whether to close the socket.

## `SessionCtx`

Every intent handler takes a `&SessionCtx` (or `&mut`, where stateful):

```rust
pub struct SessionCtx<'a> {
    pub session: &'a Session,         // role, guest_id, display_name, sink
    pub room:   &'a Arc<Room>,        // in-mem state + broadcast tx
    pub state:  &'a AppState,         // db, writer_tx, metrics
}
```

This avoids each handler taking a sprawling argument list.

## Per-area modules

| Module | Intents handled |
|---|---|
| `intents/topics.rs` | `AddTopic`, `RenameTopic`, `MoveTopic`, `SetTopicStatus`, `DeleteTopic`, `SetActiveTopic` |
| `intents/questions.rs` | `SubmitQuestion`, `MarkAnswered`, `DeleteQuestion`, `VoteQuestion`, `PromoteQuestionToTopic` |
| `intents/pen.rs` | `AddPenBoard`, `PenStrokeBegin`, `PenStrokeAppend`, `PenStrokeEnd`, `PenTextUpsert`, `PenTextDelete`, `PenClear`, `PenUndo` |
| `intents/excalidraw.rs` | `AddExcalidrawBoard`, `ExcalidrawUpdate`, `ExcalidrawPointer` |
| `intents/moderation.rs` | `KickGuest`, `UnkickGuest`, `MuteGuest`, `UnmuteGuest` |
| `intents/raise_hand.rs` | `RaiseHand`, `LowerHand` |
| `intents/presence.rs` | `CursorMove`, `ClickPing`, `RenameRoom`, `SetFocusedBoard` |

Each handler returns `Result<(), IntentError>`. The lifecycle layer maps the error to either an `Error` payload + continue, or `Error` + close, depending on `IntentError::should_close()`.

## `ws/mod.rs` shape (sketch)

```rust
pub async fn handle_text(ctx: &mut SessionCtx<'_>, raw: &str) -> Result<(), IntentError> {
    let intent: ClientMsg = serde_json::from_str(raw).map_err(IntentError::bad_payload)?;
    match intent {
        ClientMsg::Topic(i)      => intents::topics::handle(ctx, i).await,
        ClientMsg::Question(i)   => intents::questions::handle(ctx, i).await,
        ClientMsg::Pen(i)        => intents::pen::handle(ctx, i).await,
        ClientMsg::Excalidraw(i) => intents::excalidraw::handle(ctx, i).await,
        ClientMsg::Moderation(i) => intents::moderation::handle(ctx, i).await,
        ClientMsg::RaiseHand(i)  => intents::raise_hand::handle(ctx, i).await,
        ClientMsg::Presence(i)   => intents::presence::handle(ctx, i).await,
        ClientMsg::Hello(_)      => Err(IntentError::misuse("hello not allowed mid-session")),
    }
}
```

The exact enum grouping depends on the existing `proto.rs` layout — if `ClientMsg` is flat today, prefer to keep it flat and group inside `match` arms instead.

## LOC math (approximate)

| File | Before | After |
|---|---|---|
| `ws.rs` (mod root) | 2,375 | ~600 |
| `ws/helpers.rs` | — | ~80 |
| `intents/topics.rs` | — | ~250 |
| `intents/questions.rs` | — | ~250 |
| `intents/pen.rs` | — | ~400 |
| `intents/excalidraw.rs` | — | ~150 |
| `intents/moderation.rs` | — | ~120 |
| `intents/raise_hand.rs` | — | ~120 |
| `intents/presence.rs` | — | ~200 |
| **Total** | **2,375** | **~2,170** |

The total is **slightly smaller**, not radically — the helpers eliminate ~200 LOC of boilerplate. The win is structural, not LOC. Each file is independently reviewable.

## Ordering vs F1

**F5 sequences after F1**. F1 modifies every intent handler to enqueue `WriteOp`s; doing F5 first means every F1 commit later touches many small new files. Mechanical merge but expensive.

If F1 is in-flight when F5 starts, F5 must rebase on F1 — not the other way around.

## Behavioural invariants the refactor must preserve

1. **No new client-visible behavior**. F4 integration tests are the regression net.
2. **Same error semantics**. `ensure_host` must emit the same `Error{code:"forbidden"}` payload as today; `ensure_not_muted` same for `code:"muted"`.
3. **Same broadcast ordering**. Intent applied → broadcast → ack. The refactor must not reorder these. If a current handler acks before broadcasting (or vice versa), keep that order in the new module.
4. **Same rate-limit gates**. Per-intent rate-limit calls must move with their intent into the new module. Don't centralise into the dispatcher unless the gate is genuinely per-message-type-agnostic.
5. **Same broadcast scope**. Some intents broadcast room-wide, others only to specific clients (host-only acks). The refactor preserves scope.

## Dispatch

- `ccc-coder-cx` drives the mechanical split, one area at a time.
- After each commit, run `just test-server` locally and dispatch `ccc-review-cx` against the diff: prompt = "structural-only refactor; flag any behavioural drift".
- After all areas land, dispatch a Claude subagent `superpowers:requesting-code-review` against the cumulative refactor diff for a final pass.
