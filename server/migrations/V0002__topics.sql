-- Phase 2 schema: topics table for the topic tree feature.
-- See .plan/2026-05-24-amber-falcon/data-model.md §1.
-- Forward-only and additive. PRAGMAs are set on every connection by the pool customizer.

CREATE TABLE topics (
  id          TEXT PRIMARY KEY,
  room_id     TEXT NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
  parent_id   TEXT REFERENCES topics(id) ON DELETE CASCADE,
  title       TEXT NOT NULL,
  ord         REAL NOT NULL,
  status      TEXT NOT NULL DEFAULT 'pending',
  created_at  INTEGER NOT NULL
);
CREATE INDEX idx_topics_room ON topics(room_id, parent_id, ord);
