-- Services: outside sites the owner connects (Letterboxd) feeding facts about films.
-- `service_state` is one row per connected service; keys stay in the credential store.
CREATE TABLE service_state (
    service     TEXT PRIMARY KEY,
    enabled     INTEGER NOT NULL DEFAULT 1,
    username    TEXT,
    export_name TEXT,
    export_at   INTEGER,
    synced_at   INTEGER,
    error       TEXT
);

-- A film as a service knows it, and the TMDB film it was resolved to. `tried` is set once the
-- matcher has looked, so a film it could not place surely is not searched again.
CREATE TABLE service_films (
    service  TEXT NOT NULL,
    film_key TEXT NOT NULL,
    kind     TEXT NOT NULL DEFAULT 'movie',
    title    TEXT,
    year     INTEGER,
    tmdb_id  INTEGER,
    tried    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (service, film_key)
) WITHOUT ROWID;

-- What a service says about a TMDB work: a watch on a day (0 when undated), a like.
-- `value` holds the rest as JSON.
CREATE TABLE service_facts (
    service TEXT NOT NULL,
    kind    TEXT NOT NULL,
    tmdb_id INTEGER NOT NULL,
    fact    TEXT NOT NULL,
    on_day  INTEGER NOT NULL DEFAULT 0,
    value   TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (service, kind, tmdb_id, fact, on_day)
) WITHOUT ROWID;
CREATE INDEX service_facts_film ON service_facts(kind, tmdb_id);

-- The owner's own choice of watched and loved: NULL when not made, else 0 or 1. It wins over
-- everything a service says. `wanted` marks a film the owner wants and does not have yet.
ALTER TABLE items ADD COLUMN watched_choice INTEGER;
ALTER TABLE items ADD COLUMN loved_choice INTEGER;
ALTER TABLE items ADD COLUMN wanted INTEGER NOT NULL DEFAULT 0;
UPDATE items SET loved_choice = 1 WHERE loved = 1;
ALTER TABLE items DROP COLUMN loved;
