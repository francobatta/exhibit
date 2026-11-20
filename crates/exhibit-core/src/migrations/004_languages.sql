-- The library in more than one language. `items` keeps the English text TMDB gave, the base;
-- `item_texts` holds an item's text in each other language the owner has used. An empty
-- field is a gap TMDB has not translated, and reads fall back to the base field by field.
-- `poster` is the cached file of a poster in that language, NULL when TMDB has none.
CREATE TABLE item_texts (
    item_id  INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    lang     TEXT NOT NULL,
    title    TEXT NOT NULL DEFAULT '',
    overview TEXT NOT NULL DEFAULT '',
    genres   TEXT NOT NULL DEFAULT '[]',
    poster   TEXT,
    PRIMARY KEY (item_id, lang)
) WITHOUT ROWID;

-- Seasons, and episodes TMDB has no title for, were named in English by the core. They are
-- left without a title now: the window words them from their numbers.
UPDATE items SET title = ''
WHERE kind = 'season' AND (title = 'Season ' || season OR (season = 0 AND title = 'Specials'));
UPDATE items SET title = '' WHERE kind = 'episode' AND title = 'Episode ' || episode;

-- The full-text index covers the base text and every translation, so a work is found by its
-- name in any language the library holds.
DROP TRIGGER items_fts_ai;
DROP TRIGGER items_fts_au;

CREATE TRIGGER items_fts_ai AFTER INSERT ON items BEGIN
    INSERT INTO items_fts(rowid, title, people, genres, overview) VALUES (
        new.id,
        new.title,
        (SELECT coalesce(group_concat(json_extract(value, '$.name'), ' '), '') FROM json_each(new.people)),
        (SELECT coalesce(group_concat(value, ' '), '') FROM json_each(new.genres)),
        coalesce(new.overview, '')
    );
END;

CREATE TRIGGER items_fts_au AFTER UPDATE OF title, people, genres, overview ON items BEGIN
    DELETE FROM items_fts WHERE rowid = old.id;
    INSERT INTO items_fts(rowid, title, people, genres, overview)
    SELECT i.id,
        i.title || coalesce((SELECT ' ' || group_concat(t.title, ' ') FROM item_texts t WHERE t.item_id = i.id), ''),
        (SELECT coalesce(group_concat(json_extract(value, '$.name'), ' '), '') FROM json_each(i.people)),
        (SELECT coalesce(group_concat(value, ' '), '') FROM json_each(i.genres))
            || coalesce((SELECT ' ' || group_concat(g.value, ' ') FROM item_texts t, json_each(t.genres) g WHERE t.item_id = i.id), ''),
        coalesce(i.overview, '') || coalesce((SELECT ' ' || group_concat(t.overview, ' ') FROM item_texts t WHERE t.item_id = i.id), '')
    FROM items i WHERE i.id = new.id;
END;

CREATE TRIGGER item_texts_fts_ai AFTER INSERT ON item_texts BEGIN
    DELETE FROM items_fts WHERE rowid = new.item_id;
    INSERT INTO items_fts(rowid, title, people, genres, overview)
    SELECT i.id,
        i.title || coalesce((SELECT ' ' || group_concat(t.title, ' ') FROM item_texts t WHERE t.item_id = i.id), ''),
        (SELECT coalesce(group_concat(json_extract(value, '$.name'), ' '), '') FROM json_each(i.people)),
        (SELECT coalesce(group_concat(value, ' '), '') FROM json_each(i.genres))
            || coalesce((SELECT ' ' || group_concat(g.value, ' ') FROM item_texts t, json_each(t.genres) g WHERE t.item_id = i.id), ''),
        coalesce(i.overview, '') || coalesce((SELECT ' ' || group_concat(t.overview, ' ') FROM item_texts t WHERE t.item_id = i.id), '')
    FROM items i WHERE i.id = new.item_id;
END;

CREATE TRIGGER item_texts_fts_au AFTER UPDATE ON item_texts BEGIN
    DELETE FROM items_fts WHERE rowid = new.item_id;
    INSERT INTO items_fts(rowid, title, people, genres, overview)
    SELECT i.id,
        i.title || coalesce((SELECT ' ' || group_concat(t.title, ' ') FROM item_texts t WHERE t.item_id = i.id), ''),
        (SELECT coalesce(group_concat(json_extract(value, '$.name'), ' '), '') FROM json_each(i.people)),
        (SELECT coalesce(group_concat(value, ' '), '') FROM json_each(i.genres))
            || coalesce((SELECT ' ' || group_concat(g.value, ' ') FROM item_texts t, json_each(t.genres) g WHERE t.item_id = i.id), ''),
        coalesce(i.overview, '') || coalesce((SELECT ' ' || group_concat(t.overview, ' ') FROM item_texts t WHERE t.item_id = i.id), '')
    FROM items i WHERE i.id = new.item_id;
END;

-- A translation removed with its item needs nothing: the item's own trigger clears its row.
CREATE TRIGGER item_texts_fts_ad AFTER DELETE ON item_texts
WHEN EXISTS (SELECT 1 FROM items WHERE id = old.item_id) BEGIN
    DELETE FROM items_fts WHERE rowid = old.item_id;
    INSERT INTO items_fts(rowid, title, people, genres, overview)
    SELECT i.id,
        i.title || coalesce((SELECT ' ' || group_concat(t.title, ' ') FROM item_texts t WHERE t.item_id = i.id), ''),
        (SELECT coalesce(group_concat(json_extract(value, '$.name'), ' '), '') FROM json_each(i.people)),
        (SELECT coalesce(group_concat(value, ' '), '') FROM json_each(i.genres))
            || coalesce((SELECT ' ' || group_concat(g.value, ' ') FROM item_texts t, json_each(t.genres) g WHERE t.item_id = i.id), ''),
        coalesce(i.overview, '') || coalesce((SELECT ' ' || group_concat(t.overview, ' ') FROM item_texts t WHERE t.item_id = i.id), '')
    FROM items i WHERE i.id = old.item_id;
END;
