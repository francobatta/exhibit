# The hero placard as a wall label

The hero placard (Home, Details, Show, the dev gallery) is reshaped so it reads at three metres
without tiring the eye: fewer, bolder steps, grouped by kind, using the width the window has. Same
information as today.

## Why

On Home the placard is a 6.5rem title over six lines of the same small, dim size (credit, facts and
marks, verdicts, languages, description). Nothing after the title leads the eye, so every line has
to be read to find anything; the verdicts line mixes six kinds of mark; the placard keeps to a 46rem
column while the space above the title and to its right goes unused. It reads as a fact sheet.

## Goals

- Three clear steps: the title, a bright byline, then the body.
- Facts grouped by kind: the story on one side, the verdicts on the other, the file underneath.
- Verdicts set as large numbers that read across the room.
- Bold from scale and type only. No new colour; the approved tokens hold.
- One placard for every hero screen, so they stay alike.

## Non-goals and anti-goals

- No change to the panel placard beside a wall, the cues, focus or the shelves.
- No fact is dropped from Home.
- Failure even if it works: the placard moving as focus crosses works (it holds its lines today and
  must still); the backdrop's subject hidden under text more than it must be; a screen whose hero no
  longer matches the others.

## Approaches

- **A. A clearer order, same column.** Same stack with three steps, one-height marks, wider measure.
  Cheap and safe, but still a list of lines.
- **B. A wall label with the scores as big numbers (chosen by the owner).** A larger title and a
  bright byline, then two columns: the description on the left, the verdicts as large numbers on the
  right with one quiet line about the file below them.

## The design (B)

```
Taxi Driver
Martin Scorsese, 1976

Suffering from insomnia, disturbed    ★★★★½      8.2    89%    94
loner Travis Bickle takes a job as    yours      IMDb   RT     MC
a New York City cabbie, haunting
the streets nightly…                  1 h 54 · HD · English, 19 subtitles

[▶ Play] [Details]  ♡  ◉
```

- **Title.** Larger than today (about 8rem), still wrapping before it condenses (`FitTitle`); the
  subtitle of a split title stays as it is.
- **Byline.** The credit, one step up from today (heading size), in the label colour: artist first,
  as a wall label sets it ("Martin Scorsese, 1976"). An episode keeps its own order (show, episode,
  year).
- **Left column: the story.** The description in the reading face, two lines on Home (held), whole on
  Details. Details' further lines (genres) follow it, quietly.
- **Right column: the verdicts.** The owner's rating as stars, then up to three public scores. Each is
  a large figure in the display face, with its origin's mark small beneath it, drawn in the text
  colour; under the stars, the mark of the service the rating came from. Metacritic keeps its badge
  shape at the figure's size.
- **Right column, below: the file.** One quiet line: seasons or runtime, picture words, watched,
  offline, the format marks, and the languages ("English, 19 subtitles"). This replaces today's facts
  and tracks lines.
- **Width.** The placard widens to about 70rem on Home and Details. The backdrop's left gradient
  reaches further to keep the text on a dark enough wall.
- **Narrow heroes.** Where the placard has less room (Show, beside its seasons; a small window), a
  container query stacks the right column under the description. Today's order, in the new type.
- **Holding still.** On Home the lines keep their place across works: the right column keeps its
  height whenever a service is on (as `holdVerdicts` does today), and a work with no scores leaves the
  figures' row empty, not collapsed. With no service on, the verdicts row does not exist and the file
  line sits alone in the right column.
- **New words.** The label under the owner's stars ("yours") and the shortened languages ("19
  subtitles") go into the three catalogues (en-US, es-ES, es-MX).

## Left to the builder

- Exact sizes, gaps and the gradient's reach, as tokens in `tokens.css`, settled by looking at the
  running window at 1920×1080 and full screen.
- How `Placard` is split internally to serve both the stacked and the two-column form.
- Checking each hero screen (Home, Details, Show) in the dev window with keys and a screenshot, with
  and without services, with a long title, a show, an episode and a film the owner does not have.
