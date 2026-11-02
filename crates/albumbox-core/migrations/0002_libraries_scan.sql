-- Phase 2: library roots and scanning.

-- Library root relative to its drive, e.g. 'Aile' for F:\Aile ('' = drive root).
-- Lets the engine find the library again when the drive letter changes.
ALTER TABLE libraries ADD COLUMN root_subpath TEXT NOT NULL DEFAULT '';

-- media.thumb_ready: 0 = pending, 1 = ready, -1 = failed (not retried automatically).
CREATE INDEX ix_media_thumb_pending ON media (thumb_ready) WHERE thumb_ready = 0;
CREATE INDEX ix_media_files_seen ON media_files (library_id, last_seen_at);
