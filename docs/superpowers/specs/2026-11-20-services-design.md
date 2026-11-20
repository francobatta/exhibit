# Services: your Letterboxd, public ratings and laurels

Date: 2026-11-20. Status: awaiting owner approval.

Items marked **(call)** were decided by the design author, not the owner, and are open to
correction. The owner handed placement in the interface to the author ("we will correct later").

## 1. Intent
A cinephile who lives on Letterboxd should open Exhibit and find their taste already there:
their ratings, their reviews, what they have logged and what they want to see. Exhibit should
treat that activity with care, and set it beside the public's verdict (IMDb, Rotten Tomatoes,
Metacritic, Letterboxd's average) and a film's laurels.

Nothing here is Letterboxd code in Exhibit's core. Exhibit understands neutral facts about a
film (a rating, a review, a watch, a like, a place on a watchlist, a public score, an award),
each labelled with the service it came from. A service is an **adapter** that turns one site
into those facts. A second site later is a second adapter; the core and the screens do not
change.

## 2. Decisions (owner)
- **Public ratings come through an aggregator, MDBList**, not from Rotten Tomatoes or IMDb
  directly (both are licence-only). **Laurels come from Wikidata.** Both are adapters behind the
  same interface as Letterboxd, not core logic.
- **No Letterboxd API application.** Use what is free: the member's public RSS feed and their
  account export (a ZIP). Exhibit helps the owner through the export as much as it can, in
  friendly, plain words.
- **Letterboxd features:**
  - filter and sort by Letterboxd activity (reviewed, watched/logged);
  - the review on the details screen when there is one;
  - the rating wherever the film was rated;
  - the watchlist synced **one way**, Letterboxd to Exhibit.
- **The whole watchlist (option B):** films the owner does not have appear too, as unlit works
  that can be browsed but not played.
- **Letterboxd counts as watched (option B):** a film logged on Letterboxd lights the eye and
  leaves the unwatched shelves. This revises the main spec's non-goal "importing viewing history"
  for services only (see section 11).
- **Exhibit's own choice wins (option A):** the eye and the heart are always the owner's to press.
  Un-marking a film Letterboxd says was logged turns the eye off in Exhibit only; Letterboxd is
  untouched and Details still says "Logged on Letterboxd".
- **Pluggable.** Not every owner uses Letterboxd. With a service off, or never set up, Exhibit is
  exactly today's app: no empty rows, no greyed filters, no nagging.
- **Adapters are Rust crates compiled into Exhibit** behind one interface (approach A, section 4).
- **Placement in the interface is the author's call**, to be corrected after seeing it.

## 3. Goals, non-goals, anti-goals
**Goals**
1. The owner's Letterboxd activity shows in Exhibit's existing screens, marked as Letterboxd's.
2. Public ratings and laurels add to the placard and Details without crowding them.
3. One neutral model of film facts; every service is an adapter; adding one touches no screen.
4. Everything works offline from cache, and nothing waits on a service to draw.

**Non-goals**
- Writing to any service (logging, rating, adding to a watchlist). Adapters only read.
- The Letterboxd API, scraping Letterboxd pages, Rotten Tomatoes' critics consensus (licence-only).
- A plugin system: adapters are compiled in, switched on or off (the main spec's non-goal stands).
- Letterboxd for shows or episodes (Letterboxd is films only). Public ratings do cover shows.
- An Exhibit rating of its own. The owner rates on Letterboxd.
- Social features: friends' ratings, follower activity, lists other than the watchlist.

**Anti-goals** (failures even if it works)
- Settings multiply: services take one part of the settings screen, not a screen each.
- A Letterboxd row, filter or label shows when Letterboxd is not set up.
- A confident wrong match: an export row that cannot be matched surely is left out and counted,
  never guessed.
- A score that reads as Exhibit's own: every rating and review carries its source's mark.
- A screen waits on the network, or a service's failure shows as an error on a screen that did not
  ask for it.
- Rotten Tomatoes, IMDb or Letterboxd named in `exhibit-core`.

## 4. Approaches considered
| Approach | Trade-off | Outcome |
|---|---|---|
| **A. Rust crates behind one interface**, compiled in, facts stored in SQLite by TMDB id, refreshed by the existing worker | One language, one process, works offline; adding a service means a rebuild | **Chosen by the owner.** |
| B. Adapters as separate programs Exhibit runs | Truly pluggable, but it is the plugin system the spec rules out, and adds processes | Rejected. |
| C. Adapters in the UI | The UI never touches the network | Ruled out. |

**Letterboxd intake**
| Route | Trade-off | Outcome |
|---|---|---|
| **Export ZIP, then RSS** | Full history once, kept current for the diary; watchlist and likes refresh only on a new export; export rows have no TMDB id and must be matched | **Chosen**, with Exhibit guiding the export. |
| RSS only | No setup beyond a username, but about 50 entries and no watchlist | Rejected: fails the watchlist and history. |
| Scraping profile pages | Complete and current, but against Letterboxd's terms and fragile | Rejected. |
| The Letterboxd API | Complete, live, can write; by selective application | Not pursued (owner). The interface leaves room for it as a later adapter. |

**Aggregator**
| Option | Trade-off | Outcome |
|---|---|---|
| **MDBList** | One lookup by TMDB id gives IMDb, Rotten Tomatoes, Metacritic, Letterboxd's average and more; free key | **Chosen.** |
| OMDb | Needs IMDb ids, no Letterboxd average; has a one-line awards text | Rejected. |
| **Wikidata** for awards | No key; real award records with year and win or nomination | **Chosen** for laurels. |

## 5. The neutral model (core)
A new module, `exhibit-core/src/service.rs`, owns the model and the interface. It names no site.

**A film, as a service knows it** (`FilmRef`): the TMDB id and kind when the service has them,
else its own id, title and year. The core resolves a `FilmRef` to a TMDB film (section 7).

**Facts.** Each carries the service id that produced it and the service's own id for the fact,
so a re-sync replaces rather than duplicates.
- **Activity** (the owner's own):
  - `Rating { value, scale }`: the value on the service's own scale (Letterboxd: 0.5 to 5 in
    halves), shown on that scale.
  - `Review { text, written_at, spoiler }`.
  - `Watch { on: date, rewatch }`: one per diary entry; a film marked watched with no date is a
    `Watch` with no date.
  - `Like`.
  - `Watchlisted { since }`.
- **Reception** (the public's):
  - `Score { origin, value, scale, votes, url }`. `origin` names whose score it is (IMDb,
    Tomatometer, Popcornmeter, Metacritic, Letterboxd). It can differ from the adapter that
    fetched it: MDBList fetches IMDb's score, and the placard shows IMDb's mark.
  - `Award { body, category, year, won }` (Academy Awards, Best Picture, 1975, won).

**Sources and marks.** Every adapter describes itself:
`ServiceInfo { id, name, mark (one-colour SVG), provides: [Activity | Reception | Awards], connect }`.
An adapter that reports other origins (MDBList) describes each origin the same way (id, name,
mark). The UI draws whatever mark it is handed, as it draws format marks today. The marks are
in the adapter crates, with their sources noted, like `ui/src/assets/marks/SOURCES.md`.

**Connecting.** `connect` lists what the adapter needs from the owner, so Settings can draw it
without knowing the site: a username, a key (kept in the credential store like the TMDB key),
an export file, or nothing.

**The interface** (`trait Service`), each method optional by what `provides` says:
- `sync(&self, input: SyncInput) -> Result<ActivityBatch>`:
  - `SyncInput::Snapshot(file)` replaces everything this service said before.
  - `SyncInput::Since(time)` adds and updates what is newer.
  - A batch is facts plus the `FilmRef`s they are about.
- `reception(&self, films: &[TmdbFilm]) -> Result<Vec<(TmdbFilm, Vec<Score>)>>`, batched where
  the service allows.
- `awards(&self, films: &[TmdbFilm]) -> Result<Vec<(TmdbFilm, Vec<Award>)>>`.
- `check(&self, connection) -> Result<CheckOutcome>`: is this username, key or file good? It
  answers in words the owner can act on.
- Adapters take an HTTP client from the core, as `tmdb.rs` does, so tests can feed them recorded
  responses.

**Exhibit is a source too.** The owner's plays, eye and heart are facts from the `exhibit`
service, in the same shape, and one rule merges sources. Whether `play_state` and `loved` move
into the new table or are presented through it is the builder's call. The rule must live in one
place (a SQL view or one query helper), never repeated per screen.

**The merging rule:**
- **watched:** the owner's choice in Exhibit if made; else Exhibit played it to the end; else
  any activity service has a `Watch`.
- **loved:** the owner's choice in Exhibit if made; else any service has a `Like`.
- **The owner's choice is three-state:** not set, yes or no. Pressing the eye or the heart sets
  it. Today's `set_watched` and `set_loved` write it.
- **Playing a film to the end in Exhibit clears a "no" choice for watched** **(call)**. Watching it
  again is a newer answer than having un-marked it.
- **The owner's rating:** the newest `Rating` from any activity service, with its mark. Details
  lists every service's rating when there is more than one.
- **Watchlisted:** any service has `Watchlisted`, and the film is not watched.

## 6. Storage (one migration)
**(call)** Tables, plain SQL as the project requires:
- **`service_facts`**:
  - columns: service, service fact id, TMDB id and kind (nullable until resolved), the
    `FilmRef` as JSON, fact type, value as JSON, date;
  - unique on (service, service fact id);
  - indexed by (TMDB id, kind).
- **`service_scores`**:
  - columns: TMDB id, kind, via (adapter), origin, value, scale, votes, url, fetched_at;
  - unique on (TMDB id, kind, origin).
- **`service_awards`**: TMDB id, kind, via, body, category, year, won, fetched_at.
- **`service_state`**:
  - columns: service id, on/off, username, export file name and date, last sync time, last
    error, and the count of unmatched rows;
  - keys stay in the credential store.
- **On `items`:** `watched_choice` and `loved_choice` (NULL, 0 or 1) for the owner's three-state
  choice.

**Watchlist films the owner does not have** are `items` rows with no files: kind `movie`, matched
by TMDB id, with metadata fetched from TMDB as identification does today.
- Summaries gain `in_library` (the item has a file) next to `available` (a file is online now).
  "Not in your library" and "offline" are different states.
- **One row per TMDB film.** When a file is later identified as a film that already has a row,
  the file joins that row; no duplicate is made.
- When a film leaves the watchlist (a new export) and has no files, its row goes. Its activity
  facts stay.
- **Every existing listing leaves these rows out unless it asks for them** **(call)**: the walls,
  shelves, counts, facets, Needs review, the tool surface and the scanner's bookkeeping. Only
  the watchlist view, the Home watchlist shelf, Ask and Details show them.
- Without a TMDB key, watchlist films not in the library are not fetched and not shown. Films the
  owner has still show their Letterboxd activity.

## 7. Matching service films
- A `FilmRef` with a TMDB id (Letterboxd's RSS carries one) is used as is.
- A `FilmRef` with only title and year (Letterboxd's export) goes through the existing matcher
  against TMDB.
  - Only a sure match is used.
  - Anything else is left out, counted in `service_state`, and listed in Settings:
    "12 films from your Letterboxd export could not be matched surely and are left out."
  - A later RSS entry for the same film, which carries the TMDB id, fixes it.
- Resolved TMDB ids are cached per service fact, so a new snapshot does not search again for
  films it already knows.

## 8. The adapters
### Letterboxd (`crates/exhibit-letterboxd`)
- **provides:** Activity, and a Letterboxd average score if the RSS or export carries one (the
  aggregator otherwise brings it).
- **connect:** a username, and an export file.
- **Snapshot from the export ZIP:**
  - ratings, diary (watch dates, rewatches, ratings, reviews), reviews, watched films, likes
    and the watchlist;
  - the builder confirms the file and column names against a real export before relying on them.
- **Since, from `letterboxd.com/<username>/rss/`:**
  - new diary entries, ratings and reviews, with the TMDB id;
  - read on start, then every 30 minutes while the window is open **(call)**;
  - entries already present (from the export or an earlier read) are recognised and not
    duplicated;
  - the builder confirms the feed's fields against a live feed.
- **Helping the owner** (the owner asked for this, in friendly words) **(call: the steps)**:
  1. Settings asks for the username, on the on-screen keyboard as everywhere. Exhibit reads the
     feed and answers: "Found @name: 48 recent diary entries. Your ratings and reviews will
     appear on your films." A private diary or an unknown name gets a plain sentence saying what
     to check.
  2. "Bring in your whole history and your watchlist":
     - Exhibit explains in two lines that Letterboxd lets you download everything as one file.
     - It opens Letterboxd's export page in the default browser; the builder confirms the URL.
     - Exhibit then watches the Downloads folder for a new `letterboxd-*.zip` and offers it:
       "Your Letterboxd export from today is in Downloads. Bring it in?"
     - The folder browser picks the file anywhere else.
  3. After an import, Settings says what came in: "1,204 films logged, 312 rated, 48 reviews,
     96 on your watchlist (71 you don't have yet)."
  4. **The watchlist's age:** the Letterboxd part of Settings says "Watchlist from 3 November".
     It offers "Bring in a newer export" when the export is more than 30 days old. This shows
     only there, never as a banner **(call)**.

### MDBList (`crates/exhibit-mdblist`)
- **provides:** Reception.
- **connect:** a key, from the owner's MDBList account. Settings links the page that makes one
  and says what it is for.
- One lookup by TMDB id per film, batched if the API allows. It reports each origin it has: IMDb,
  Tomatometer, Popcornmeter, Metacritic, Letterboxd average, and others it carries. Each origin
  comes with its mark, and attribution as the origins require (Rotten Tomatoes asks for its
  name and a link).
- **Unconfirmed:**
  - the exact response fields, batch support and free-tier limit (reported 1,000 a day);
  - the builder confirms all three with a real key before building on them.
- **The adapter respects the limit:**
  - it reads the rate headers and stops for the day when they say so;
  - it resumes the next day, newest and most-watched films first.

### Wikidata (`crates/exhibit-wikidata`)
- **provides:** Awards. **connect:** nothing; on by default when a TMDB key is set **(call)**.
- One SPARQL query per batch of films by their TMDB id property, for awards received and
  nominations with their year. The builder confirms the property id (reported as P4947 for
  films) on Wikidata.
- **(call)** It keeps the awards that matter to a cinephile, a fixed list in the adapter:
  - the Academy Awards, Cannes, Venice, Berlin, the Golden Globes, BAFTA, Sundance, the Independent
    Spirit Awards and César;
  - wins and nominations for these, nothing else.

## 9. Refresh and failure
- **The worker does the work.** After identification it runs services: activity on start and on
  the RSS interval; reception and awards for library and watchlist films that have none, or whose
  scores are older than 7 days (films from the last year) or 60 days (older films) **(call)**.
- **Screens only read the cache.** A failing service records its error in `service_state`, shown
  in its part of Settings in plain words, and nowhere else. Screens show what was last fetched.
- **Offline,** everything shows from cache, and refresh waits for the connection.
- **Switching a service off** hides its facts everywhere at once and stops its refresh. Its data
  is kept, so switching it on again does not mean importing again. Removing the username or key
  deletes its data **(call)**.

## 10. In the interface (all placement is the author's call)
**Settings.**
- One new part, **Services**, after the TMDB key. One line per adapter: its mark, name, on or
  off, and what it needs (drawn from `connect`), with its status in a sentence. Nothing else
  moves.
- Letterboxd's line opens the steps in section 8. The agent key and the TMDB key stay where
  they are.

**The placard** (Home's hero, the wall's panel, Details):
- **A new verdicts line, after the facts.**
  - The owner's rating comes first, as stars with Letterboxd's mark. Then up to three public
    scores, each as its mark and number: IMDb 8.3, Tomatometer 98%, Metacritic 89. The fixed
    order is the owner's rating, IMDb, Tomatometer, Metacritic, Letterboxd average.
  - The line is drawn only when a service with something to say is on. The hero holds its
    height only when a service is on, so Home does not jump.
- **"Not in your library"** replaces the runtime-and-format facts for a watchlist film the owner
  does not have.

**The wall.**
- Posters stay clean: no ratings on works. Watched and loved badges follow the merged state.
- **Sort** gains "Your rating" (when an activity service is on) and "Rating" (when a reception
  service is on). "Rating" is the average of the public scores on a 0 to 100 scale, so it names
  no site.
- **The filter sheet** gains a "Your activity" group, only when an activity service is on:
  Rated, Reviewed, Logged and On your watchlist. Loved already covers likes.
- **Watchlist films not in the library** hang on the Movies wall only when "On your watchlist" is
  chosen, unlit, after the ones the owner has.
  - The wall's count says both: "96 on your watchlist, 71 not in your library".
  - The default wall stays the owner's collection.

**Home.**
- One shelf, "On your watchlist", when there is one. Films the owner has come first; then the
  others, unlit.
- No other new shelves.

**Details.**
- **Laurels, the strong idea.** Wins stand above the title as laurel wreaths drawn in the text
  colour, the way a festival poster wears them: "Palme d'Or, Cannes 1976".
  - At most three wreaths, wins before nominations, the major bodies first.
  - More than three becomes a line under them: "and 4 more awards".
  - No colour; no wreath for a film without wins.
- **The first movement's cues:** a film not in the library has no play pill. The heart and the eye
  stay.
- **The second movement gains two parts:**
  - **"Your review"**, placed before the cast, when there is one:
    - the stars, the date and the Letterboxd mark, then the text in the reading face, in full;
    - a spoiler review is folded behind one press: "Contains spoilers. Press A to read".
  - **"Reception"**, next to "The file":
    - each public score with its mark, number and vote count;
    - Metacritic shown as a square badge in the text colour;
    - then every award the adapter kept, wins and nominations, by year.
- **"In Exhibit" becomes "History"**, with every watch from every source and its mark: "Played in
  Exhibit 12 March", "Logged on Letterboxd 3 March 2019 (rewatch)". It also says where the eye's
  state comes from when the owner's choice overrides a service: "You marked it unwatched here;
  Letterboxd has it logged."

**Ask (plain search)** finds watchlist films not in the library too, unlit and marked. The agent's
tool surface (main spec section 12) gains the same filters and sorts, and the facts on an item's
detail.

**States:** each screen's existing loading, empty, offline-drive and error states cover services
without new ones. A service that has fetched nothing yet simply draws nothing.

**Language:** every new string goes through the catalogues (en-US, es, es-ES, es-MX). Dates use
the date setting. Review text is shown as written.

## 11. Changes to the main spec
- **Non-goals:** "Importing viewing history … from any other app" becomes: "except through a
  service the owner connects (Letterboxd), whose watches count as watched under the owner's own
  choice in Exhibit". The line about Kodi, Jellyfin and Plex stays.
- **Settings** (section 14) lists Services as one part.
- "Accounts" stays a non-goal: Exhibit has none; it only reads services the owner already uses.

## 12. Constraints
- **Bun and Rust only;** plain `rusqlite` with hand-written SQL.
- **The UI never touches the network or the files.** Everything comes through Tauri commands
  over `Library`.
- **Keys live in the credential store.** Tests never touch the owner's data directory or keyring.
- **Adapters are tested against recorded responses,** never live:
  - a sample RSS feed;
  - a small export ZIP;
  - MDBList JSON;
  - a Wikidata result.
- **No proprietary names in `exhibit-core`:** a test greps it for the site names.
- **5,000 works still hold 60 fps.** The new columns join in the existing summary query without
  a per-row lookup.

## 13. Build order
1. **The neutral model and storage:** the facts tables, the three-state choices, Exhibit as a
   source, and the merged watched and loved state in summaries and filters. No visible change
   yet.
2. **The Letterboxd adapter:** export snapshot, RSS, matching, and the Services part of Settings
   with the guided export.
3. **The owner's activity in the interface:** the verdicts line with the owner's rating, the
   activity filters and the "Your rating" sort, "Your review", and History.
4. **Watchlist films not in the library:** items without files, "not in your library", the
   wall's watchlist view and the Home shelf.
5. **The MDBList adapter:** reception on the placard, the "Rating" sort, and Reception on Details.
6. **The Wikidata adapter:** laurels and the awards list.

Each stage is checked in the dev window with keys, a pad and the mouse, in English and Spanish,
with services on and off.

## 14. Left to the builder
- Whether Exhibit's own plays move into `service_facts` or are presented through it (section 5).
- Recognising a diary entry seen in both the export and the RSS feed.
- The laurel wreath drawing, made once as an SVG in the text colour.
- **Confirming against the real thing before relying on it:**
  - the export's files and columns;
  - the RSS fields;
  - the export page URL;
  - MDBList's fields, limits and batching;
  - Wikidata's TMDB property.
