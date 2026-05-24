-- Phase 3 schema: questions + question_votes for the Q&A feature.
-- See .plan/2026-05-24-amber-falcon/data-model.md §1.
-- Forward-only and additive. PRAGMAs are set on every connection by the pool customizer.

CREATE TABLE questions (
  id              TEXT PRIMARY KEY,
  room_id         TEXT NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
  author_guest_id TEXT NOT NULL,
  author_name     TEXT NOT NULL,
  anonymous       INTEGER NOT NULL DEFAULT 0,
  text            TEXT NOT NULL,
  answered        INTEGER NOT NULL DEFAULT 0,
  created_at      INTEGER NOT NULL
);
CREATE INDEX idx_questions_room ON questions(room_id, created_at);

CREATE TABLE question_votes (
  question_id TEXT NOT NULL REFERENCES questions(id) ON DELETE CASCADE,
  guest_id    TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (question_id, guest_id)
);
