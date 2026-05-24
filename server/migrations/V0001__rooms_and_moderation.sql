-- Phase 1 schema: rooms + moderation. See .plan/2026-05-24-amber-falcon/data-model.md §1.
-- Forward-only and additive. PRAGMAs (journal_mode=WAL, foreign_keys=ON,
-- synchronous=NORMAL) are set on every connection by the pool customizer,
-- not here, because PRAGMAs are connection-scoped in SQLite.

CREATE TABLE rooms (
  id               TEXT    PRIMARY KEY,
  title            TEXT    NOT NULL DEFAULT 'Untitled',
  admin_token_hash TEXT    NOT NULL,
  created_at       INTEGER NOT NULL,
  last_active_at   INTEGER NOT NULL,
  active_topic_id  TEXT,
  focused_board_id TEXT,
  settings_json    TEXT    NOT NULL DEFAULT '{}'
);

CREATE TABLE moderation (
  room_id    TEXT    NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
  guest_id   TEXT    NOT NULL,
  kicked     INTEGER NOT NULL DEFAULT 0,
  muted      INTEGER NOT NULL DEFAULT 0,
  reason     TEXT,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (room_id, guest_id)
);
