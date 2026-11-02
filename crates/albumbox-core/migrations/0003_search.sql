-- Phase 4: search.

-- Turkish-folded folder path with separators as spaces ("Köy 2022\Nişan" → "koy 2022 nisan"),
-- so words in folder names are searchable. Filled by the scanner; existing rows are
-- back-filled at startup (see Db::open).
ALTER TABLE media_files ADD COLUMN folder_norm TEXT;
