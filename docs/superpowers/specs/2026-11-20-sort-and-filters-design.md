# Sort and filters

## Intent
The wall's sort and filters were a first pass: four sorts, each with one fixed direction, and
on/off toggles for Not watched, Loved, Available, 4K and a single genre. The owner wants them "quite
powerful" while staying deterministic (the agent comes later): an order either way, quality
rather than a 4K switch, a year interval, and several genres at once.

## Decisions (owner)
- Sort ascending or descending.
- Filters more granular: quality instead of "4K", a year interval, two or more genres.
- Database and layers may change freely; the app has no users.

## Design
- **Core** (`exhibit-core/src/filter.rs`): `Filters`, flattened into `SearchQuery`.
  - Genres: all required, or any (`any_genre`); and genres left out (`without_genres`).
  - Ranges: `year_min/max`, `runtime_min/max` (minutes).
  - `seen`: unwatched, started, watched (a show is started when some episodes are watched).
  - `loved`, `available`.
  - Technical filters match when **any file beneath the work** has the value:
    - `resolution` (uhd, full_hd, hd, sd), by the window's mark bounds;
    - `picture` (dolby_vision, hdr10, hlg, sdr);
    - `sound` (atmos, dts_x, lossless, surround); commentaries never count;
    - `audio` and `subtitles` languages.
  - Within a facet the choices are alternatives; across facets they all apply.
  - Track languages are spelled one way by a SQL function, `track_lang`, registered on every
    connection: "eng", "en" and "ENG" are "en", "ger" is "de", and regions are kept.
  - Sorts: title, year, added, runtime and played (last watched; for a show, its latest episode).
    - Each takes `descending`; unset, the sort keeps its own direction.
    - Works without the value come last either way.
  - `facets(kind)` lists what a wall holds, with counts:
    - genres and languages, most common first;
    - years and runtimes, per value;
    - the formats present.
    It is one statement.
  - No migration: everything reads the stored tech JSON.
  - Checked at 5,000 works (release build): a search with every filter takes 50 ms, facets 132 ms.
  - CLI: `exhibit search` takes every filter (repeatable flags, plus `--reverse`); `exhibit facets`
    is new.
- **Window**:
  - **The row:**
    - Sort: Title, Year, Added, Runtime and Last watched. A picks a sort in its own order, A again
      turns it round, and a third A clears it (owner's request): no sort is lit and the wall takes
      the library's own order, titles A to Z. A wall opens with no sort.
    - An arrow shows the direction, and the hint names what A will do ("Oldest first", "Clear
      sort").
    - Filters are cleared by Clear filters in the row, Clear all in the sheet, or the button on
      "Nothing matches these filters".
    - Filters, with a badge counting the kinds in force.
    - The filters in one line: "Drama + Crime · not Horror · 1970 – 1989 · Spanish audio".
    - Clear filters, and the count.
  - **The sheet:**
    - Filters opens a sheet at the wall's right. Each choice applies at once: the wall rehangs
      behind a light veil, and the sheet's header says how many works are left.
    - B or Done closes it.
  - **Sections:** each shows only where the wall has something to choose. They are Watching,
    Collection, Genres, Year, Runtime, Picture, Sound, Audio and Subtitles.
  - **Chips:**
    - A chosen chip is filled; focus is the lamp.
    - A genre cycles through wanted, left out (struck through, with "−") and neither.
    - With two genres wanted, "Has all of them / Has any of them" appears.
  - **Year and Runtime:**
    - A bar for each year (or each 10 minutes) shows the wall's works, lit inside the span.
    - Two thumbs mark the ends. Up and Down move between the ends and out of the range.
    - Left and Right move an end; LB and RB (`[` `]`) jump 10 years or 30 minutes.
    - A thumb at the end of the track leaves that end open.
    - The hint line shows the d-pad's sides and the jump.
  - The view (sort, direction, filters) is kept in the navigation entry, as before.
  - A change of language forgets the chosen genres, since they are named in the language they
    were chosen in.
  - The hint line now follows an item whose A action changes while it has focus (a sort turned
    round, a chip chosen).

## Approaches considered
- **A dialog per facet**, opened from chips in the row (the old Genre dialog, generalised).
  Rejected for one sheet: every facet would cost a dialog open and close, and the row would
  overflow with nine chips.
- **UI SDK Slider** for the ranges. Its thumbs' inputs live inside its parts, out of reach of the
  focus system, so the pad could not land on them. Two native range inputs over one track are
  focus items like any other.
- **Facet columns stored at probe time.** Filtering would be faster, but it needs a migration, a
  backfill and a second copy of the tech. The JSON reads stay inside the budget, so they were not
  needed.

## Not done
- TMDB rating (as a sort and a filter): it is not stored yet and would need fetching for every
  work.
- Counts per choice under the other filters ("Drama 3" after narrowing): facets count the whole
  wall, and the sheet shows the total left instead.
