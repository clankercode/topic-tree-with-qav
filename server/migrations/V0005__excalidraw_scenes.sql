-- Phase F1 schema (.plan/2026-05-25-followup/persistence.md §5): excalidraw scene state.
-- Forward-only and additive. PRAGMAs are set on every connection by the pool customizer.

CREATE TABLE excalidraw_scenes (
  board_id        TEXT    PRIMARY KEY REFERENCES boards(id) ON DELETE CASCADE,
  scene_version   INTEGER NOT NULL DEFAULT 0,
  elements_json   TEXT    NOT NULL DEFAULT '[]',
  app_state_json  TEXT    NOT NULL DEFAULT '{}',
  updated_at      INTEGER NOT NULL
);
