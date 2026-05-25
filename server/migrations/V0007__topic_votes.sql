-- Topic upvotes: one row per (topic, guest), mirroring question_votes.

CREATE TABLE topic_votes (
  topic_id   TEXT NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
  guest_id   TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (topic_id, guest_id)
);
