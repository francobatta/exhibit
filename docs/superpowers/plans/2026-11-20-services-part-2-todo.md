# Services, part 2: what is left

Date: 2026-11-20. Status: a to-do list for writing plan 2, not the plan itself.

Spec: `docs/superpowers/specs/2026-11-20-services-design.md`. Plan 1
(`docs/superpowers/plans/2026-11-20-services-letterboxd.md`) built the spec's stages 1 to 4 on
branch `services` (head 227f9ac, not merged). This file lists everything left for stages 5
(MDBList) and 6 (Wikidata). It also lists the loose ends plan 1 left behind.

## 0. Before plan 2 can be written
- **An MDBList key.** The response fields, batching and limits could not be read without one
  (the title lookup answers 401). Make one at https://mdblist.com/preferences/#api.
- **The response to confirm,** for one film and one show, with that key:
  - `GET https://api.mdblist.com/tmdb/movie/{id}?apikey=…`, and the same for a show;
  - which rating sources it returns (IMDb, Tomatometer, Popcornmeter, Metacritic, Letterboxd
    average, others) and the fields of each (value, score, votes, url);
  - whether it says "certified fresh";
  - whether a batch lookup exists;
  - the free tier's daily limit (reported secondhand as 1,000 a day; the docs only say "depends on
    your supporter tier", readable at `GET /user`).
- **How the owner signs in** (owner's request: explore at
  https://api.mdblist.com/docs/authentication/). The options the docs list:
  - **API key:** "best for personal scripts"; sent as `?apikey=`.
  - **Device code:** "for TVs, CLIs and devices that cannot receive a redirect". It suits a
    gamepad-driven TV app: Exhibit shows a code, the owner enters it on mdblist.com.
  - **Public PKCE:** "best for desktop… open-source and local apps". It needs an app registered
    at https://mdblist.com/developer and a loopback redirect (`http://127.0.0.1:…/callback`).
  - **OAuth 2.0 with a secret:** for servers; not for Exhibit.
  - Tokens last 30 days, with a refresh token. The only scope is `write` ("full access to the
    user's profile, lists, watchlist, ratings…").
  - **To decide with the owner:**
    - API key: simplest, the same Settings shape as the TMDB key.
    - Device code or PKCE: no key to copy, but Exhibit must register an app and refresh tokens.
    - Whichever is chosen, Exhibit only reads, though the token can write.
- **Wikidata's properties,** checked on wikidata.org:
  - the TMDB movie id (reported as P4947) and the TMDB TV series id;
  - "award received" (P166) and "nominated for" (P1411), with the "point in time" qualifier for
    the year.
- **One SPARQL query tried by hand** at https://query.wikidata.org: the awards of a few known
  films (Taxi Driver: Palme d'Or 1976) through their TMDB ids. Note the time it takes for a batch
  of 50.

## 1. Core (`exhibit-core`), still site-agnostic
- **The `Service` trait** gains the two methods the spec defines but plan 1 left out (today it
  has only `info`, `activity` and `check`):
  - `reception(&self, films: &[TmdbFilm], http) -> Result<Vec<(TmdbFilm, Vec<Score>)>>`;
  - `awards(&self, films: &[TmdbFilm], http) -> Result<Vec<(TmdbFilm, Vec<Award>)>>`;
  - both default to `service.unsupported`, like `activity`.
- **New neutral types** in `service.rs`:
  - `Score { origin, value, scale, votes, url }`;
  - `Award { body, category, year, won }`;
  - `TmdbFilm { kind, tmdb_id }`;
  - `OriginInfo { id, name, mark_svg }`: the adapter describes each origin it reports, so
    MDBList hands over IMDb's mark and the placard shows IMDb's mark, not MDBList's.
  - `ServiceInfo` gains `origins: &'static [OriginInfo]`.
- **The HTTP trait needs response headers.** `tmdb::Http::get` returns only `(status, body)`;
  MDBList's limits come in `X-RateLimit-Limit`, `-Remaining` and `-Reset`, and a 429 carries
  `Retry-After`. Extend the trait, or add a second method, without breaking TMDB's use.
- **Migration 006:**
  - `service_scores(kind, tmdb_id, via, origin, value, scale, votes, url, fetched_at)`, unique
    on (kind, tmdb_id, origin);
  - `service_awards(kind, tmdb_id, via, body, category, year, won, fetched_at)`;
  - both gated by `service_state.enabled`, like the facts.
- **Refresh** (spec section 9), as a step the worker runs after identification:
  - films in the library or on the watchlist that have no scores, or whose scores are older
    than 7 days (films from the last year) or 60 days (older films);
  - newest and most-watched first;
  - stop for the day when the rate headers say so, and record that in `service_state`;
  - awards refresh rarely (60 days) and only for films.
- **Queries:**
  - `ItemDetail` gains `reception: Vec<SourcedScore>` and `awards: Vec<Award>`, from enabled
    services only.
  - `Sort::Rating`: the average of the public scores on 0 to 100, unrated last, named after
    no site.
  - Facets need nothing new.
- **No site names in the core:** the existing `core_names_no_site` test already covers
  `mdblist`, `wikidata`, `tomato` and `metacritic`; IMDb stays allowed (NFO ids).
- Tests against fakes only, never the network, as plan 1 did.

## 2. The MDBList adapter (`crates/exhibit-mdblist`)
- `provides: [Reception]`. `needs`: `[Key]`, or none if device code or PKCE is chosen (then a
  new `Need`, for example `SignIn`).
- **One lookup per film** by TMDB id, batched if the API allows. Map each returned rating to a
  `Score` with its origin, whichever of these it carries:
  - IMDb;
  - Tomatometer and Popcornmeter;
  - Metacritic;
  - the Letterboxd average;
  - others, kept or dropped deliberately.
- **Marks:** one one-colour SVG per origin, with a `MARK-SOURCE.md` like the Letterboxd crate.
  Rotten Tomatoes asks for its name and a link back. Decide how a TV app without a browser link
  honours that (name always shown; the URL in Details' Reception lines?).
- `check`: confirms the key (or token) and returns the remaining daily requests.
- **Error codes** reuse the generic `service.*` set; add `service.limit_reached` with words in
  all catalogues.
- **Fixtures** written by hand in the confirmed response shape, never a real response with
  someone's key.

## 3. The Wikidata adapter (`crates/exhibit-wikidata`)
- `provides: [Awards]`, `needs: []`. On by default when a TMDB key is set (spec section 8); its
  `service_state` row must be created then.
- **One SPARQL query per batch** of films by TMDB id: awards received and nominations, with the
  year.
- **The fixed list of bodies** (spec section 8): Academy Awards, Cannes, Venice, Berlin, Golden
  Globes, BAFTA, Sundance, Independent Spirit and César. Wins and nominations for these only.
- **Wikimedia asks automated clients for a descriptive User-Agent.** Use a generic client name
  ("Exhibit, a desktop media gallery") with no personal details, per the project's privacy rule.
- Error codes: `service.unreachable`, and a timeout from the query service handled as
  unreachable.

## 4. App (`exhibit-app`)
- **A key-based service in Settings:**
  - `SecretName` in `commands.rs` knows only `tmdb` and `agent`; service keys are read from the
    keyring under the service id (plan 1, Task 3). Add a command to set or clear a service's key
    (checked first, like `connect_service`), or open `SecretName` to registered service ids.
  - With device code or PKCE instead: commands to start the sign-in, poll it, and refresh the
    token.
- **The worker:**
  - a `Job::Reception` step (scores and awards) after identification and on the refresh
    interval;
  - its progress shown like the other phases;
  - never blocking the window.
- The registry (`exhibit-services::all`) adds both adapters.

## 5. Window (UI)
- **Settings, Services:**
  - draw a `Key` need (a `SecretForm`-like field with Paste) or the sign-in flow, still from
    the adapter's `needs` and naming no site in code;
  - show the remaining daily lookups or "limit reached until …" in the status sentence;
  - Wikidata's line has no field, only on/off.
- **The placard's verdicts line:**
  - `PlacardLines.verdicts` today holds only `mine`; add `public: SourcedScore[]`;
  - up to three public scores after your rating, each as its mark and number, in the fixed
    order your rating, IMDb, Tomatometer, Metacritic, Letterboxd average (spec section 10);
  - the line is held on Home only while a service that says something is on.
- **The wall:**
  - the "Rating" sort (`sorts.rating`, already in the catalogues) only while a reception service
    is on;
  - posters stay clean, with no scores on works.
- **Details:**
  - **Laurels (the spec's strong idea):**
    - wins stand above the title as laurel wreaths drawn in the text colour: "Palme d'Or, Cannes
      1976";
    - at most three, wins first, major bodies first;
    - more becomes "and 4 more awards";
    - no wreath for a film without wins;
    - the wreath is one SVG made once;
    - check that the title never shifts when laurels load (hold or reserve the space).
  - **"Reception"** next to "The file":
    - each score with its mark, number and vote count;
    - Metacritic as a square badge in the text colour;
    - then every kept award, wins and nominations, by year.
- **Switching a service off** removes its scores, sort, laurels and part at once, as Letterboxd's
  filters do now.
- **Words:** every new string in en-US and es (es-ES and es-MX fall back to es unless the wording
  differs).
- **Fluency:** walls with 5,000 works still hold 60 fps, so reception joins the summary only if
  the Rating sort needs it, as an SQL expression, never a per-work call. Measure with the rAF
  recorder (there is still no `?fps` meter).

## 6. CLI and the agent's tools
- **`exhibit service`:**
  - `key <id> <key>` and `key <id> --clear`, or the sign-in equivalents;
  - `refresh <id>`.
- `exhibit search --sort rating`.
- `exhibit item <id>` shows `reception` and `awards`.

## 7. Loose ends from plan 1
- **Not yet confirmed against the real thing:**
  - the Letterboxd export layout, built from secondhand column lists (the owner has no export
    yet);
  - the export page URL, `https://letterboxd.com/settings/data/` (it needs a login);
  - the new Settings controls, filters and shelf with a real gamepad (keys, the mouse and a
    faked pad only so far).
- **The dev library holds test Letterboxd data:** @davidehrlich's public feed and a scratchpad
  export with Taxi Driver, Heat and Paris, Texas. Clear it with Settings → Services → Forget
  Letterboxd before judging the screens with real data.
- **Branch:** `services` (from `i18n`) is not merged; the owner has not yet said merge, PR or
  wait.
- **Smaller items, none blocking:**
  - Films un-marked as watched before migration 005 have no saved "no"; a Letterboxd log can
    light their eye (one press turns it off).
  - `continue_watching` still reads Exhibit's own play state on purpose: a part-way rewatch
    stays resumable.
  - Facets rose from 131 to 174 ms, and needs_review from 38 to 66 ms, at 5,000 works. Recheck
    after reception joins.
  - The CLI prints an error's kind and message but not its code.
  - `find_entry` in the Letterboxd export reader stops at the first ZIP entry whose name cannot
    be read.
  - Wanted works look like any unlit work until focused; `data-wanted` is there if they should
    stand out more.
  - The Home shelf is titled "Your watchlist" (spec: "On your watchlist"); a one-key swap if
    the owner prefers the spec's wording.
  - The spec's Letterboxd adapter "Letterboxd average if the feed carries one": the feed does
    not carry it, so it comes from MDBList in plan 2.
