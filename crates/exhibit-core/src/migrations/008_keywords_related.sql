-- What a work is about and what it is like, for the keyword and people filters, `similar` and
-- outside agents.

-- TMDB's keywords for a work, in English (TMDB does not translate them).
CREATE TABLE item_keywords (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    keyword TEXT NOT NULL,
    PRIMARY KEY (item_id, keyword)
) WITHOUT ROWID;
CREATE INDEX item_keywords_keyword ON item_keywords(keyword);

-- The works TMDB recommends after this one, by TMDB id (the same kind), most fitting first.
CREATE TABLE item_related (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    tmdb_id INTEGER NOT NULL,
    rank INTEGER NOT NULL,
    PRIMARY KEY (item_id, tmdb_id)
) WITHOUT ROWID;
CREATE INDEX item_related_tmdb ON item_related(tmdb_id);

-- `items.people`, one row per person and role, kept by triggers so every path that writes the
-- people keeps it.
CREATE TABLE item_people (
    item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    name TEXT NOT NULL,
    tmdb_id INTEGER,
    PRIMARY KEY (item_id, role, name)
) WITHOUT ROWID;
CREATE INDEX item_people_name ON item_people(name, role);

CREATE TRIGGER item_people_ai AFTER INSERT ON items BEGIN
    INSERT OR IGNORE INTO item_people(item_id, role, name, tmdb_id)
    SELECT new.id, p.value ->> 'role', p.value ->> 'name', p.value ->> 'tmdb_id'
    FROM json_each(new.people) p WHERE p.value ->> 'name' IS NOT NULL;
END;

CREATE TRIGGER item_people_au AFTER UPDATE OF people ON items BEGIN
    DELETE FROM item_people WHERE item_id = new.id;
    INSERT OR IGNORE INTO item_people(item_id, role, name, tmdb_id)
    SELECT new.id, p.value ->> 'role', p.value ->> 'name', p.value ->> 'tmdb_id'
    FROM json_each(new.people) p WHERE p.value ->> 'name' IS NOT NULL;
END;

INSERT OR IGNORE INTO item_people(item_id, role, name, tmdb_id)
SELECT i.id, p.value ->> 'role', p.value ->> 'name', p.value ->> 'tmdb_id'
FROM items i, json_each(i.people) p WHERE p.value ->> 'name' IS NOT NULL;

-- Every matched work is due again, so its keywords, its wider crew and its related works arrive.
UPDATE items SET fetched_at = NULL WHERE kind IN ('movie', 'show') AND tmdb_id IS NOT NULL;
