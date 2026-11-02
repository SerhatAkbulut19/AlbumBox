-- AlbumBox initial schema.
-- Timestamps are UTC ISO-8601 text (strftime('%Y-%m-%dT%H:%M:%SZ','now')).

-- ---------------------------------------------------------------------------
-- Libraries (removable disks identified by the .albumbox marker file)
-- ---------------------------------------------------------------------------
CREATE TABLE libraries (
    id              TEXT PRIMARY KEY,                -- UUID stored in <root>\.albumbox
    name            TEXT NOT NULL,
    last_seen_root  TEXT,                            -- e.g. 'F:\' or 'F:\Aile'; changes with drive letter
    is_online       INTEGER NOT NULL DEFAULT 0,
    last_scan_at    TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);

-- ---------------------------------------------------------------------------
-- Media: one row per unique content (blake3 hash)
-- ---------------------------------------------------------------------------
CREATE TABLE media (
    id               INTEGER PRIMARY KEY,
    hash             TEXT NOT NULL UNIQUE,
    kind             TEXT NOT NULL CHECK (kind IN ('photo', 'video')),
    width            INTEGER,
    height           INTEGER,
    duration_ms      INTEGER,
    taken_at         TEXT,
    taken_at_source  TEXT CHECK (taken_at_source IN ('exif', 'video_meta', 'filename', 'mtime', 'manual')),
    gps_lat          REAL,
    gps_lon          REAL,
    camera           TEXT,
    thumb_ready      INTEGER NOT NULL DEFAULT 0,
    faces_done       INTEGER NOT NULL DEFAULT 0,
    visibility_mode  TEXT NOT NULL DEFAULT 'inherit' CHECK (visibility_mode IN ('inherit', 'custom')),
    is_hidden        INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX ix_media_taken_at ON media (taken_at);

-- Physical locations of a media item; same content may exist in several folders.
CREATE TABLE media_files (
    id             INTEGER PRIMARY KEY,
    media_id       INTEGER NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    library_id     TEXT NOT NULL REFERENCES libraries (id) ON DELETE CASCADE,
    rel_path       TEXT NOT NULL,                    -- relative to library root, '\' separated
    folder_rel     TEXT NOT NULL,
    size           INTEGER NOT NULL,
    mtime          INTEGER NOT NULL,                 -- unix seconds
    is_missing     INTEGER NOT NULL DEFAULT 0,
    first_seen_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    last_seen_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    UNIQUE (library_id, rel_path)
);
CREATE INDEX ix_media_files_media ON media_files (media_id);
CREATE INDEX ix_media_files_folder ON media_files (library_id, folder_rel);

-- ---------------------------------------------------------------------------
-- Tags
-- ---------------------------------------------------------------------------
CREATE TABLE tag_types (
    id          INTEGER PRIMARY KEY,
    code        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    color       TEXT,
    is_builtin  INTEGER NOT NULL DEFAULT 0,
    sort_order  INTEGER NOT NULL DEFAULT 0
);
INSERT INTO tag_types (id, code, name, color, is_builtin, sort_order) VALUES
    (1, 'person', 'Kişi',     '#7c9cc9', 1, 1),
    (2, 'place',  'Yer',      '#7fb59a', 1, 2),
    (3, 'event',  'Etkinlik', '#d4a373', 1, 3),
    (4, 'date',   'Tarih',    '#b59ac9', 1, 4),
    (5, 'custom', 'Serbest',  '#9aa5b1', 1, 5);

CREATE TABLE tags (
    id          INTEGER PRIMARY KEY,
    type_id     INTEGER NOT NULL REFERENCES tag_types (id) ON DELETE RESTRICT,
    name        TEXT NOT NULL,
    name_norm   TEXT NOT NULL,                       -- Turkish-folded, see core::text::normalize
    parent_id   INTEGER REFERENCES tags (id) ON DELETE SET NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    UNIQUE (type_id, name_norm)
);
CREATE INDEX ix_tags_parent ON tags (parent_id);

CREATE TABLE media_tags (
    media_id    INTEGER NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    tag_id      INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    source      TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'face', 'folder_suggestion')),
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    PRIMARY KEY (media_id, tag_id)
) WITHOUT ROWID;
CREATE INDEX ix_media_tags_tag ON media_tags (tag_id, media_id);

CREATE VIRTUAL TABLE tags_fts USING fts5 (
    name_norm,
    content = 'tags',
    content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER tags_fts_ai AFTER INSERT ON tags BEGIN
    INSERT INTO tags_fts (rowid, name_norm) VALUES (new.id, new.name_norm);
END;
CREATE TRIGGER tags_fts_ad AFTER DELETE ON tags BEGIN
    INSERT INTO tags_fts (tags_fts, rowid, name_norm) VALUES ('delete', old.id, old.name_norm);
END;
CREATE TRIGGER tags_fts_au AFTER UPDATE OF name_norm ON tags BEGIN
    INSERT INTO tags_fts (tags_fts, rowid, name_norm) VALUES ('delete', old.id, old.name_norm);
    INSERT INTO tags_fts (rowid, name_norm) VALUES (new.id, new.name_norm);
END;

-- ---------------------------------------------------------------------------
-- Faces
-- ---------------------------------------------------------------------------
CREATE TABLE persons (
    tag_id         INTEGER PRIMARY KEY REFERENCES tags (id) ON DELETE CASCADE,
    cover_face_id  INTEGER,
    centroid       BLOB,                             -- 512 x f32 little-endian
    face_count     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE faces (
    id             INTEGER PRIMARY KEY,
    media_id       INTEGER NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    frame_ms       INTEGER,                          -- video frame offset, NULL for photos
    bbox_x         REAL NOT NULL,                    -- normalized 0..1
    bbox_y         REAL NOT NULL,
    bbox_w         REAL NOT NULL,
    bbox_h         REAL NOT NULL,
    score          REAL NOT NULL,
    embedding      BLOB NOT NULL,                    -- 512 x f32 little-endian
    person_tag_id  INTEGER REFERENCES tags (id) ON DELETE SET NULL,
    cluster_id     INTEGER,
    status         TEXT NOT NULL DEFAULT 'unassigned'
                   CHECK (status IN ('unassigned', 'suggested', 'confirmed', 'rejected')),
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX ix_faces_media ON faces (media_id);
CREATE INDEX ix_faces_person ON faces (person_tag_id);
CREATE INDEX ix_faces_cluster ON faces (cluster_id);

-- ---------------------------------------------------------------------------
-- Users, sessions, profiles (publishing audiences)
-- ---------------------------------------------------------------------------
CREATE TABLE users (
    id             INTEGER PRIMARY KEY,
    username       TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name   TEXT NOT NULL,
    password_hash  TEXT NOT NULL,                    -- Argon2id PHC string
    role           TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    is_active      INTEGER NOT NULL DEFAULT 1,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    last_login_at  TEXT
);

CREATE TABLE sessions (
    token_hash    TEXT PRIMARY KEY,                  -- SHA-256 hex of the bearer token
    user_id       INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    expires_at    TEXT NOT NULL,
    last_seen_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    user_agent    TEXT,
    ip            TEXT
);
CREATE INDEX ix_sessions_user ON sessions (user_id);

CREATE TABLE profiles (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE COLLATE NOCASE,
    description  TEXT,
    is_builtin   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
-- Every user is automatically a member of the built-in "Tüm Aile" profile.
INSERT INTO profiles (id, name, description, is_builtin)
VALUES (1, 'Tüm Aile', 'Tüm kullanıcılar bu profilin üyesidir.', 1);

CREATE TABLE user_profiles (
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    profile_id  INTEGER NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, profile_id)
) WITHOUT ROWID;
CREATE INDEX ix_user_profiles_profile ON user_profiles (profile_id);

-- ---------------------------------------------------------------------------
-- Categories (album tree) and publishing
-- ---------------------------------------------------------------------------
CREATE TABLE categories (
    id              INTEGER PRIMARY KEY,
    parent_id       INTEGER REFERENCES categories (id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    description     TEXT,
    cover_media_id  INTEGER REFERENCES media (id) ON DELETE SET NULL,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX ix_categories_parent ON categories (parent_id);

CREATE TABLE media_categories (
    media_id     INTEGER NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    category_id  INTEGER NOT NULL REFERENCES categories (id) ON DELETE CASCADE,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    added_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    PRIMARY KEY (media_id, category_id)
) WITHOUT ROWID;
CREATE INDEX ix_media_categories_category ON media_categories (category_id, media_id);

-- No share rows = private (admin only).
CREATE TABLE category_shares (
    category_id  INTEGER NOT NULL REFERENCES categories (id) ON DELETE CASCADE,
    profile_id   INTEGER NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    PRIMARY KEY (category_id, profile_id)
) WITHOUT ROWID;

-- Used only when media.visibility_mode = 'custom'.
CREATE TABLE media_shares (
    media_id    INTEGER NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    profile_id  INTEGER NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    PRIMARY KEY (media_id, profile_id)
) WITHOUT ROWID;

-- Materialized effective visibility; client queries join on this.
CREATE TABLE media_access (
    profile_id  INTEGER NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    media_id    INTEGER NOT NULL REFERENCES media (id) ON DELETE CASCADE,
    PRIMARY KEY (profile_id, media_id)
) WITHOUT ROWID;
CREATE INDEX ix_media_access_media ON media_access (media_id);

-- ---------------------------------------------------------------------------
-- Background jobs and settings
-- ---------------------------------------------------------------------------
CREATE TABLE jobs (
    id              INTEGER PRIMARY KEY,
    kind            TEXT NOT NULL,                   -- 'scan', 'thumbs', 'faces', ...
    payload         TEXT NOT NULL DEFAULT '{}',      -- JSON
    status          TEXT NOT NULL DEFAULT 'queued'
                    CHECK (status IN ('queued', 'running', 'done', 'failed', 'cancelled')),
    progress_done   INTEGER NOT NULL DEFAULT 0,
    progress_total  INTEGER NOT NULL DEFAULT 0,
    message         TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    started_at      TEXT,
    finished_at     TEXT
);
CREATE INDEX ix_jobs_status ON jobs (status, id);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);
