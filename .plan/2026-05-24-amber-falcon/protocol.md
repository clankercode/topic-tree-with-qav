# WebSocket Protocol

JSON over WS. Each frame is one envelope. Versioned by `v`.

## Envelope

```ts
type ClientMsg = { v: 1; id?: string; type: string; ...payload };
type ServerMsg = { v: 1; type: string; ts: number; seq: number; ...payload };
```

- `id` is an optional client-generated correlation id; server echoes in the ack/error response.
- `ts` (ms epoch) is set by server on outbound messages.
- `seq` (u64) is a per-room monotonic counter set by server. See *Sequence numbering + gap detection* below.

## Type definitions

All shapes below are defined as Rust `#[derive(Serialize, Deserialize, TS)]` structs in `server/src/proto.rs` and the TS types are generated via `ts-rs`. Camel-cased JSON (serde `rename_all = "camelCase"`) on the wire; snake_case in Rust.

```rust
struct Guest { guest_id: String; display_name: String; muted: bool; joined_at: i64 }
struct Topic { id: String; parent_id: Option<String>; title: String; ord: f64; status: TopicStatus; created_at: i64 }
enum   TopicStatus { Pending, Done }
struct Question {
  id: String; room_id: String;
  author_guest_id: String;        // "" on outbound when anonymous
  author_name: String;            // "Anonymous" on outbound when anonymous
  anonymous: bool; text: String; answered: bool;
  created_at: i64; vote_count: u32;
}
struct Board { id: String; kind: BoardKind; title: String; created_at: i64; ord: f64 }
enum   BoardKind { Pen, Excalidraw }
struct PenText { id: String; x: f64; y: f64; text: String; font_size: f64; color: String; updated_at: i64 }
struct PenStrokeSummary { id: String; color: String; size: f64; points: Vec<[f32; 3]>; created_at: i64; ord: u32 }
struct ExcalidrawScene { board_id: String; scene_version: u64; elements: JsonValue; app_state: JsonValue }
struct RaisedHand { guest_id: String; display_name: String; topic: String; raised_at: i64 }
struct Cursor { board_id: String; client_id: String; x: f64; y: f64; ts: i64 }
struct Presence { guest_id: String; display_name: String; muted: bool; joined_at: i64; client_ids: Vec<String> }
```

Boards inside `RoomSnapshot.boards` are *fat*: a Pen board carries its `strokes` + `texts`, an Excalidraw board carries its `scene_version` + `elements` + `app_state`.

## Connection lifecycle

```
1. Client opens WS:  /ws?room=<roomId>
2. Client sends:     {type:"Hello", role:"host"|"guest", guestId, displayName?, adminToken?}
3. Server replies:   {type:"Welcome", you:{clientId,role}, snapshot:{...}}
4. Heartbeat:        server sends {type:"Ping"} every 25s; client replies {type:"Pong"}.
                     Either side disconnects after 60s silence.
5. Reconnect:        client retries with exponential backoff (1s, 2s, 4s, ... max 30s).
                     On Welcome, server re-sends snapshot.
```

## Client → Server

| Type | Payload | Auth |
|---|---|---|
| `Hello` | `{role, guestId, displayName?, adminToken?}` | implicit |
| `SetDisplayName` | `{name}` | guest self |
| `SubmitQuestion` | `{text, anonymous}` | guest |
| `VoteQuestion` | `{questionId, vote: bool}` (true = upvote, false = retract) | guest |
| `AddTopic` | `{parentId?, title, afterId?}` | admin |
| `RenameTopic` | `{topicId, title}` | admin |
| `MoveTopic` | `{topicId, newParentId?, afterId?}` | admin |
| `DeleteTopic` | `{topicId}` | admin |
| `ImportTopicTree` | `{parentTopicId?, topics}` | admin |
| `SetActiveTopic` | `{topicId\|null}` | admin |
| `MarkTopicDone` | `{topicId, done:bool}` | admin |
| `MarkQuestionAnswered` | `{questionId, answered:bool}` | admin |
| `DeleteQuestion` | `{questionId}` | admin |
| `CreateBoard` | `{kind: "pen"\|"excalidraw", title?}` | admin |
| `RenameBoard` | `{boardId, title}` | admin |
| `DeleteBoard` | `{boardId}` | admin |
| `SetFocusedBoard` | `{boardId}` | admin |
| `PenStrokeBegin` | `{boardId, strokeId, color, size}` | admin |
| `PenStrokeAppend` | `{boardId, strokeId, points: [[x,y,pressure],...]}` | admin |
| `PenStrokeEnd` | `{boardId, strokeId}` | admin |
| `PenTextSet` | `{boardId, textId, x, y, text, fontSize, color}` | admin |
| `PenTextDelete` | `{boardId, textId}` | admin |
| `PenClear` | `{boardId}` | admin |
| `PenUndo` | `{boardId}` | admin |
| `ExcalidrawUpdate` | `{boardId, sceneVersion, elements, appState}` | admin |
| `Cursor` | `{boardId, x, y}` | any |
| `Click` | `{boardId, x, y}` | any |
| `KickGuest` | `{guestId}` | admin |
| `MuteGuest` | `{guestId, muted:bool}` | admin |
| `RaiseHand` | `{topic: string}` (1-10 words, server enforces ≤80 chars + word count via Unicode whitespace split) | guest |
| `LowerHand` | `{}` | guest self |
| `CallOnHand` | `{guestId}` | admin |
| `DismissHand` | `{guestId}` | admin |
| `PromoteQuestionToTopic` | `{questionId, parentTopicId?, afterTopicId?}` | admin |
| `GetSnapshot` | `{since?: ts}` | any |
| `Pong` | `{}` | any |

## Server → Client

Each event includes only the delta unless noted.

| Type | Payload |
|---|---|
| `Welcome` | `{you:{clientId,role}, snapshot: RoomSnapshot}` |
| `Error` | `{code, message, refId?}` |
| `Ack` | `{refId}` |
| `PresenceUpdate` | `{guests: [{guestId,displayName,muted,joinedAt}]}` |
| `QuestionAdded` | `{question: Question}` |
| `QuestionUpdated` | `{question: Question}` |
| `QuestionDeleted` | `{questionId}` |
| `VoteUpdated` | `{questionId, voteCount, voterGuestId}` (clients track their own vote: if `voterGuestId == myGuestId`, the action toggled my vote) |
| `TopicTreeUpdated` | `{topics: Topic[], activeTopicId}` (full tree — small enough; simpler than diffs) |
| `BoardCreated` | `{board: Board}` |
| `BoardUpdated` | `{board: Board}` |
| `BoardDeleted` | `{boardId}` |
| `FocusedBoardChanged` | `{boardId}` |
| `PenStrokeBegun` | `{boardId, strokeId, color, size, authorClientId}` |
| `PenStrokeAppended` | `{boardId, strokeId, points}` |
| `PenStrokeEnded` | `{boardId, strokeId}` |
| `PenTextUpserted` | `{boardId, text: PenText}` |
| `PenTextDeleted` | `{boardId, textId}` |
| `PenCleared` | `{boardId}` |
| `PenUndone` | `{boardId, removedStrokeId\|removedTextId}` |
| `ExcalidrawDelta` | `{boardId, sceneVersion, elements, appState}` |
| `ExcalidrawSceneReset` | `{boardId, sceneVersion, elements, appState}` (server-initiated periodic anti-drift snapshot; clients replace state wholesale) |
| `CursorMoved` | `{boardId, clientId, guestId, displayName, x, y}` |
| `Clicked` | `{boardId, clientId, guestId, displayName, x, y}` |
| `KickNotice` | `{}` (sent to the kicked client just before close) |
| `HandsUpdated` | `{hands: [{guestId, displayName, topic, raisedAt}]}` (replaces full queue; ephemeral, not persisted) |
| `QuestionPromotedToTopic` | `{questionId, topic: Topic}` (clients remove question from Q&A list and insert topic into tree) |
| `Ping` | `{}` |

## Snapshot shape (sent in `Welcome`)

```ts
type RoomSnapshot = {
  room: { id: string; title: string; createdAt: number };
  you: { clientId: string; role: "host" | "guest"; guestId: string };
  guests: Guest[];
  topics: Topic[];
  activeTopicId: string | null;
  questions: Question[];
  myVotes: string[];                 // questionIds I've voted on — drives "already voted" UI on reconnect
  boards: Board[];                   // each board includes its full content (strokes/texts or scene)
  focusedBoardId: string | null;
  hands: RaisedHand[];               // current raised-hand queue (ephemeral)
};
```

## Auth rules

- `adminToken` is verified **once during `Hello`** (one argon2id check). The result is cached on the connection's session state as `role = host | guest`. Subsequent admin messages are gated by the cached role — no per-message rehashing.
- Role is immutable for the life of the connection. Switching requires reconnect with a different `Hello`.
- Tampered intents (a `guest` connection sending an admin-only message) are dropped with `Error{code:"forbidden"}`.
- Token leak mitigation: when the host opens the admin URL once, the client immediately strips `?admin=...` from the address bar (`history.replaceState`) and stores token in IndexedDB. Subsequent loads use the stored token.
- **Role vocabulary**: the wire uses `"host"` and `"guest"` everywhere — ws `Hello.role`, server snapshot `you.role`, and IndexedDB `RoomRecord.role`. The credential itself is called `adminToken` because it's a credential, not a role label.

## Sequence numbering + gap detection

- Server attaches a monotonic per-room `seq: u64` to every `ServerMsg` (in addition to `ts`).
- Clients track the last-seen `seq`. If a received `seq` is not exactly previous + 1, the client requests `GetSnapshot` to resync.
- `Welcome` carries the current high-water `seq`; subsequent broadcasts increment from there.

## Rate limits (server-enforced, per-client)

| Message | Rate |
|---|---|
| `Cursor` | 30 msg/s; excess dropped silently |
| `Click` | 5 msg/s |
| `PenStrokeAppend` | 60 msg/s |
| `SubmitQuestion` | 6 msg/min |
| `VoteQuestion` | 30 msg/min |
| `RaiseHand` | 2 msg/min (lower → raise → lower → raise spam guard) |
| `ImportTopicTree` | 6 msg/min with a 1-message burst |
| All others | 20 msg/s blanket |

Exceeding sustained limit → `Error{code:"rate_limit"}` and the offending message is dropped.

## Versioning

- `v:1` for entire v1. Breaking changes bump to `v:2` and server keeps a one-version compatibility shim during transition.
