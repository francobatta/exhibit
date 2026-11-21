-- When TMDB's details of a matched movie or show were last fetched (unix seconds). TMDB's terms
-- allow its data to be kept six months at most, so `freshen` fetches each work again before
-- then. Works already matched get a time spread across the window, so they come due a few at a
-- time rather than all on one day.
ALTER TABLE items ADD COLUMN fetched_at INTEGER;
UPDATE items SET fetched_at = unixepoch() - (abs(random()) % (140 * 86400))
WHERE kind IN ('movie', 'show') AND tmdb_id IS NOT NULL;
CREATE INDEX items_fetched ON items(fetched_at) WHERE fetched_at IS NOT NULL;
