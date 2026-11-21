-- Reception: what the public thinks of a work (scores) and what it won (awards), as services
-- report them. `service` is the adapter that fetched it; `origin` is whose score it is.

CREATE TABLE service_scores (
    service   TEXT NOT NULL,
    kind      TEXT NOT NULL,
    tmdb_id   INTEGER NOT NULL,
    origin    TEXT NOT NULL,
    value     REAL NOT NULL,
    scale     REAL NOT NULL,
    votes     INTEGER,
    url       TEXT,
    certified INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (kind, tmdb_id, origin)
) WITHOUT ROWID;

-- `award` is the service's id for the award; `names` its short name by language, as JSON.
-- `year` is 0 when unknown. A lower `rank` is more prominent within its body.
CREATE TABLE service_awards (
    service TEXT NOT NULL,
    kind    TEXT NOT NULL,
    tmdb_id INTEGER NOT NULL,
    award   TEXT NOT NULL,
    year    INTEGER NOT NULL DEFAULT 0,
    won     INTEGER NOT NULL,
    body    TEXT NOT NULL,
    names   TEXT NOT NULL DEFAULT '{}',
    rank    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (service, kind, tmdb_id, award, year)
) WITHOUT ROWID;
CREATE INDEX service_awards_film ON service_awards(kind, tmdb_id);

-- When a service was last asked about a work, found or not, so a work it knows nothing about
-- is not asked again every time.
CREATE TABLE service_lookups (
    service    TEXT NOT NULL,
    kind       TEXT NOT NULL,
    tmdb_id    INTEGER NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (service, kind, tmdb_id)
) WITHOUT ROWID;

-- A service's daily allowance as it last said, and when it may be asked again after spending
-- it (unix seconds).
ALTER TABLE service_state ADD COLUMN remaining INTEGER;
ALTER TABLE service_state ADD COLUMN paused_until INTEGER;
