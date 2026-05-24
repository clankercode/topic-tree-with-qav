# Data model

## 1. SQLite schema (server, `/data/app.db`)

`PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;`

```sql
CREATE TABLE rooms (
  id              TEXT PRIMARY KEY,                -- 12-char b32 (url-safe)
  title           TEXT NOT NULL DEFAULT 'Untitled',
  admin_token_hash BLOB NOT NULL,                  -- argon2id of raw token
  created_at      INTEGER NOT NULL,
  last_active_at  INTEGER NOT NULL,
  active_topic_id TEXT,
  focused_board_id TEXT,
  settings_json   TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE topics (
  id          TEXT PRIMARY KEY,
  room_id     TEXT NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
  parent_id   TEXT REFERENCES topics(id) ON DELETE CASCADE,
  title       TEXT NOT NULL,
  ord         REAL NOT NULL,                       -- fractional indexing for cheap reorders
  status      TEXT NOT NULL DEFAULT 'pending',     -- pending | done
  created_at  INTEGER NOT NULL
);
CREATE INDEX idx_topics_room ON topics(room_id, parent_id, ord);

CREATE TABLE questions (
  id           TEXT PRIMARY KEY,
  room_id      TEXT NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
  author_guest_id TEXT NOT NULL,                   -- ALWAYS the real guest_id; blank only on outbound when anonymous=true
  author_name  TEXT NOT NULL,                      -- the real submitted name; replaced with "Anonymous" on outbound when anonymous=true
  anonymous    INTEGER NOT NULL DEFAULT 0,
  text         TEXT NOT NULL,
  answered     INTEGER NOT NULL DEFAULT 0,
  created_at   INTEGER NOT NULL
);
CREATE INDEX idx_questions_room ON questions(room_id, created_at);

CREATE TABLE question_votes (
  question_id TEXT NOT NULL REFERENCES questions(id) ON DELETE CASCADE,
  guest_id    TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (question_id, guest_id)
);

CREATE TABLE boards (
  id         TEXT PRIMARY KEY,
  room_id    TEXT NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL,                        -- pen | excalidraw
  title      TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  ord        REAL NOT NULL
);
CREATE INDEX idx_boards_room ON boards(room_id, ord);

CREATE TABLE pen_strokes (
  id         TEXT PRIMARY KEY,
  board_id   TEXT NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
  color      TEXT NOT NULL,
  size       REAL NOT NULL,
  points_json TEXT NOT NULL,                       -- [[x,y,pressure],...]
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_strokes_board ON pen_strokes(board_id);

CREATE TABLE pen_texts (
  id         TEXT PRIMARY KEY,
  board_id   TEXT NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
  x          REAL NOT NULL,
  y          REAL NOT NULL,
  text       TEXT NOT NULL,
  font_size  REAL NOT NULL DEFAULT 16,
  color      TEXT NOT NULL DEFAULT '#000000',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Unified action log per board for undo. Each mutation writes one row.
CREATE TABLE pen_actions (
  id         TEXT PRIMARY KEY,
  board_id   TEXT NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
  ord        INTEGER NOT NULL,                     -- monotonic per board
  kind       TEXT NOT NULL,                        -- "stroke_add" | "text_upsert" | "text_delete" | "clear"
  target_id  TEXT,                                 -- the stroke_id or text_id this action affected
  before_json TEXT,                                -- prior value for text_upsert/text_delete, NULL for stroke_add
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_pen_actions_board ON pen_actions(board_id, ord);

CREATE TABLE excalidraw_scenes (
  board_id        TEXT PRIMARY KEY REFERENCES boards(id) ON DELETE CASCADE,
  scene_version   INTEGER NOT NULL DEFAULT 0,
  elements_json   TEXT NOT NULL DEFAULT '[]',
  app_state_json  TEXT NOT NULL DEFAULT '{}',
  updated_at      INTEGER NOT NULL
);

CREATE TABLE moderation (
  room_id    TEXT NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
  guest_id   TEXT NOT NULL,
  kicked     INTEGER NOT NULL DEFAULT 0,
  muted      INTEGER NOT NULL DEFAULT 0,
  reason     TEXT,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (room_id, guest_id)
);
```

### Notes

- **adminToken hashing**: never store raw tokens. Argon2id with 64MB memory cost is fine on Railway containers; cache-bypass is OK because admin actions are rare relative to ws traffic.
- **Fractional indexing on `ord`**: lets us insert between two adjacent items by averaging their ords. Rebalance only when neighbors get within 2^-30 of each other.
- **Anonymous questions**: server **always** persists the real `guest_id` and `display_name` so moderation works against anonymous posters. The `anonymous` flag controls *outbound* shaping only — server replaces `author_guest_id` with `""` and `author_name` with `"Anonymous"` on every `QuestionAdded`/`QuestionUpdated` payload when `anonymous=true`. The host's UI never sees the real id either (defense in depth — moderation still works because the host can mute/kick the guest via *presence*).
- **No `questions.topic_id`**: we dropped free-form Q→topic linking in favor of `PromoteQuestionToTopic` (creates a new topic node, deletes the question).
- **Snapshot generation**: on `Welcome` or `GetSnapshot`, server reads each table for the room with simple joins; total payload per typical room (~50 topics, ~200 questions, ~10 boards) is well under 500KB JSON. Don't optimize yet.
- **Migrations**: `refinery` crate; migrations live in `server/migrations/`. Forward-only and additive (two-phase for column drops).
- **Settings JSON**: `rooms.settings_json` is a flexible per-room key/value blob for v1.x growth (e.g. toggles for raise-hand on/off). v1 doesn't use it but the column is cheap; the schema doesn't force consumers to read it.

## 2. Client-side state (IndexedDB)

Database: `topic-tree-with-qav`. One object store:

```ts
type RoomRecord = {
  roomId: string;
  title: string;
  role: "admin" | "guest";          // admin if we hold token
  adminToken?: string;              // present only for admin rooms
  guestId: string;                  // shared across all rooms on this device
  displayName?: string;
  createdAt: number;
  lastJoinedAt: number;
};
```

- `guestId` is generated once on first visit and reused (so vote dedup + name memory works across rooms).
- Admin rooms live indefinitely until user deletes them from the dashboard.
- Guest-joined rooms are also recorded so the user sees "Recently joined" + can re-enter quickly.

## 3. In-memory server state (per room actor)

```rust
struct RoomState {
  room_id: RoomId,
  clients: HashMap<ClientId, ClientHandle>,
  topics: BTreeMap<TopicId, Topic>,
  active_topic_id: Option<TopicId>,
  questions: Vec<Question>,                // sorted by created_at; vote count maintained alongside
  vote_index: HashMap<QuestionId, HashSet<GuestId>>,
  boards: HashMap<BoardId, BoardState>,
  focused_board_id: Option<BoardId>,
  cursors: HashMap<ClientId, Cursor>,      // not persisted
  presence: HashMap<GuestId, Presence>,
  hands: BTreeMap<GuestId, RaisedHand>,    // ephemeral: cleared on disconnect, never persisted
  broadcast: broadcast::Sender<ServerMsg>,
  cmd_rx: mpsc::Receiver<RoomCmd>,
  db: DbHandle,
  last_activity: Instant,                  // for reap-after-idle (default 10 min)
}

struct RaisedHand {
  guest_id: GuestId,
  display_name: String,
  topic: String,                           // 1-10 words, server-validated
  raised_at: Instant,
}

enum BoardState {
  Pen { strokes: Vec<Stroke>, texts: HashMap<TextId, Text> },
  Excalidraw { scene_version: u64, elements: serde_json::Value, app_state: serde_json::Value },
}
```

Room state is *rehydrated from SQLite* on first connection after a restart. After that, the in-memory copy is authoritative and persists incrementally on every state-changing intent.

### Read / write split

- `AppState` holds a single `Pool<SqliteConnectionManager>` (size = `num_cpus`) used by **read-only handlers** (presence list queries, snapshot generation for `Welcome`).
- **All writes** go through a single dedicated tokio task with an mpsc queue, owning *its own* SQLite connection. This keeps WAL writers single-threaded (SQLite's preferred shape) while letting reads scale.
- For `:memory:` (test) mode the pool is forced to `max_size(1)` and the read pool + write task share that single connection, so tests see a coherent database.
