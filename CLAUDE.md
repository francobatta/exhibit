# Exhibit

A gallery for a home-theatre PC. It finds a local media collection, identifies it, shows it on a TV, and hands playback to mpv.

Read `README.md` for the layout, prerequisites and checks. For anything in `ui/`, also read `ui/AGENTS.md`. The spec is `docs/superpowers/specs/2026-11-14-exhibit-design.md`.

## What Exhibit is for
The interface is the product. Everything else serves it: the core, the matching, the speed.

- **Expressive.** A gallery at night. The walls are dark and the artwork carries the colour. The background takes its tint from the focused title. Light falls on whatever has focus. Each title has a placard, like a museum wall label. Every screen should feel like a place you would want to sit in front of.
- **Built for 2026.** Use what a current WebView2 can do: view transitions, container queries, `oklch`, `@property`, scroll-driven animation, variable-font axes. Don't fall back on defaults from ten years ago, and don't build anything that looks like an admin template.
- **Creative.** Before you build a screen, decide what makes it memorable. Then commit to that idea. For new screens or visual changes, use the `frontend-design` skill as well as `ui-sdk`.
- **Bold and clear.** Bold comes from scale, type, artwork and motion, not from extra colour. The approved tokens have no accent colour, so don't add one. Be clear at three metres: big type, one focal point per screen, and plain words. Errors say what happened and what to do.
- **Easy to use.** A direction pad and four buttons reach everything. Gamepad, keyboard and mouse drive one focus system and behave the same on every screen. Use strong defaults and one screen of settings. Nothing should need explaining.

The anti-goals in spec section 4 count as failures even when the code works. The main ones are a multiplying number of settings, screens that behave differently from each other, and a confident wrong match.

## Fluent UI
A screen that looks right but stutters is not finished. Fluency is a requirement, not polish.

- **Seamless.**
  - Focus is never lost and never jumps. Going back returns focus to where it was.
  - Moving between screens is one continuous movement. There are no blank frames, layout shifts or flashes of unstyled content.
  - Every screen has designed loading, empty, offline-drive and error states.
- **Fresh.**
  - Motion answers the owner's input: moving focus, opening and returning. Otherwise the screen stays still.
  - Motion is quick and physical. Use Motion springs and `layout` and shared-element transitions, not long eased fades.
  - Respect `prefers-reduced-motion`.
- **Fast.**
  - Input gets a visible response in the same frame.
  - Grids and shelves hold 60 fps with 5,000 items. They are virtualised, and artwork loads at the size it is shown.
  - Animate only `transform`, `opacity` and `filter`.
  - Keep data cached and current with TanStack Query, so a screen never waits for something it has already loaded.
  - The `?fps` check is how you measure this.

Build tastefully. Every element earns its place, and one strong idea beats five decorations. If you are unsure, show the owner before you apply a visual change widely.

## Building UI with UI SDK
Vercel UI SDK (`@ui-sdk/react` over `@ui-sdk/core`, in beta) is the behaviour layer. It handles keyboard, focus, selection, dismissal, popups and ARIA. It renders no styles; the look is ours.

- Follow the `ui-sdk` skill. Read `ui/node_modules/@ui-sdk/react/docs/llms.txt` first, then only the component pages you need. Never write its API from memory, because the beta changes between releases.
- Import each component from its own entry (`@ui-sdk/react/dialog`). Never import `@ui-sdk/core/internal/*`.
- Don't hand-roll what the SDK ships. A `role="listbox|menu|dialog|tablist|switch|slider|..."` on a plain `div` is almost always a bug.
- If the SDK doesn't ship a component, build it from `@ui-sdk/core` behaviours. Search `docs/core.md` by heading; it is over 300 KB, so don't read it whole.
- Style from state: use `data-slot`, `data-open`, `data-pressed` and the component's CSS variables. Never use inline styles for state.
- Owned primitives (shadcn conventions) live in `ui/src/components/ui/`. Wrap SDK parts there once, and use the wrappers everywhere else.
- SDK focus must work with Exhibit's focus system in `ui/src/focus/`. Check every new control with a gamepad d-pad, the keyboard and the mouse.

## Running the app
Use the `/dev` skill (`.claude/skills/dev/SKILL.md`).

- Start: `scripts/dev.ps1 -Cdp`, in the background.
- Drive and screenshot: `bun scripts/cdp.ts keys|shot|eval`.
- Stop: `scripts/dev.ps1 -Stop`.

When you change the UI, look at the result in the running window before you say it works. A screenshot shows the look; also drive the change with keys to check its motion and focus.

## Where things stand
`.superpowers/README.md` is git-ignored. It holds:

- what has been built;
- the owner's decisions still in force;
- the next steps and known small debts;
- the facts about the dev library.

The spec and stage plans are in `docs/superpowers/`.

## Rules
- Bun only. Node.js is not installed: never `node`, `npm` or `npx`.
- Never write, rename or delete anything in a media folder. `D:\` is the owner's media drive.
- Never print, remove or overwrite the TMDB key in the `exhibit-dev` keyring.
- Never point tests at the owner's real data dir or credential store.
- The UI never touches the database, the file system or the network itself. Everything goes through Tauri commands over `exhibit_core::Library`.
- Code in `ui/src` is written for WebView2 only. Stub happy-dom gaps in `ui/test/setup.ts`, never in `src`.
- Tokens live only in `ui/src/styles/tokens.css`. Everything is sized in `rem`.
- Database code is plain `rusqlite` with hand-written SQL.
- No mutation testing. Tests support the work; they are not the deliverable.
