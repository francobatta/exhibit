// Renders the navigation stack. Every entry stays mounted, so Back finds its scroll position and
// focus exactly as they were; only the top entry is visible and live, the rest are inert.
// Push fades the new screen up; Back reverses it.

import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { Empty } from "@/components/states";
import { easeOut, length, seconds } from "@/lib/motion";
import { Home } from "@/screens/Home";
import { useNav, type Entry, type Route } from "./stack";

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
  const shift = length("--shift-screen");
  const transition = { duration: seconds("--dur-screen"), ease: easeOut() };
  return (
    <motion.div
      className="absolute inset-0 overflow-x-hidden overflow-y-auto pt-bar"
      inert={!top}
      aria-hidden={!top || undefined}
      style={{ pointerEvents: top ? "auto" : "none" }}
      initial={{ opacity: 0, y: shift }}
      animate={top ? { opacity: 1, y: 0 } : { opacity: 0, y: 0 }}
      exit={{ opacity: 0, y: shift }}
      transition={transition}
      data-entry={entry.id}
    >
      {screenFor(entry.route)}
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
