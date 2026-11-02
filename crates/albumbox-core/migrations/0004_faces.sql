-- Phase 5: face recognition.

-- media.faces_done: 0 = pending, 1 = analysed, -1 = failed.
CREATE INDEX ix_media_faces_pending ON media (faces_done) WHERE faces_done = 0;
CREATE INDEX ix_faces_status ON faces (status, person_tag_id);
