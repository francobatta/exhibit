# Intro: the E opens onto Home

A short design (a small change). The owner's decisions are marked **(owner)**; everything else is
the designer's call and can be corrected.

## Intent

Each time Exhibit opens, the E of the logo grows from small to large and gives way to Home: a
moment of occasion like Steam Big Picture or Netflix, quick and dazzling, never a wait. On by
default, with one switch in Settings to turn it off. **(owner)**

## Goals, non-goals, anti-goals

- **Goal:** about 1.5 s from first frame to a usable Home when the library is ready. Any button
  skips it.
- **Goal:** the intro covers the time the library takes to open, so it adds little or nothing to
  startup. Today that time shows "Opening the library".
- **Non-goal:** sound, artwork from the library, a different intro on each launch. The E alone,
  silent. **(owner: the E alone)**
- **Anti-goal:** a stutter in the first frames, a blank or white frame, a flash between the intro
  and Home, a key press that skips the intro and also acts on Home, or a second setting.

## Approaches

1. **Motion and an SVG E in the main window (picked by the owner).** No new library. The E is a
   vector, so there is no wait for the font. It animates only `transform`, `opacity` and `filter`.
   Because it is in the same window as Home, the E can literally open onto Home.
2. **A Tauri splash window.** Tauri's own startup recipe. The intro lives in a second window, so
   it cannot open onto Home, and swapping full-screen windows can flash on a TV. Rejected.
3. **A GPU canvas (vgpu or three.js).** Real 3D light and bloom, but shaders compile on a cold
   start, which risks a stutter in the first frames, and it adds a dependency for 1.4 s. Rejected.

## The sequence

The intro is one full-screen layer above the whole app. It is a wall-coloured surface with an
E-shaped hole, and a solid E drawn over that hole. Home renders underneath from the first frame,
as it does today.

The owner asked, after seeing a first version, that the E not come in upright and straight but
spawn elegantly, creep slowly, then zoom quickly and finish. **(owner)**

1. **Spawn (about 0.65 s).** The window opens on the wall colour, as it already does natively.
   The lit E (`--label`, with a lamp bloom behind it) swings in on a spring from about 40% of its
   size, turned away in depth (mostly about its upright axis), out of focus and in the lamp's
   shade. It comes to rest upright, sharp and lit, warmer at the top where the lamp falls. A soft
   shadow on the wall follows its turn and drops below it as it comes away from the wall. The
   lamp's pool (`--lamp`, quiet, from above) rises behind it and lights the hole's cover like the
   rest of the wall, and one soft glint sweeps across the letter. No colour: the light is the lamp's warm white. Only the E turns. The wall with the
   hole scales flat, and a wall-coloured E fills the hole until the reveal.
2. **Creep.** The whole surface pushes slowly and steadily toward the viewer. It creeps at least
   `--dur-intro-creep-min`, and for as long as the library still needs.
3. **Zoom (about 0.4 s).** As soon as Home is ready and the minimum creep has run, the lit E fades
   out, and Home shows through the E-shaped hole. The surface then zooms on a sharp ease-in about a
   point inside the letter's middle arm, until the hole covers the window. Then the layer is
   removed. Rasterisation blur at high scale reads as speed and is accepted.

- **Skip.** Any key, pad button or click jumps straight to step 3 (or to its end, if step 3 is
  already running). The press that skips is used up by the intro: it never reaches Home's focus
  system. Skipping while the library is still opening completes the spawn, and the creep goes on until the library is ready.
- **When the library fails to open.** The intro opens onto the existing error state, the same
  way.
- **First run.** The intro plays and opens onto the first-run screen.
- **Reduced motion.** No growth and no fly-through: the E shows for a moment, then crossfades to
  Home.
- **Focus.** Home's landing focus is set as it is today, underneath. Focus is never lost, and the
  lamp is visible as the layer opens.

## Where the setting lives

- **Core.** `Library::intro() -> bool` and `Library::set_intro(bool)` over the existing `settings`
  table (key `intro`; absent means on, `"off"` means off). `LibraryStatus` gains `intro: bool`.
- **CLI.** A way to set it, in whatever shape the existing commands use, since the spec says no
  feature is UI-only.
- **App.** A `set_intro` command. At launch, `setup` already opens the library before it builds
  the window. It reads `intro()` there and passes it to the page with the window builder's
  `initialization_script`, so the first frame knows whether to play, before any command returns.
- **Settings.** The Display part gains an **Intro** switch next to Full screen. It uses the same
  owned UI SDK `Switch` and the same focus wiring, and it reads from status. The change applies
  from the next launch.

**Dropped from the approach as discussed: hiding the window until the first intro frame.** The
window already paints the wall colour natively before the page loads, so the intro starts from
the same dark with nothing to hide. A hidden window would also never appear if the page failed
to load.

## Left to the builder

- **The E.** Build it from rectangles matching the app icon's proportions
  (`crates/exhibit-app/icons/icon.png`). Its size and timings are tokens in `tokens.css`; the
  springs go in `lib/motion.ts`.
- **"Home ready."** The boot query has settled, and React has committed the screen beneath.
- **Swallowing the skip press.** Do it through the focus system's input layer (`ui/src/focus/input`),
  so keys, the pad and the pointer behave the same.
- **Tests.**
  - Core: the default and the toggle.
  - UI: no layer when the flag is off; a skip press is used up; the Settings switch calls
    `set_intro`.
- **Checking the result.** In the running window over CDP: a reload replays the intro, because the
  init script runs on each load. Take screenshots mid-intro, and run `?fps` during it.
