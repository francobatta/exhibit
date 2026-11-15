-- Sources store their path relative to the volume root, so the same relative path can
-- exist on two volumes (two drives added at their root both store ''). Rebuild `sources`
-- with uniqueness on (volume_id, path) and a `last_path` column holding the last known
-- absolute path, shown while the volume is offline.
--
-- `files` references `sources`, and with foreign keys on, `DROP TABLE sources` would
-- cascade into `files`. So `files` is rebuilt too, pointing at the new table first; the
-- renames at the end rewrite that reference back to `sources`.

CREATE TABLE sources_new (
    id        INTEGER PRIMARY KEY,
    path      TEXT NOT NULL,
    volume_id TEXT NOT NULL,
    added_at  INTEGER NOT NULL DEFAULT 0,
    last_path TEXT NOT NULL DEFAULT '',
    UNIQUE (volume_id, path)
);
INSERT INTO sources_new(id, path, volume_id, added_at, last_path)
    SELECT id, path, volume_id, added_at, path FROM sources;

CREATE TABLE files_new (
    id        INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES sources_new(id) ON DELETE CASCADE,
    rel_path  TEXT NOT NULL,
    size      INTEGER NOT NULL DEFAULT 0,
    mtime     INTEGER NOT NULL DEFAULT 0,
    item_id   INTEGER REFERENCES items(id) ON DELETE SET NULL,
    tech      TEXT,
    probed    INTEGER NOT NULL DEFAULT 0,
    present   INTEGER NOT NULL DEFAULT 1,
    UNIQUE (source_id, rel_path)
);
INSERT INTO files_new SELECT id, source_id, rel_path, size, mtime, item_id, tech, probed, present FROM files;

DROP TABLE files;
DROP TABLE sources;
ALTER TABLE sources_new RENAME TO sources;
ALTER TABLE files_new RENAME TO files;
CREATE INDEX files_item ON files(item_id);
