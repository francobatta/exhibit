# The hero placard as a wall label: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reshape the hero placard (Home, Details, Show) into a wall label: a larger title, a bright byline, then the description beside the verdicts as large figures, with one quiet line about the file.

**Architecture:** `placardLines` gains the byline's new order and a short languages phrase. `Placard`'s `hero` size renders a new body: a container-query grid that sets the story and the verdicts side by side when the placard is wide, and stacks them when it is narrow. The `panel` size is untouched apart from the byline's order. Tokens carry every size; Home and Details widen to a new hero measure; the backdrop's left gradient reaches further.

**Tech Stack:** React 19, Tailwind 4 (container query variants), react-i18next, Bun test with happy-dom.

**Spec:** `docs/superpowers/specs/2026-11-21-hero-placard-design.md`

## Global Constraints

- Bun only: `bun test`, `bunx --bun tsc --noEmit`, run from `ui/`. Never `node`, `npm` or `npx`.
- Tokens live only in `ui/src/styles/tokens.css`; everything sized in `rem`; no new colour.
- Animate only `transform`, `opacity` and `filter` (this change adds no motion).
- Every new string goes in `en-US.json` and `es.json` (es-ES and es-MX hold only regional differences).
- The hero placard on Home holds its lines across works: nothing moves as focus crosses works.
- Look at the result in the running window (`/dev` skill, `bun scripts/cdp.ts keys|shot`) before calling a task done that changes the look.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; subjects in the repo's style (`ui: …`).

## Review Focus

1. **A work with scores next to one without, on Home.** Moving between them must not move the placard: the verdicts row keeps its height, empty. Test in Task 2.
2. **No service on.** The verdicts row does not exist; the right column holds the file line alone, and nothing empty is drawn. Test in Task 2.
3. **Show's narrow placard (42% beside the seasons) and a small window.** The body stacks instead of squeezing two columns. Not testable in happy-dom; checked in the window in Task 3.
4. **Long Spanish language names and many tracks.** The file line never wraps or pushes the marks off; the languages truncate. Test in Task 1 for the phrase; window check in Task 3.
5. **A film not in the library, and an episode.** The file line says "Not in your library" with no languages; an episode's byline keeps "show, episode, year". Tests in Task 1.

---

### Task 1: The byline's order and the file line's languages

**Files:**
- Modify: `ui/src/components/Placard.tsx` (`placardLines`, `PlacardLines`, a new `heardAndRead`)
- Modify: `ui/src/i18n/locales/en-US.json`, `ui/src/i18n/locales/es.json`
- Test: `ui/src/components/Placard.test.tsx`

**Interfaces:**
- Produces: `PlacardLines.languages: string | null`, the hero's short phrase ("English, 19 subtitles").
- Produces: `export function heardAndRead(tech: TechInfo | null): string | null`.
- Produces: catalogue keys `units.subtitle_one|_other` (es: `_one|_many|_other`), `placard.noSubtitles`, `placard.yours` (used in Task 2).
- `PlacardLines.credit` for a film or show is now `facts([director, year])`; episodes unchanged.

- [ ] **Step 1: Write the failing tests** in `Placard.test.tsx`:
  - Update `year and director; …` to expect `credit: "Martin Scorsese, 1976"` and `languages: "No subtitles"`. (No audio tracks, no subtitle tracks: there is nothing heard to name, so the phrase is only the subtitles part.)
  - `the_file_line_names_what_is_heard_and_counts_the_subtitles`:
    ```ts
    const tech = (audio, subtitles) => ({ duration_s: 1, width: 1920, height: 1080, video_codec: "h264", hdr: null, audio, subtitles });
    expect(heardAndRead(null)).toBeNull();
    expect(heardAndRead(tech([audioTrack("eng")], ["eng","chi","cze","ger"].map((l) => subTrack(l))))).toBe("English, 4 subtitles");
    expect(heardAndRead(tech([audioTrack("eng")], [subTrack("eng")]))).toBe("English, 1 subtitle");
    expect(heardAndRead(tech([audioTrack("eng")], []))).toBe("English, no subtitles");
    // Forced tracks are not subtitles of the film; unnamed tracks are counted.
    expect(heardAndRead(tech([audioTrack("eng")], [subTrack("eng", { forced: true }), subTrack(null), subTrack(null)]))).toBe("English, 2 subtitles");
    // Heard languages as today: original first, commentaries left out, three named then "and N more".
    ```
    Use the fixture helpers' real signatures (`test/fixtures.ts`); adjust the call shapes above to them.
  - `an episode names its show and place` stays as it is (episodes keep their order).
  - In `a film with no file says…`: `expect(placardLines(wanted.item, wanted).languages).toBeNull()`.
- [ ] **Step 2: Run them to see them fail:** `cd ui && bun test src/components/Placard.test.tsx` → FAIL on credit order and the missing `heardAndRead`.
- [ ] **Step 3: Implement.** In `placardLines`, set the film credit as `facts([director, year])` and add `languages: absent ? null : heardAndRead(tech)`. `heardAndRead` reuses `trackLines`' filtering. It names the heard languages with `named(...)`, then the subtitles: the count of distinct named languages, else the count of unnamed tracks, as `t("units.subtitle", { count })`, or `t("placard.noSubtitles")` when there are none. The two parts are joined with `facts([...])`. When nothing is heard, the phrase is the subtitles part alone, with its first letter upper-cased in the UI language ("No subtitles"). Catalogue: en `"subtitle_one": "{{count, number}} subtitle"`, `"subtitle_other": "{{count, number}} subtitles"`, `"noSubtitles": "no subtitles"`, `"yours": "Yours"`. es: `"subtítulo"/"subtítulos"` (one/many/other), `"sin subtítulos"`, `"Tu nota"`.
- [ ] **Step 4: Run the tests:** `cd ui && bun test src/components/Placard.test.tsx` → PASS. Then `bun test` and `bunx --bun tsc --noEmit` → no failures. Fix any other test that pinned "1976, Martin Scorsese" (grep `ui/src` for it).
- [ ] **Step 5: Commit:** `ui: the placard's byline names the director first; a short languages phrase for the hero`.

### Task 2: The hero placard's body

**Files:**
- Modify: `ui/src/components/Placard.tsx` (hero rendering; the panel path unchanged)
- Modify: `ui/src/components/ServiceMark.tsx` (`HEIGHT` gains `caption: "h-(--mark-caption)"`)
- Modify: `ui/src/styles/tokens.css` (new tokens below; the container size in a plain `@theme` block, because a container query cannot read `var()`)
- Test: `ui/src/components/Placard.test.tsx`

**Interfaces:**
- Consumes: `PlacardLines.languages`, `placard.yours` (Task 1).
- Produces: hero DOM, in order: `[data-line=title]`, `[data-line=credit]`, `[data-line=body]` holding `[data-line=story]` (overview, then `more` lines) and `[data-line=verdicts]`, then `[data-line=facts]`. `[data-line=tracks]` no longer exists on the hero. The panel's lines are unchanged.
- Produces tokens: `--verdict-figure: 3rem` (a score's figure), `--verdict-figure-stars: 2.25rem`, `--mark-caption: 1rem`, `--verdict-figure-gap: 2.5rem`, `--placard-split-gap: 4rem`, `--placard-side: 30rem` (the right column's width), and in `@theme { --container-placard-split: 60rem; }`, giving the `@placard-split:` variant. Starting values only; Task 3 settles them in the window.

- [ ] **Step 1: Write the failing tests:**
  - `the_hero_sets_the_story_beside_the_verdicts`: render a hero with `rated(4.5)` plus `reception` from the existing scores test and `[service(), scoresService()]`. Assert `lineNames` order is `["title","credit","body","story","overview","verdicts","facts"]`. Each `[data-score]` holds its `ScoreValue` and then its `OriginMark`. Under the stars sit the Letterboxd mark and the text "Yours". `[data-line=tracks]` is absent. The facts line's text contains the languages phrase.
  - Update `the_verdicts_line_shows_my_rating_with_its_mark`: on the hero, verdicts come after `story` and before `facts`; on the panel, after `facts` as today.
  - `the_held_hero_keeps_an_empty_verdicts_row`: `<Placard item={summary()} size="hero" holdVerdicts />` has `[data-line=verdicts]` with empty text. Without `holdVerdicts` it is absent, and `[data-line=facts]` is still present.
  - The existing `awkward metadata…` and `empty lines are left out…` tests keep passing unchanged.
- [ ] **Step 2: Run them to see them fail:** `cd ui && bun test src/components/Placard.test.tsx` → FAIL on the line order.
- [ ] **Step 3: Implement.**
  - Split the hero path into a small `HeroBody` component in `Placard.tsx`. Keep the panel path as it is.
  - **The placard:** the hero wrapper drops `max-w-measure` (each screen sets the width) and becomes `@container/placard`.
  - **The byline:** `text-heading font-ui text-label`.
  - **`body`:** one column by default. At `@placard-split:` it becomes a grid of `minmax(0,1fr) var(--placard-side)` with `--placard-split-gap` between, aligned at the end, so the figures sit on the description's last line.
  - **`story`:** the overview, with its current clamp and hold rules. Then the `more` lines, `text-body text-label-dim`.
  - **`verdicts`:** a row of figures `--verdict-figure-gap` apart. Each figure is a column: the value on top, then a caption row at `--mark-caption`. The owner's stars at `--verdict-figure-stars` have `ServiceMark size="caption"` plus `t("placard.yours")` under them. A score shows `ScoreValue` at `--verdict-figure`, Metacritic's badge included, with `OriginMark size="caption"` under it. It is held at the figure's full height when `hold && holdVerdicts`.
  - **`facts` (the file line):** inside the right column when split, below the story when stacked. It holds the `facts` text, then the format marks (`FormatMark size="hero"`), then `languages` in a `min-w-0 truncate` span. Only the text truncates; the marks never shrink.
- [ ] **Step 4: Run the tests:** `cd ui && bun test` and `bunx --bun tsc --noEmit` → PASS.
- [ ] **Step 5: Commit:** `ui: the hero placard sets the story beside the verdicts as figures`.

### Task 3: The screens, the scrim, and the look in the window

**Files:**
- Modify: `ui/src/styles/tokens.css`. `--size-hero: 8rem`. A new `--placard-hero-measure: 72rem` mapped as `--spacing-measure-hero`. Scrim stops `--scrim-hold: 40%` and `--scrim-reach: 72%`. Tune Task 2's starting values here.
- Modify: `ui/src/styles/base.css` (`.backdrop-scrim`'s left gradient reads `--scrim-hold` and `--scrim-reach` in place of 28% and 58%)
- Modify: `ui/src/screens/Home.tsx:146` and `ui/src/screens/Details.tsx:207` (`max-w-measure` → `max-w-measure-hero`). `Show.tsx` keeps `max-w-measure`, so its placard stacks.
- Test: existing `Home.test.tsx`, `Details.test.tsx` and `Show.test.tsx` keep passing.

**Interfaces:**
- Consumes: the hero DOM and tokens from Task 2.

- [ ] **Step 1: Make the changes above;** then run `cd ui && bun test` and `bunx --bun tsc --noEmit` → PASS.
- [ ] **Step 2: Look in the window.** Start with `scripts/dev.ps1 -Cdp`, in the background. Then check each case with `bun scripts/cdp.ts shot` and settle the tokens:
  - **Home at 1920×1080:** Taxi Driver; a work without scores (the placard must not move as you cross it with `keys ArrowRight`); a long title; a show.
  - **Details** of a film with laurels and a long description, and of a film you don't own.
  - **Show**, where the placard is narrow and must stack.
  - **Home in es-ES** (Settings → Language), for the file line's length.
  - **Full screen** (`-Fullscreen`) once.
  - Then drive focus from the shelves up to the cues and back with keys, to check that focus and the action line are unchanged.
- [ ] **Step 3: Commit:** `ui: the hero placard widens on Home and Details; the backdrop's shade reaches further`.
- [ ] **Step 4:** Update `.superpowers/README.md` with a line under Done for the hero placard, in the same voice as the entries there.
