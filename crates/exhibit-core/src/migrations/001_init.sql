CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT
);

CREATE TABLE sources (
    id        INTEGER PRIMARY KEY,
    path      TEXT NOT NULL UNIQUE,
    volume_id TEXT NOT NULL,
    added_at  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE items (
    id           INTEGER PRIMARY KEY,
    kind         TEXT NOT NULL,
    parent_id    INTEGER REFERENCES items(id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    year         INTEGER,
    season       INTEGER,
    episode      INTEGER,
    overview     TEXT,
    runtime_min  INTEGER,
    genres       TEXT NOT NULL DEFAULT '[]',
    people       TEXT NOT NULL DEFAULT '[]',
    poster       TEXT,
    backdrop     TEXT,
    tmdb_id      INTEGER,
    match_state  TEXT NOT NULL DEFAULT 'unmatched',
    match_source TEXT,
    confidence   REAL,
    candidates   TEXT NOT NULL DEFAULT '[]',
    loved        INTEGER NOT NULL DEFAULT 0,
    added_at     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX items_parent ON items(parent_id);
CREATE INDEX items_tmdb ON items(tmdb_id);
CREATE INDEX items_match_state ON items(match_state);

CREATE TABLE files (
    id        INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    rel_path  TEXT NOT NULL,
    size      INTEGER NOT NULL DEFAULT 0,
    mtime     INTEGER NOT NULL DEFAULT 0,
    item_id   INTEGER REFERENCES items(id) ON DELETE SET NULL,
    tech      TEXT,
    probed    INTEGER NOT NULL DEFAULT 0,
    present   INTEGER NOT NULL DEFAULT 1,
    UNIQUE (source_id, rel_path)
);
CREATE INDEX files_item ON files(item_id);

CREATE TABLE play_state (
    item_id        INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
    position_s     REAL NOT NULL DEFAULT 0,
    duration_s     REAL NOT NULL DEFAULT 0,
    watched        INTEGER NOT NULL DEFAULT 0,
    last_played_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE play_log (
    id         INTEGER PRIMARY KEY,
    item_id    INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    started_at INTEGER NOT NULL,
    ended_at   INTEGER,
    position_s REAL NOT NULL DEFAULT 0
);
CREATE INDEX play_log_item ON play_log(item_id);

-- Full-text index keyed by item id (rowid). Kept in sync by triggers.
CREATE VIRTUAL TABLE items_fts USING fts5(title, people, genres, overview);

CREATE TRIGGER items_fts_ai AFTER INSERT ON items BEGIN
    INSERT INTO items_fts(rowid, title, people, genres, overview) VALUES (
        new.id,
        new.title,
        (SELECT coalesce(group_concat(json_extract(value, '$.name'), ' '), '') FROM json_each(new.people)),
        (SELECT coalesce(group_concat(value, ' '), '') FROM json_each(new.genres)),
        coalesce(new.overview, '')
    );
END;

CREATE TRIGGER items_fts_ad AFTER DELETE ON items BEGIN
    DELETE FROM items_fts WHERE rowid = old.id;
END;

CREATE TRIGGER items_fts_au AFTER UPDATE ON items BEGIN
    DELETE FROM items_fts WHERE rowid = old.id;
    INSERT INTO items_fts(rowid, title, people, genres, overview) VALUES (
        new.id,
        new.title,
        (SELECT coalesce(group_concat(json_extract(value, '$.name'), ' '), '') FROM json_each(new.people)),
        (SELECT coalesce(group_concat(value, ' '), '') FROM json_each(new.genres)),
        coalesce(new.overview, '')
    );
END;
