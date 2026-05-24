# WebSocket Protocol

JSON over WS. Each frame is one envelope. Versioned by `v`.

## Envelope

```ts
type ClientMsg = { v: 1; id?: string; type: string; ...payload };
type ServerMsg = { v: 1; type: string; ts: number; ...payload };
```

- `id` is an optional client-generated correlation id; server echoes in the ack/error response.
- `ts` (ms epoch) is set by server on outbound messages.

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
| `VoteQuestion` | `{questionId, vote: 1\|-1\|0}` | guest |
| `AddTopic` | `{parentId?, title, afterId?}` | admin |
| `RenameTopic` | `{topicId, title}` | admin |
| `MoveTopic` | `{topicId, newParentId?, afterId?}` | admin |
| `DeleteTopic` | `{topicId}` | admin |
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
| `ExcalidrawUpdate` | `{boardId, sceneVersion, elements, appState?}` | admin |
| `Cursor` | `{boardId, x, y}` | any |
| `Click` | `{boardId, x, y}` | any |
| `KickGuest` | `{guestId}` | admin |
| `MuteGuest` | `{guestId, muted:bool}` | admin |
| `RaiseHand` | `{topic: string}` (1-10 words, server enforces ≤80 chars + word count) | guest |
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
| `VoteUpdated` | `{questionId, voteCount}` |
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
| `ExcalidrawDelta` | `{boardId, sceneVersion, elements}` |
| `CursorMoved` | `{boardId, clientId, x, y}` |
| `Clicked` | `{boardId, clientId, x, y}` |
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
  boards: Board[];           // each board includes its full content
  focusedBoardId: string | null;
};
```

## Auth rules

- `adminToken` is verified server-side per message that requires admin. Token never leaves server-side validation logic.
- A connection's role is locked at `Hello`; switching requires reconnect.
- Token leak mitigation: when the host opens the admin URL once, the client immediately strips `?admin=...` from the address bar (`history.replaceState`) and stores token in IndexedDB. Subsequent loads use the stored token.

## Rate limits (server-enforced, per-client)

| Message | Rate |
|---|---|
| `Cursor` | 30 msg/s; excess dropped silently |
| `Click` | 5 msg/s |
| `PenStrokeAppend` | 60 msg/s |
| `SubmitQuestion` | 6 msg/min |
| `VoteQuestion` | 30 msg/min |
| `RaiseHand` | 2 msg/min (lower → raise → lower → raise spam guard) |
| All others | 20 msg/s blanket |

Exceeding sustained limit → `Error{code:"rate_limit"}` and the offending message is dropped.

## Versioning

- `v:1` for entire v1. Breaking changes bump to `v:2` and server keeps a one-version compatibility shim during transition.
