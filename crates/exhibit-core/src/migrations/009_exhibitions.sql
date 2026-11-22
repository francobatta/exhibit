-- Exhibitions: small shows a curator (an outside agent, later the built-in one) hangs from the
-- owner's own collection, each with a title, a wall text and a note on every work. Home shows
-- them as shelves; nothing else depends on them.
CREATE TABLE exhibitions (
    id         INTEGER PRIMARY KEY,
    title      TEXT NOT NULL,
    wall_text  TEXT NOT NULL DEFAULT '',
    curator    TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE exhibition_works (
    exhibition_id INTEGER NOT NULL REFERENCES exhibitions(id) ON DELETE CASCADE,
    item_id       INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL,
    note          TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (exhibition_id, item_id)
) WITHOUT ROWID;
CREATE INDEX exhibition_works_item ON exhibition_works(item_id);
