-- Phase 4 schema: pen whiteboard tables. See .plan/2026-05-24-amber-falcon/whiteboards.md §1.
-- Forward-only and additive. PRAGMAs are set on every connection by the pool customizer.

CREATE TABLE boards (
  id         TEXT PRIMARY KEY,
  room_id    TEXT NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK(kind IN ('pen', 'excalidraw')),
  title      TEXT NOT NULL DEFAULT 'Untitled Board',
  ord        REAL NOT NULL DEFAULT 0.0,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_boards_room ON boards(room_id, ord);

CREATE TABLE pen_strokes (
  id         TEXT PRIMARY KEY,
  board_id   TEXT NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
  color      TEXT NOT NULL,
  size       REAL NOT NULL,
  points_json TEXT NOT NULL,
  ord        INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_pen_strokes_board ON pen_strokes(board_id, ord);

CREATE TABLE pen_texts (
  id         TEXT PRIMARY KEY,
  board_id   TEXT NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
  x          REAL NOT NULL,
  y          REAL NOT NULL,
  text        TEXT NOT NULL,
  font_size  REAL NOT NULL DEFAULT 16.0,
  color      TEXT NOT NULL DEFAULT '#000000',
  updated_at INTEGER NOT NULL
);
CREATE INDEX idx_pen_texts_board ON pen_texts(board_id);

-- Unified action log for undo. kind = 'stroke_begin' | 'text_set' | 'text_delete' | 'clear'
-- stroke_begin stores stroke_id in target_id; text_set stores text_id; clear has target_id=NULL
CREATE TABLE pen_actions (
  id         TEXT PRIMARY KEY,
  board_id   TEXT NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL,
  target_id  TEXT,
  ord        INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_pen_actions_board_ord ON pen_actions(board_id, ord DESC);
