// Renders the navigation stack. Every entry stays mounted, so Back finds its scroll position and
// focus exactly as they were; only the top entry is visible and live, the rest are inert.
// Push fades the new screen up; Back reverses it.

import { AnimatePresence, motion, useIsPresent } from "motion/react";
import type { ReactNode } from "react";
import { modalOpen } from "@/focus/bridge";
import { FocusScope } from "@/focus/react";
import { easeOut, length, seconds } from "@/lib/motion";
import { Details } from "@/screens/Details";
import { FirstRun } from "@/screens/FirstRun";
import { Home } from "@/screens/Home";
import { Review } from "@/screens/Review";
import { Settings } from "@/screens/Settings";
import { Show } from "@/screens/Show";
import { Wall } from "@/screens/Wall";
import { DEFAULT_VIEW, onFirstRun, samePlace, useNav, type Entry, type Route, type SettingsPart } from "./stack";

/** The top-level sections `[` and `]` (LB and RB) cycle through. */
const SECTIONS: Route[] = [
  { screen: "home" },
  { screen: "wall", kind: "movie" },
  { screen: "wall", kind: "show" },
  { screen: "wall", kind: "other" },
];

/** Replaces the root with the previous or next section, wrapping; from elsewhere it starts at Home. */
export function cycleSection(delta: -1 | 1): void {
  const { entries, replaceRoot } = useNav.getState();
  const root = entries[0]!.route;
  if (onFirstRun()) return;
  const at = SECTIONS.findIndex((s) => samePlace(s, root));
  const next = at === -1 ? 0 : (at + delta + SECTIONS.length) % SECTIONS.length;
  replaceRoot(SECTIONS[next]!);
}

/** Opens Settings (on `part`, if given), unless it is already on top, an overlay is open, or first run is showing. */
export function openSettings(part?: SettingsPart): void {
  const nav = useNav.getState();
  if (nav.current.screen === "settings" || modalOpen() || onFirstRun()) return;
  nav.push(part === undefined ? { screen: "settings" } : { screen: "settings", part });
}

function screenFor(entry: Entry): ReactNode {
  const route = entry.route;
  switch (route.screen) {
    case "home":
      return <Home />;
    case "wall":
      return (
        <Wall
          kind={route.kind}
          view={route.view ?? DEFAULT_VIEW}
          onView={(view) => useNav.getState().setRoute(entry.id, { ...route, view })}
        />
      );
    case "show":
      return <Show id={route.id} />;
    case "details":
      return <Details id={route.id} />;
    case "review":
      return <Review />;
    case "settings":
      return <Settings part={route.part} />;
    case "firstRun":
      return <FirstRun />;
  }
}

function Layer({ entry, top }: { entry: Entry; top: boolean }) {
  const setFocusKey = useNav((s) => s.setFocusKey);
  // A layer leaving after Back keeps its last props while it animates out; it must not hold focus.
  const present = useIsPresent();
  const live = top && present;
  const shift = length("--shift-screen");
  const transition = { duration: seconds("--dur-screen"), ease: easeOut() };
  return (
    <motion.div
      className="absolute inset-0 scroll-pt-bar overflow-x-hidden overflow-y-auto pt-bar"
      inert={!live}
      aria-hidden={!live || undefined}
      style={{ pointerEvents: live ? "auto" : "none" }}
      initial={{ opacity: 0, y: shift }}
      animate={top ? { opacity: 1, y: 0 } : { opacity: 0, y: 0 }}
      exit={{ opacity: 0, y: shift }}
      transition={transition}
      data-entry={entry.id}
    >
      <FocusScope
        id={`entry-${entry.id}`}
        restoreKey={entry.focusKey}
        active={live}
        onFocusKey={live ? setFocusKey : undefined}
      >
        {screenFor(entry)}
      </FocusScope>
    </motion.div>
  );
}

export function Screens() {
  const entries = useNav((s) => s.entries);
  return (
    <main className="relative h-full">
      <AnimatePresence initial={false}>
        {entries.map((e, i) => (
          <Layer key={e.id} entry={e} top={i === entries.length - 1} />
        ))}
      </AnimatePresence>
    </main>
  );
}
