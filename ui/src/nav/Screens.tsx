// Renders the navigation stack. Every entry stays mounted, so Back finds its scroll position and
// focus exactly as they were; only the top entry is visible and live, the rest are inert.
// Push fades the new screen up; Back reverses it.

import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { Empty } from "@/components/states";
import { FocusScope } from "@/focus/react";
import { easeOut, length, seconds } from "@/lib/motion";
import { Home } from "@/screens/Home";
import { useNav, type Entry, type Route } from "./stack";

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
  const at = SECTIONS.findIndex((s) => JSON.stringify(s) === JSON.stringify(root));
  const next = at === -1 ? 0 : (at + delta + SECTIONS.length) % SECTIONS.length;
  replaceRoot(SECTIONS[next]!);
}

function screenFor(route: Route): ReactNode {
  switch (route.screen) {
    case "home":
      return <Home />;
    case "wall": {
      const name = { movie: "Movies", show: "Shows", other: "Other" }[route.kind];
      return <Empty title={name} body="This wall is hung in a later step." />;
    }
    case "show":
    case "details":
      return <Empty title="Details" body="This screen is built in a later step." />;
    case "review":
      return <Empty title="Needs review" body="This screen is built in a later step." />;
    case "settings":
      return <Empty title="Settings" body="This screen is built in a later step." />;
    case "firstRun":
      return <Empty title="Welcome to Exhibit" body="This screen is built in a later step." />;
  }
}

function Layer({ entry, top }: { entry: Entry; top: boolean }) {
  const setFocusKey = useNav((s) => s.setFocusKey);
  const shift = length("--shift-screen");
  const transition = { duration: seconds("--dur-screen"), ease: easeOut() };
  return (
    <motion.div
      className="absolute inset-0 scroll-pt-bar overflow-x-hidden overflow-y-auto pt-bar"
      inert={!top}
      aria-hidden={!top || undefined}
      style={{ pointerEvents: top ? "auto" : "none" }}
      initial={{ opacity: 0, y: shift }}
      animate={top ? { opacity: 1, y: 0 } : { opacity: 0, y: 0 }}
      exit={{ opacity: 0, y: shift }}
      transition={transition}
      data-entry={entry.id}
    >
      <FocusScope
        id={`entry-${entry.id}`}
        restoreKey={entry.focusKey}
        active={top}
        onFocusKey={top ? setFocusKey : undefined}
      >
        {screenFor(entry.route)}
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
