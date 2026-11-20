# Services, part 1: the neutral model and Letterboxd Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The owner's Letterboxd ratings, reviews, logs, likes and watchlist show in Exhibit's
existing screens, marked as Letterboxd's. They arrive through a site-agnostic service interface,
and Exhibit's core names no site.

**Architecture:** `exhibit-core` gains a `Service` trait and neutral facts (`service.rs`). It
also gains their storage, the resolution of service films to TMDB films, and the merged
watched/loved rule (`activity.rs`, `sync.rs`, migration 005). A new crate `exhibit-letterboxd`
turns an export ZIP and the RSS feed into facts. A registry crate, `exhibit-services`, lists
the compiled-in adapters for the app and the CLI. The UI reads everything through Tauri
commands and draws each fact with the mark its adapter supplies.

**Tech Stack:** Rust 2024 with `rusqlite`, `ureq`, and new `zip`, `csv` and `roxmltree` in the
Letterboxd crate; Tauri 2; React, TanStack Query and UI SDK; Bun.

**Spec:** `docs/superpowers/specs/2026-11-20-services-design.md`. This plan covers its build
stages 1 to 4. Stages 5 (MDBList) and 6 (Wikidata) get their own plan once a real MDBList key
confirms that API's fields.

## Global Constraints
- **Bun only;** never `node`, `npm` or `npx`. Plain `rusqlite` with hand-written SQL.
- **The UI never touches the database, the file system or the network.** Everything goes
  through Tauri commands over `exhibit_core::Library`.
- **`exhibit-core` names no site.** A test fails if `letterboxd`, `mdblist`, `wikidata`,
  `tomato` or `metacritic` appears in `crates/exhibit-core/src` (case-insensitive).
- **Never point tests at the owner's data dir or keyring.** Never write in a media folder.
  Adapters are tested against fixtures, never live.
- **Tokens only in `ui/src/styles/tokens.css`,** sized in `rem`. No accent colour; marks are
  drawn in the text colour.
- **Every new string is in all four catalogues** (`en-US`, `es`, `es-ES`, `es-MX`).
  Errors carry codes (`Detail::coded`) and have words in the catalogues.
- **With no activity service set up, every screen is exactly today's:** no new rows, groups,
  sorts, shelves or labels.
- **The owner's choice in Exhibit wins** over any service, for watched and loved.
- **An export row matched unsurely is left out and counted,** never guessed
  (`matcher::decide`).
- **5,000 works hold 60 fps.** New summary columns are SQL expressions in the one listing
  query, never per-row calls.

## Review Focus
1. **A film logged on Letterboxd, un-marked in Exhibit, and then Letterboxd switched off:** the
   eye stays off; switching Letterboxd on again keeps it off. (Task 1)
2. **Titles with commas, quotes or non-Latin letters in the export CSV**, and two films with the
   same title in different years: each resolves to its own film. (Task 5)
3. **RSS items that are not films** (list entries, no `tmdb:movieId`) are skipped without
   failing the sync. (Task 5)
4. **Importing an export older than the RSS entries already stored:** after the import's
   follow-up feed read, the newer entries are still there. (Task 3)
5. **A watchlist film the owner later adds to a source** becomes one row with files. It keeps
   the owner's choices, and leaves "not in your library". (Task 3)

---

### Task 1: Migration 005 and the merged watched and loved state

**Files:**
- Create: `crates/exhibit-core/src/migrations/005_services.sql`, `crates/exhibit-core/src/activity.rs`
- Modify: `crates/exhibit-core/src/db.rs` (register 005), `query.rs` (`list`, `set_watched`,
  `set_loved`, `item_with`), `rows.rs` (`ITEM_COLUMNS`, `LOCAL_ITEM_COLUMNS`), `filter.rs`
  (`loved`, `seen`), `player.rs` (`record`), `identify.rs` (`absorb`), `lib.rs`,
  `docs/superpowers/specs/2026-11-14-exhibit-design.md` (section 3 non-goal and section 14
  Settings, as the services spec section 11 words them)

**Interfaces:**
- Produces:
  - **Tables:**
    - `service_state(service TEXT PK, enabled INTEGER NOT NULL DEFAULT 1, username TEXT,
      export_name TEXT, export_at INTEGER, synced_at INTEGER, error TEXT)`;
    - `service_films(service, film_key, kind TEXT NOT NULL DEFAULT 'movie', title, year,
      tmdb_id INTEGER NULL, tried INTEGER NOT NULL DEFAULT 0, PK(service, film_key))`;
    - `service_facts(service, kind, tmdb_id INTEGER NOT NULL, fact TEXT NOT NULL,
      on_day INTEGER NOT NULL DEFAULT 0, value TEXT NOT NULL DEFAULT '{}',
      PK(service, kind, tmdb_id, fact, on_day))`, indexed on `(kind, tmdb_id)`.
  - **Columns on `items`:** `watched_choice INTEGER NULL`, `loved_choice INTEGER NULL` and
    `wanted INTEGER NOT NULL DEFAULT 0`. `loved` is migrated into `loved_choice` (1 becomes 1,
    0 becomes NULL), then dropped.
  - **`activity.rs`:** `pub(crate) fn watched_sql(item: &str, play: &str) -> String` and
    `pub(crate) fn loved_sql(item: &str) -> String`. These are the only place the merging
    rule is written:
    - the choice if set;
    - else Exhibit's `play_state.watched`;
    - else any `service_facts` row with fact `watch` (or `like`) for the item's
      `(kind, tmdb_id)`, from a service whose `service_state.enabled = 1`.
  - `Item.loved`, `ItemSummary.loved` and `ItemSummary.watched` now carry the merged values.

- [ ] **Step 1: Write the failing tests** (in `query.rs` tests, with its `env()`; insert facts
  and state rows with plain SQL)

```rust
#[test]
fn a_service_watch_counts_and_the_owners_choice_wins() {
    let e = env();
    let m = e.movie_with_tmdb("Taxi Driver", 103);           // helper: a movie item with a file and tmdb_id
    e.sql("INSERT INTO service_state(service) VALUES ('svc')");
    e.sql("INSERT INTO service_facts(service, kind, tmdb_id, fact, on_day) VALUES ('svc','movie',103,'watch',20000)");
    assert!(summary(&e, m).watched);
    set_watched(&e.c, m, false).unwrap();
    assert!(!summary(&e, m).watched);                          // the choice wins
    e.sql("UPDATE service_state SET enabled = 0");
    assert!(!summary(&e, m).watched);
    e.sql("UPDATE service_state SET enabled = 1");
    assert!(!summary(&e, m).watched);                          // still the choice
}

#[test]
fn a_service_like_lights_loved_until_the_owner_says_no() { /* same shape with 'like', set_loved */ }

#[test]
fn a_switched_off_service_counts_for_nothing() { /* enabled = 0, no choice: watched false */ }
```
  - In `player.rs` tests: `record_to_the_end_clears_a_no_choice`. After `set_watched(m, false)`,
    a `record` at 95% leaves `watched_choice` NULL and the summary watched.
  - In `identify.rs` tests: `absorb_carries_choices`. A file item absorbed into a `wanted` row
    keeps the target's choices, takes the source's choices where the target has none, and sets
    `wanted = 0`.

- [ ] **Step 2: Run them to see them fail**
  - Run: `cargo test -p exhibit-core service_ choice absorb_carries`
  - Expected: compile errors (no columns, no helpers).

- [ ] **Step 3: Implement**
  - Write the migration and `activity.rs`.
  - Use `watched_sql` in `list` for non-containers, and in the containers' episode count
    (replacing `q.watched = 1`). Use `loved_sql` for `loved` in `list` and in the item
    columns.
  - `set_watched` also writes `watched_choice` for each target. `set_loved` writes
    `loved_choice`.
  - `record` clears `watched_choice = 0` when the session marks watched.
  - `absorb` carries the choices and clears `wanted`.
  - Grep `loved` in `crates/exhibit-core/src`, and the CLI, to leave no read of the dropped
    column.
  - Edit the main spec's non-goal and Settings lines.

- [ ] **Step 4: Run the core suite**
  - Run: `cargo test -p exhibit-core`
  - Expected: PASS, including every existing test.

- [ ] **Step 5: Commit**: `git commit -m "core: merged watched and loved, the owner's choice, service tables"`

---

### Task 2: The service interface

**Files:**
- Create: `crates/exhibit-core/src/service.rs`
- Modify: `crates/exhibit-core/src/lib.rs`
- Test: in `service.rs`

**Interfaces:**
- Produces (all `pub`, serde `snake_case`):

```rust
pub const EXHIBIT: &str = "exhibit";                       // reserved id for Exhibit's own facts
pub enum Provides { Activity, Reception, Awards }
pub enum Need { Username, Key, ExportFile }
pub struct ExportHelp { pub page: &'static str, pub file_prefix: &'static str, pub file_ext: &'static str }
pub struct ServiceInfo {
    pub id: &'static str, pub name: &'static str,
    pub mark_svg: &'static str,                            // one-colour SVG, drawn in the text colour
    pub provides: &'static [Provides], pub needs: &'static [Need],
    pub export: Option<ExportHelp>,
}
pub struct FilmRef { pub key: String, pub tmdb_id: Option<i64>, pub title: String, pub year: Option<i32> }
pub enum Fact { Rating { value: f32, scale: f32 }, Review { text: String, spoiler: bool }, Watch { rewatch: bool }, Like, Watchlisted }
pub struct Dated { pub film: FilmRef, pub fact: Fact, pub on: Option<i64> }   // `on`: unix seconds of the day, UTC
pub struct ActivityBatch { pub facts: Vec<Dated> }
pub enum SyncInput<'a> { Snapshot(&'a std::path::Path), Since(Option<i64>) }
pub struct Account<'a> { pub username: Option<&'a str>, pub key: Option<&'a str> }
pub trait Service: Send + Sync {
    fn info(&self) -> &ServiceInfo;
    fn activity(&self, account: &Account, input: SyncInput, http: &dyn crate::tmdb::Http) -> Result<ActivityBatch>;
    /// How many recent entries the account shows; a coded error says what is wrong.
    fn check(&self, account: &Account, http: &dyn crate::tmdb::Http) -> Result<u32>;
}
pub fn film_key(title: &str, year: Option<i32>) -> String;  // lowercased, trimmed, "title|year" or "title|"
pub fn normalize_username(s: &str) -> String;               // trims, drops a leading '@', lowercases
```
- **Error codes,** all generic, for adapters to use (with words in the catalogues in Task 8):
  `service.user_not_found`, `service.private`, `service.unreachable`, `service.bad_export`,
  `service.unsupported`.

- [ ] **Step 1: Write the failing tests**
  - `film_key("  Taxi Driver ", Some(1976)) == "taxi driver|1976"`.
  - `normalize_username(" @Rian ") == "rian"`.
  - `core_names_no_site`: reads every `.rs` file under `env!("CARGO_MANIFEST_DIR")/src` and
    asserts none contains any of the five names (case-insensitive).
- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p exhibit-core service::` (expected: compile error).
- [ ] **Step 3: Implement `service.rs`** as above. `activity`'s default body returns
  `Err(Error::Invalid(Detail::coded("service.unsupported", …)))`.
- [ ] **Step 4: Run the tests.** Run: `cargo test -p exhibit-core service::` (expected: PASS).
- [ ] **Step 5: Commit**: `git commit -m "core: the service interface and neutral facts"`

---

### Task 3: Syncing activity into the library

**Files:**
- Create: `crates/exhibit-core/src/sync.rs`
- Modify: `crates/exhibit-core/src/library.rs`, `identify.rs` (`save_movie` and `search_all`
  visibility to `pub(crate)` if needed)
- Test: in `sync.rs` (a test-only fake `Service`, and a fake `MetadataProvider` following
  `identify.rs`'s `Fake`)

**Interfaces:**
- Consumes: Task 1's tables and Task 2's types; `identify::search_all`, `matcher::score`
  (with `Evidence { title, year, duration_min: None }`), `matcher::decide` and `save_movie`.
- Produces, on `Library`:

```rust
pub struct ServiceState { pub service: String, pub enabled: bool, pub username: Option<String>,
    pub export_name: Option<String>, pub export_at: Option<i64>, pub synced_at: Option<i64>,
    pub error: Option<String>, pub unmatched: u32 }
pub struct SyncReport { pub logged: u32, pub rated: u32, pub reviewed: u32, pub liked: u32,
    pub watchlisted: u32, pub not_in_library: u32, pub unmatched: u32 }
pub fn service_states(&self) -> Result<Vec<ServiceState>>;
pub fn set_service_enabled(&self, service: &str, on: bool) -> Result<()>;
/// `None` forgets the service: its facts, films, state and `wanted` rows go.
pub fn set_service_username(&self, service: &str, username: Option<&str>) -> Result<()>;
pub fn check_service(&self, svc: &dyn Service, username: &str) -> Result<u32>;
/// Snapshot replaces the service's facts, then reads `Since(None)` at once; Since adds and updates.
pub fn sync_activity(&self, svc: &dyn Service, input: SyncInput, on: &mut dyn FnMut(Id)) -> Result<SyncReport>;
pub fn service_unmatched(&self, service: &str) -> Result<Vec<(String, Option<i32>)>>;
```
- **Rules:**
  - A `FilmRef` with a `tmdb_id` resolves at once, and fixes any unresolved `service_films` row
    with the same `key`.
  - Without one, the `key` row is looked up first; it is searched only if it has never been
    tried, with an accepted match only.
  - Facts are stored at `on_day = on.unwrap_or(0)`. A `Rating`, `Like` or `Watchlisted` keeps
    one row per film (the newest replaces).
  - After storing, a watchlisted film with no item gets a `wanted = 1` movie row through
    `save_movie` (only with a TMDB key). A `wanted` row with no files whose film is no longer
    watchlisted is deleted.
  - Failures write `service_state.error` and return the error.

- [ ] **Step 1: Write the failing tests**
  - `snapshot_resolves_by_title_and_year_and_leaves_unsure_out`: the fake TMDB knows
    "Solaris" 1972 and 2002. Rows "Solaris|1972" resolve; "Untitled Thing|" has no accepted
    hit, so `unmatched == 1` and it is not searched again on a second snapshot (count the fake's
    search calls).
  - `a_later_feed_entry_with_a_tmdb_id_fixes_an_unmatched_row`.
  - `snapshot_then_since_keeps_newer_feed_entries`: a snapshot without the 2026-03-03 watch,
    then a feed holding it, so the watch is stored.
  - `watchlist_makes_wanted_rows_and_drops_them_when_gone`.
  - `a_file_identified_as_a_wanted_film_joins_its_row`: one `items` row for the TMDB id, with
    the file, `wanted = 0`, and the `watched_choice` set before kept (checked with plain SQL;
    `in_library` arrives in Task 4).
  - `forgetting_a_service_removes_its_facts_and_wanted_rows`.
- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p exhibit-core sync::` (expected: compile error).
- [ ] **Step 3: Implement `sync.rs` and the `Library` methods.** One write transaction per
  batch.
- [ ] **Step 4: Run the core suite.** Run: `cargo test -p exhibit-core` (expected: PASS).
- [ ] **Step 5: Commit**: `git commit -m "core: sync a service's activity, resolve its films, keep the watchlist"`

---

### Task 4: Activity in queries: detail, filters, sort, shelf, facets

**Files:**
- Modify: `crates/exhibit-core/src/query.rs`, `filter.rs`, `activity.rs`
- Test: in `query.rs` and `filter.rs`

**Interfaces:**
- **Produces:**
  - `ItemSummary.in_library: bool` (the item has a file in its subtree).
  - `ItemDetail.activity: Activity`:

```rust
pub struct Sourced<T> { pub service: String, pub on: Option<i64>, #[serde(flatten)] pub fact: T }
pub struct RatingFact { pub value: f32, pub scale: f32 }
pub struct ReviewFact { pub text: String, pub spoiler: bool }
pub struct WatchFact { pub rewatch: bool }
pub struct Activity {
    pub watched: bool, pub loved: bool,
    pub watched_choice: Option<bool>, pub loved_choice: Option<bool>,
    pub ratings: Vec<Sourced<RatingFact>>,      // newest first
    pub reviews: Vec<Sourced<ReviewFact>>,      // newest first
    pub watches: Vec<Sourced<WatchFact>>,       // newest first; Exhibit's watched play as service "exhibit", on = last_played_at
    pub liked_by: Vec<String>, pub watchlisted_by: Vec<String>,
}
```
  - **`Filters`** gains `rated`, `reviewed`, `logged` (a service `watch`, not Exhibit's) and
    `watchlist`, all `Option<bool>`. `watchlist: Some(true)` also lists `wanted` rows.
  - `SearchQuery.with_wanted: bool` (serde default false) lists `wanted` rows too; Ask uses it.
  - `Sort::MyRating`: the newest rating's `value / scale`, descending by default, unrated last.
  - `Shelf::Watchlist`: watchlisted and not watched, the owner's films first, then wanted
    rows, each newest-watchlisted first.
  - `Facets.activity: ActivityCounts { rated, reviewed, logged, watchlist, not_in_library }`.
  - Only enabled services count anywhere.
- **The listing:** `list` takes `wanted: bool`. When true it LEFT JOINs `lib` and admits
  `i.wanted = 1`. `available` is 0 for those rows.

- [ ] **Step 1: Write the failing tests**
  - `a_wanted_row_is_hidden_from_every_listing_unless_asked`: `search` default,
    `shelf(Unwatched)`, `facets` counts and `needs_review` all leave it out. `watchlist:
    Some(true)` and `with_wanted` list it with `in_library == false`.
  - `activity_filters_and_my_rating_sort`: three films, rated 4.5 / 2 / none. `Sort::MyRating`
    gives 4.5, 2, none, and `rated: Some(true)` gives two.
  - `detail_lists_activity_newest_first_with_exhibits_own_watch`.
  - `watchlist_shelf_puts_owned_first`.
  - `facets_count_activity_only_for_enabled_services`.
- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p exhibit-core activity wanted watchlist`.
- [ ] **Step 3: Implement.** Put the SQL fragments in `activity.rs` and use them from `filter.rs`
  and `query.rs`.
- [ ] **Step 4: Run the core suite, and time a search with every filter on the 5,000-work seed
  if it exists.** Run: `cargo test -p exhibit-core` (expected: PASS).
- [ ] **Step 5: Commit**: `git commit -m "core: activity in details, filters, the rating sort and the watchlist shelf"`

---

### Task 5: The Letterboxd adapter

**Files:**
- Create: `crates/exhibit-letterboxd/{Cargo.toml, src/lib.rs, src/rss.rs, src/export.rs, src/mark.svg, MARK-SOURCE.md}`,
  plus `tests/fixtures/feed.xml`
- Modify: root `Cargo.toml` (workspace member)

**Interfaces:**
- **Consumes:** Task 2's trait and types, and `exhibit_core::tmdb::Http`.
- **Produces:** `pub struct Letterboxd;` implementing `Service`, with
  - `info().id == "letterboxd"`, `name == "Letterboxd"`;
  - `provides: [Activity]`, `needs: [Username, ExportFile]`;
  - `export: Some(ExportHelp { page: <export page URL, confirmed live>, file_prefix: "letterboxd-", file_ext: "zip" })`.
- **Feed** (`https://letterboxd.com/{username}/rss/`), one `Dated` set per `<item>` whose guid
  starts `letterboxd-review-` or `letterboxd-watch-`; anything else is skipped.
  - `letterboxd:filmTitle` and `letterboxd:filmYear` give the `FilmRef`; `tmdb:movieId` gives
    its `tmdb_id`.
  - `letterboxd:watchedDate` gives `on`.
  - Every entry gives a `Watch { rewatch: letterboxd:rewatch == "Yes" }`.
  - `letterboxd:memberRating` (when present) gives a `Rating { scale: 5 }`.
  - `letterboxd:memberLike == "Yes"` gives a `Like`.
  - A review entry gives a `Review` from the description's paragraphs:
    - the image paragraph and a "This review may contain spoilers." paragraph are dropped (the
      latter sets `spoiler`);
    - paragraphs are joined by a blank line;
    - HTML entities are decoded.
  - A 404 is `service.user_not_found`; no channel items with a private profile is
    `service.private`; a network failure is `service.unreachable`.
- **Export** (`Snapshot(path)`), CSV with headers. A missing file is an empty set; a ZIP
  without `watched.csv` is `service.bad_export`.
  - `watched.csv` gives a `Watch` (no date).
  - `diary.csv` gives a `Watch` on `Watched Date`, with rewatch, and a `Rating` if present.
  - `ratings.csv` gives a `Rating`.
  - `reviews.csv` gives a `Review` on `Watched Date`.
  - `likes/films.csv` gives a `Like`.
  - `watchlist.csv` gives `Watchlisted` on `Date`.
  - Film refs come from `Name`/`Year` with no `tmdb_id`.
- `check` reads the feed and returns its film entries' count.

- [ ] **Step 1: Confirm the formats.**
  - Read one live public feed in a browser and compare its tags with the list above.
  - If the owner provides their export, compare its file and column names too.
  - Record any difference in this task before writing tests.
  - Write `feed.xml` by hand in the live structure, holding:
    - a rated review with a spoiler line;
    - a liked rewatch with no rating;
    - a plain watch;
    - a list entry;
    - a title with `&amp;`.
- [ ] **Step 2: Write the failing tests.**
  - `feed_entries_become_facts`: the exact facts for the five fixture items; the list entry
    gives none.
  - `review_text_drops_image_and_spoiler_line_and_decodes_entities`.
  - `export_rows_become_facts`: the ZIP is built in the test from CSV strings, including
    `"Crouching Tiger, Hidden Dragon"`, `"Amélie"` and two "Solaris" rows (1972, 2002).
  - `missing_user_is_user_not_found`: a fake `Http` returns 404.
- [ ] **Step 3: Run them to see them fail.** Run: `cargo test -p exhibit-letterboxd`.
- [ ] **Step 4: Implement.**
  - Use `roxmltree` for the feed, and `zip` (deflate only) with `csv` for the export.
  - Write a small entity decoder for `&amp; &lt; &gt; &quot; &#39; &#NNN; &#xHH; &nbsp;`.
  - Draw the mark as a one-colour SVG and note its source in `MARK-SOURCE.md`.
- [ ] **Step 5: Run the tests.** Run: `cargo test -p exhibit-letterboxd` (expected: PASS).
- [ ] **Step 6: Commit**: `git commit -m "letterboxd: an adapter for the export and the RSS feed"`

---

### Task 6: The registry and the CLI

**Files:**
- Create: `crates/exhibit-services/{Cargo.toml, src/lib.rs}`
- Modify: root `Cargo.toml`, `crates/exhibit-cli/Cargo.toml`, `crates/exhibit-cli/src/main.rs`

**Interfaces:**
- **Produces:**
  - `exhibit_services::all() -> Vec<Box<dyn Service>>` (Letterboxd, for now).
  - `exhibit_services::find(id: &str) -> Option<Box<dyn Service>>`.
- **CLI commands:**
  - `exhibit service list`;
  - `exhibit service user <id> <name>` and `exhibit service user <id> --clear`;
  - `exhibit service on <id>` and `exhibit service off <id>`;
  - `exhibit service import <id> <zip>`;
  - `exhibit service sync <id>`;
  - `exhibit service unmatched <id>`.
- **`exhibit search` gains** `--rated`, `--reviewed`, `--logged`, `--watchlist`,
  `--with-wanted` and `--sort my-rating`.
- Everything prints JSON as today.

- [ ] **Step 1: Write the failing test.** In `exhibit-services`, `ids_are_unique_and_known`
  (`find("letterboxd")` is `Some`; ids are distinct).
- [ ] **Step 2: Implement the registry and the CLI.** The CLI's `user` checks first, then saves
  and syncs `Since(None)`.
- [ ] **Step 3: Run the tests.**
  - Run: `cargo test -p exhibit-services -p exhibit-cli`, then
    `cargo run -p exhibit-cli -- service list`.
  - Expected: PASS, and JSON listing letterboxd with its state.
- [ ] **Step 4: Commit**: `git commit -m "services: the registry and the CLI's service commands"`

---

### Task 7: App commands, the worker and the export helper

**Files:**
- Create: `crates/exhibit-app/src/services.rs`
- Modify: `crates/exhibit-app/Cargo.toml`, `src/commands.rs`, `src/main.rs`, `src/worker.rs`

**Interfaces:**
- Consumes: Task 3's `Library` methods and Task 6's registry.
- **Produces, as Tauri commands:**

```rust
pub struct ServiceView { info fields (id, name, mark_svg, provides, needs, export_page: Option<String>), state: ServiceState }
#[tauri::command] fn services() -> ApiResult<Vec<ServiceView>>;
#[tauri::command] fn set_service_enabled(id: String, on: bool) -> ApiResult<()>;
#[tauri::command(async)] fn connect_service(id: String, username: String) -> ApiResult<u32>;   // check, save, queue Sync
#[tauri::command] fn forget_service(id: String) -> ApiResult<()>;
#[tauri::command] fn import_service_export(id: String, path: String) -> ApiResult<()>;          // queues Import
#[tauri::command] fn open_export_page(id: String) -> ApiResult<()>;                              // only the adapter's own page
#[tauri::command] fn find_service_export(id: String) -> ApiResult<Option<FoundExport>>;          // newest matching file in Downloads, newer than export_at
#[tauri::command] fn service_unmatched(id: String) -> ApiResult<Vec<(String, Option<i32>)>>;
pub struct FoundExport { pub path: String, pub name: String, pub modified: i64 }
```
  - **Worker:** `Job` becomes `Clone` (it is no longer `Copy`) and gains
    `SyncService(String)` and `ImportService(String, PathBuf)`. Both emit `Changed` when done.
    The `SyncReport` of the last import is kept in the job status, for Settings to say what
    came in.
  - **A thread in `services.rs`** queues `SyncService` for each enabled activity service with a
    username: at start, after the first refresh, and every 30 minutes.
  - **Downloads** is `SHGetKnownFolderPath(FOLDERID_Downloads)`, with
    `%USERPROFILE%\Downloads` as the fallback. It is read without recursion, and nothing in it
    is written.
  - **`open_export_page`** opens the URL in the default browser (the opener the app already
    uses, or `tauri-plugin-opener`).

- [ ] **Step 1: Write the failing tests.**
  - `worker`: `sync_jobs_queue_once_per_service` (the queue de-duplicates `SyncService("x")`).
  - `services.rs`: `find_export_picks_the_newest_matching_file_newer_than_the_last_import`,
    over a temp dir standing in for Downloads.
- [ ] **Step 2: Implement.** Register every command in `main.rs`.
- [ ] **Step 3: Run the tests.** Run: `cargo test -p exhibit-app` (expected: PASS).
- [ ] **Step 4: Commit**: `git commit -m "app: service commands, sync jobs and the export finder"`

---

### Task 8: The UI's API, the service mark and the merged state

**Files:**
- Modify: `ui/src/lib/api.ts`, `ui/src/lib/queries.ts`, `ui/src/lib/actions.ts` (`facetsOf`),
  `ui/src/i18n/locales/*.json`
- Create: `ui/src/components/ServiceMark.tsx`
- Test: `ui/src/lib/actions.test.ts`, `ui/src/components/ServiceMark.test.tsx`

**Interfaces:**
- **Produces:**
  - The TS mirrors of Task 4's and Task 7's types: `Activity`, `Sourced`, `ServiceView`,
    `FoundExport`, `ItemSummary.in_library`, the new filters, the `"my_rating"` sort and the
    `"watchlist"` shelf.
  - **Queries:** `useServices()` (key `["library", "services"]`).
  - **Mutations:** `useConnectService`, `useSetServiceEnabled`, `useForgetService` and
    `useImportServiceExport`. Each invalidates `["library"]`.
  - `hasActivity(services): boolean`: an enabled service that provides activity and has a
    username.
  - **`<ServiceMark service={id} size="panel"|"hero"|"line" />`** draws the adapter's
    `mark_svg` as a CSS mask filled with `currentColor`, sized like `FormatMark`, with
    `aria-label` = the service's name.
- **`facetsOf(detail)`** reads `activity.watched` and `activity.loved`. The summary is already
  merged.
- **Catalogue keys,** with words for every `service.*` error code.
- **Under `services.*`:**
  - `title` ("Services") and `note`;
  - `on`, `off`, `username`, `connect`, `checking`, `found`, `forget`;
  - `bringHistory`, `bringHistoryBody`, `openExport`, `waiting`, `foundExport`, `bringIt`,
    `chooseFile`;
  - `imported`, `watchlistFrom`, `newerExport`, `unmatched`.
- **Under `activity.*`:**
  - `yourRating`, `yourReview`, `history`, `spoilers`, `readReview`;
  - `playedInExhibit`, `loggedOn`, `rewatch`, `overridden`;
  - `rated`, `reviewed`, `logged`, `onWatchlist`, `notInLibrary`, `watchlistCount`, `shelf`.
- **English copy** (Spanish in the same voice), with `{{service}}` filled in from the adapter's
  name:
  - "Found @{{name}}: {{count}} recent diary entries. Your ratings and reviews will appear on
    your films."
  - "Letterboxd lets you download everything as one file. Exhibit will notice it in Downloads."
  - "Your {{service}} export from today is in Downloads. Bring it in?"
  - "{{logged}} films logged, {{rated}} rated, {{reviewed}} reviews, {{watchlist}} on your
    watchlist ({{wanted}} you don't have yet)."
  - "{{count}} films from your {{service}} export could not be matched surely and are left out."
  - "Watchlist from {{date}}."
  - "Contains spoilers. Press A to read."

- [ ] **Step 1: Write the failing tests.**
  - `facetsOf` takes the eye and heart from `activity`.
  - `ServiceMark` renders with the service's name as its label.
  - The existing catalogue-completeness test covers the new keys.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run the tests.** Run: `bun test` and `bunx --bun tsc --noEmit -p ui` (expected: PASS).
- [ ] **Step 4: Commit**: `git commit -m "ui: services in the API, the service mark, merged watched and loved"`

---

### Task 9: Settings: Services, with the guided export

**Files:**
- Modify: `ui/src/screens/Settings.tsx` (a new `ServicesPart` after `TmdbKey`)
- Test: `ui/src/screens/Settings.test.tsx`

**Interfaces:**
- Consumes: Task 8's queries, mutations and keys.
- **Behaviour:** one line per service from `useServices()`, drawn from `needs` and naming no
  site in code.
  - **The line itself:** the mark and name; an on/off switch (`components/ui/switch.tsx`);
    the status sentence; and `state.error` in words when set.
  - **Username:** `TextField` with the on-screen keyboard. Saving runs `connect_service` and
    shows `services.found` or the coded error.
  - **The export** (`ExportFile` plus `export_page`), the steps of spec section 8 in one place:
    - "Bring in your whole history and your watchlist" shows the two-line body and an "Open
      Letterboxd" action, which calls `open_export_page`.
    - While that step is open, `find_service_export` is polled every 2 s. A found file offers
      `services.foundExport` with Bring it in, which calls `import_service_export`.
    - "Choose the file" opens the existing `FolderBrowser` with `ext: "zip"`.
  - **After an import:** `services.imported` with the report.
  - **The watchlist's age:** `services.watchlistFrom`, plus `services.newerExport` when
    `export_at` is more than 30 days old.
  - **Unmatched:** `services.unmatched` when the count is above 0, listing the titles on
    request.
  - **Forget:** removes the username after the existing confirm dialog.
  - Focus moves through the part with the pad, as the other parts do.

- [ ] **Step 1: Write the failing tests** (mocked `invoke`).
  - `services_part_shows_each_service_from_the_registry`.
  - `saving_a_username_says_what_was_found`.
  - `a_found_export_is_offered_and_imported`.
  - `with_no_services_registered_the_part_is_absent`.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run the tests.** Run: `bun test ui/src/screens/Settings.test.tsx` (expected: PASS).
- [ ] **Step 4: Check it in the window.**
  - Run `scripts/dev.ps1 -Cdp` with a throwaway data dir (`-Data fresh`).
  - Drive the part with keys: connect a public username, then import a test ZIP.
  - Screenshot each step.
- [ ] **Step 5: Commit**: `git commit -m "ui: Services in Settings, with the guided export"`

---

### Task 10: Your rating, your review and History

**Files:**
- Modify: `ui/src/components/Placard.tsx`, `ui/src/screens/Details.tsx`, `ui/src/screens/Home.tsx`
  (the hero's held lines), `ui/src/styles/tokens.css`
- Test: `ui/src/components/Placard.test.tsx`, `ui/src/screens/Details.test.tsx`

**Interfaces:**
- Consumes: `ItemDetail.activity`, `ServiceMark` and `hasActivity`.
- **The placard:**
  - `PlacardLines` gains `verdicts: { mine: Sourced<RatingFact> | null }`. The scores of plan
    2 join this line later.
  - `data-line="verdicts"` comes after the facts. It shows stars (`★` for whole and `½` for
    half, on the rating's own scale), then the `ServiceMark`, sized by the placard's size.
  - The line is held (`min-h`) on a hero placard only while `hasActivity`, and absent
    otherwise.
  - For a film with no files, the facts say `activity.notInLibrary` instead of runtime and
    format.
- **Details:**
  - A film with no files has no play pill; the heart and eye remain (`Cues` takes
    `play={detail.files.length > 0}`).
  - **The second movement:**
    - **"Your review"** (`activity.yourReview`) comes first when `activity.reviews[0]` exists:
      stars, date and mark, then the text in `font-reading`, whole. A spoiler review shows
      `activity.spoilers` on one focusable item (a `FocusGroup` like the cast's); A reveals
      the text.
    - **"In Exhibit" becomes "History"** (`activity.history`):
      - every `activity.watches` entry as a line with its mark ("Played in Exhibit 12 March",
        "Logged on {{service}} 3 March 2019, rewatch");
      - an `activity.overridden` note when `watched_choice` disagrees with a service watch;
      - Added and Match lines as today.
- **Tokens:** `--verdict-star` (the stars' size in the panel and the hero), and
  `--details-review-measure` (the review's line length).

- [ ] **Step 1: Write the failing tests.**
  - `the_verdicts_line_shows_my_rating_with_its_mark`.
  - `no_verdicts_line_without_activity`.
  - `stars_for_4_5_of_5_are_four_and_a_half`.
  - `details_shows_the_review_and_hides_a_spoiler_until_pressed`.
  - `history_lists_every_watch_with_its_source`.
  - `a_film_not_in_the_library_has_no_play_cue`.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run the tests.** Run: `bun test ui/src/components/Placard.test.tsx ui/src/screens/Details.test.tsx` (expected: PASS).
- [ ] **Step 4: Check it in the window.** Use Home, the Movies panel and Details on films with
  and without a review, with keys and the mouse, in en-US and es-ES. Turn the service off and
  see every screen as before.
- [ ] **Step 5: Commit**: `git commit -m "ui: your rating on the placard, your review and History on Details"`

---

### Task 11: The wall, filters, sort, the watchlist shelf and Ask

**Files:**
- Modify: `ui/src/components/FilterSheet.tsx`, `ui/src/components/WallControls.tsx`
  (`sorts`), `ui/src/lib/filters.ts`, `ui/src/screens/Wall.tsx`, `ui/src/components/Work.tsx`,
  `ui/src/screens/Home.tsx` (`SHELVES`), `ui/src/screens/Ask.tsx`, `ui/src/lib/actions.ts`
  (`mainAction`)
- Test: `ui/src/lib/filters.test.ts`, `ui/src/screens/Wall.test.tsx`, `ui/src/screens/Home.test.tsx`

**Interfaces:**
- Consumes: Task 8's types and `hasActivity`.
- **Behaviour:**
  - **The filter sheet:** a "Your activity" section after "Watching", only while
    `hasActivity`, with chips Rated, Reviewed, Logged and On your watchlist, and counts from
    `facets.activity`. `filters.ts` handles them in `filterCount` and in its summary.
  - **Sort:** `sorts("movie")` gains `my_rating` ("Your rating") only while `hasActivity`.
  - **The watchlist view:** with `watchlist: true`, the wall lists wanted rows after the owner's
    films. The count reads `activity.watchlistCount` ("96 on your watchlist, 71 not in your
    library").
  - **A wanted work:**
    - `Work` sets `data-wanted` and draws dim, but not grayscale (offline stays grayscale);
    - `workLabel` adds "Not in your library";
    - `mainAction` gives Details;
    - the panel's `ActionLine` shows no play pill.
  - **Home:** a "watchlist" shelf (`activity.shelf`) after Continue watching, only while
    `hasActivity` and non-empty.
  - **Ask:** searches with `with_wanted: true`.
  - **Switching a service off** removes its section, sort, shelf and filters at once. A wall
    view holding a now-hidden filter drops it, as language changes drop genres today.

- [ ] **Step 1: Write the failing tests.**
  - `activity_section_only_with_activity`.
  - `my_rating_sort_only_with_activity`.
  - `watchlist_view_counts_and_dims_wanted_works`.
  - `watchlist_shelf_on_home`.
  - `turning_off_drops_hidden_filters_from_the_view`.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run the tests.** Run: `bun test` and `bunx --bun tsc --noEmit -p ui` (expected: PASS).
- [ ] **Step 4: Check it in the window.**
  - With a connected public username and a test export: drive the filter sheet, the sort,
    the watchlist view and the Home shelf with keys, a faked pad and the mouse.
  - Open a wanted film's Details.
  - Search in Ask.
  - Check the `?fps` path (the rAF recorder) on the Movies wall with the watchlist view on.
- [ ] **Step 5: Commit**: `git commit -m "ui: activity filters, your-rating sort, the watchlist view and shelf"`

---

## After the last task
- Update `.superpowers/README.md` with what was built and what is not yet seen in the window.
- Plan 2 (MDBList reception, the Rating sort, Reception on Details; Wikidata laurels) is
  written once a real MDBList key has confirmed its response fields.
