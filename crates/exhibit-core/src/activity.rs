//! The owner's activity merged from every source: what Exhibit saw, what the owner chose in
//! Exhibit, and what the services the owner has switched on say. This is the only place the
//! rule is written; every read of watched or loved goes through it.

/// Whether a work counts as watched, as SQL over its `items` row `item` and its `play_state`
/// row `play` (which may be missing): the owner's choice if made, else Exhibit played it to
/// the end, else a switched-on service saw it.
pub(crate) fn watched_sql(item: &str, play: &str) -> String {
    format!(
        "coalesce({item}.watched_choice, coalesce({play}.watched, 0) = 1 OR {})",
        from_a_service(item, "watch")
    )
}

/// Whether a work counts as loved, as SQL over its `items` row `item`: the owner's choice if
/// made, else a switched-on service liked it.
pub(crate) fn loved_sql(item: &str) -> String {
    format!(
        "coalesce({item}.loved_choice, {})",
        from_a_service(item, "like")
    )
}

fn from_a_service(item: &str, fact: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM service_facts sf \
         JOIN service_state ss ON ss.service = sf.service AND ss.enabled = 1 \
         WHERE sf.kind = {item}.kind AND sf.tmdb_id = {item}.tmdb_id AND sf.fact = '{fact}')"
    )
}
