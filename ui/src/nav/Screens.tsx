// Renders the navigation stack. Every entry stays mounted, so Back finds its scroll position and
// focus exactly as they were; only the top entry is visible and live, the rest are inert.
// Push fades the new screen up; Back reverses it.

import { AnimatePresence, motion, useIsPresent } from "motion/react";
import type { ReactNode } from "react";
import { Empty } from "@/components/states";
import { Lamp } from "@/components/Lamp";
import { Button } from "@/components/ui/button";
import { modalOpen } from "@/focus/bridge";
import { FocusGroup, FocusScope, useFocusItem } from "@/focus/react";
import { easeOut, length, seconds } from "@/lib/motion";
import { DEFAULT_VIEW } from "@/components/WallControls";
import { Home } from "@/screens/Home";
import { Wall } from "@/screens/Wall";
import { samePlace, useNav, type Entry, type Route } from "./stack";

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
  const at = SECTIONS.findIndex((s) => samePlace(s, root));
  const next = at === -1 ? 0 : (at + delta + SECTIONS.length) % SECTIONS.length;
  replaceRoot(SECTIONS[next]!);
}

/** Opens Settings, unless it is already on top or an overlay is open. */
export function openSettings(): void {
  const nav = useNav.getState();
  if (nav.current.screen === "settings" || modalOpen()) return;
  nav.push({ screen: "settings" });
}

function GoBack() {
  const back = useNav((s) => s.back);
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: "stub:back", onSelect: () => back() });
  return (
    <Button ref={ref} {...props} variant="line" className="relative focus:border-lamp">
      Go back
      {focused && <Lamp />}
    </Button>
  );
}

// Screens not built yet. A pushed one offers a way back, so focus has a place on it.
function Stub({ title, body, pushed }: { title: string; body: string; pushed: boolean }) {
  return (
    <>
      <Empty title={title} body={body} />
      {pushed && (
        <FocusGroup id="stub" orientation="horizontal" className="-mt-4 px-edge">
          <GoBack />
        </FocusGroup>
      )}
    </>
  );
}

function screenFor(entry: Entry, pushed: boolean): ReactNode {
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
    case "details":
      return <Stub title="Details" body="This screen is built in a later step." pushed={pushed} />;
    case "review":
      return <Stub title="Needs review" body="This screen is built in a later step." pushed={pushed} />;
    case "settings":
      return <Stub title="Settings" body="This screen is built in a later step." pushed={pushed} />;
    case "firstRun":
      return <Empty title="Welcome to Exhibit" body="This screen is built in a later step." />;
  }
}

function Layer({ entry, top, pushed }: { entry: Entry; top: boolean; pushed: boolean }) {
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
        {screenFor(entry, pushed)}
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
          <Layer key={e.id} entry={e} top={i === entries.length - 1} pushed={i > 0} />
        ))}
      </AnimatePresence>
    </main>
  );
}
