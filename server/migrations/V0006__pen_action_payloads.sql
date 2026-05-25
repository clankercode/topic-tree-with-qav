-- Phase F1 schema (.plan/2026-05-25-followup/persistence.md §5): per-action undo payload.
-- Forward-only and additive. Existing rows get NULL — pre-V0006 actions cannot be undone
-- after a server restart, which is acceptable since undo was memory-only before persistence
-- landed.

ALTER TABLE pen_actions ADD COLUMN payload_json TEXT;
