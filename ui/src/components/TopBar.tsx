// The bar over every screen: the mark (Back, on a screen stacked over another), the top-level
// sections, the ask field, settings, and a quiet line saying what the library is doing.

import { isTypeaheadKey } from "@ui-sdk/core";
import { motion } from "motion/react";
import type { KeyboardEvent, ReactNode } from "react";
import { liveGroups, type Dir } from "@/focus/bridge";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { t, useT } from "@/i18n";
import type { JobStatus, RefreshPhase } from "@/lib/api";
import { plain } from "@/lib/api";
import { useJobStatus, useStatus } from "@/lib/queries";
import { easeOut, seconds } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { openSettings } from "@/nav/Screens";
import { openAsk, samePlace, useNav, type Route } from "@/nav/stack";
import { BackButton } from "./BackButton";
import { Lamp } from "./Lamp";
import { Button } from "./ui/button";
import { viewsOf } from "@/nav/sections";

/**
 * The sections, by a stable id (their focus keys) and the key of their name. A section with views
 * (Movies: its Spotlight and its wall, nav/sections) opens on the first and is lit on any of them.
 */
const SECTIONS = [
  { id: "home", label: "sections.home", route: { screen: "home" } },
  { id: "movies", label: "sections.movies", route: { screen: "spotlight", kind: "movie" } },
  { id: "shows", label: "sections.shows", route: { screen: "wall", kind: "show" } },
  { id: "other", label: "sections.other", route: { screen: "wall", kind: "other" } },
] as const satisfies Array<{ id: string; label: string; route: Route }>;

const REVIEW = { id: "review", label: "sections.review", route: { screen: "review" } } as const satisfies {
  id: string;
  label: string;
  route: Route;
};

const PHASE = {
  scanning: "jobs.scanning",
  identifying: "jobs.identifying",
  probing: "jobs.probing",
  rescoring: "jobs.rescoring",
  translating: "jobs.translating",
  thumbnails: "jobs.thumbnails",
  reception: "jobs.reception",
  freshening: "jobs.freshening",
} as const satisfies Record<RefreshPhase, string>;

/** "Scanning, 214 files", "Looking up titles", or the last error; null when there is nothing to say. */
export function jobLine(job: JobStatus | null): string | null {
  if (job === null) return null;
  if (job.phase !== null) {
    return job.phase === "scanning" && job.done > 0 ? t("jobs.scanningFiles", { count: job.done }) : t(PHASE[job.phase]);
  }
  return job.last_error ? plain(job.last_error) : null;
}

function SettingsIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden className="size-(--bar-icon)" fill="none">
      <path
        d="M4 7h10M18 7h2M4 17h4M12 17h8"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
      />
      <circle cx="16" cy="7" r="2.2" stroke="currentColor" strokeWidth="1.6" />
      <circle cx="10" cy="17" r="2.2" stroke="currentColor" strokeWidth="1.6" />
    </svg>
  );
}

function BarButton({
  focusKey,
  onSelect,
  className,
  children,
  ...rest
}: {
  focusKey: string;
  onSelect: () => void;
  className?: string;
  children: ReactNode;
  "aria-label"?: string;
  "aria-current"?: "page";
  size?: "md" | "icon";
}) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey, onSelect });
  return (
    <Button
      ref={ref}
      {...props}
      {...rest}
      className={cn("relative h-(--bar-control) focus-visible:bg-transparent focus:text-label", className)}
    >
      {children}
      {focused && <Lamp />}
    </Button>
  );
}

/** A real key typing a letter or digit (`/`, `[` and `]` stay shortcuts; the pad's keys are synthetic and stay its buttons). */
const startsSearch = (e: globalThis.KeyboardEvent) => e.isTrusted && isTypeaheadKey(e) && /^[\p{L}\p{N}]$/u.test(e.key);

/** Looks like a field and opens Ask, as Y, `/` and Ctrl+K do; a letter typed on it starts the search. */
function AskField({ label }: { label: string }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: "ask", onSelect: () => openAsk(), hint: t("hints.search") });
  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (startsSearch(e.nativeEvent)) {
      e.preventDefault();
      openAsk(e.key);
      return;
    }
    props.onKeyDown(e);
  };
  return (
    <button
      ref={ref}
      {...props}
      onKeyDown={onKeyDown}
      type="button"
      aria-haspopup="dialog"
      className={cn(
        "flex h-(--bar-control) w-(--bar-field) items-center rounded-control border border-line bg-wall/40 px-5 outline-none",
        "font-display text-(length:--size-bar-field) text-label-faint transition-[border-color,background-color]",
        "hover:border-label-faint",
        focused && "border-lamp/60 bg-wall/70 text-label-dim",
      )}
    >
      {label}
    </button>
  );
}

/**
 * Down from the bar with nothing within reach below (a screen set to the left, under the ask field
 * on the right) enters the screen at its first group that takes focus, as if from straight above.
 */
function intoScreen(dir: Dir): void {
  const from = document.activeElement;
  if (dir[1] !== 1 || !(from instanceof HTMLElement)) return;
  const at = from.getBoundingClientRect();
  for (const g of liveGroups()) if (g.id !== "topbar" && g.enter(at, dir)) return;
}

export function TopBar() {
  const t = useT();
  const root = useNav((s) => s.entries[0]!.route);
  const stacked = useNav((s) => s.entries.length > 1);
  const { replaceRoot, toRoot, back } = useNav.getState();
  const fade = { initial: { opacity: 0 }, animate: { opacity: 1 }, transition: { duration: seconds("--dur-screen"), ease: easeOut() } };
  const { data: status } = useStatus();
  const job = jobLine(useJobStatus());
  const review = status?.needs_review ?? 0;
  const sections: Array<{ id: string; label: (typeof SECTIONS)[number]["label"] | typeof REVIEW.label; route: Route }> =
    review > 0 ? [...SECTIONS, REVIEW] : [...SECTIONS];

  return (
    <header className="fixed inset-x-0 top-0 z-30 h-(--bar-height) px-edge">
      <FocusGroup id="topbar" orientation="horizontal" onEdge={intoScreen} className="flex h-full items-center gap-12">
        {/* Back takes the mark's place, so the sections never move. */}
        <div className="flex h-full w-(--bar-lead) shrink-0 items-center">
          {stacked ? (
            <motion.div key="back" {...fade} className="-ml-4">
              <BackButton focusKey="back" onBack={back} />
            </motion.div>
          ) : (
            <motion.span
              key="mark"
              {...fade}
              className="font-display text-(length:--size-bar-mark) font-title tracking-title text-label [font-stretch:var(--stretch-mark)]"
            >
              Exhibit
            </motion.span>
          )}
        </div>

        <nav aria-label={t("sections.label")} className="-ml-4 flex items-center gap-1">
          {sections.map(({ id, label, route }) => {
            const here = samePlace(root, route) || (viewsOf(route)?.some((v) => samePlace(root, v.route)) ?? false);
            return (
              <BarButton
                key={id}
                focusKey={`section:${id}`}
                aria-current={here ? "page" : undefined}
                className={cn("px-5 text-(length:--size-bar)", here && "text-label")}
                // The section already under the stack returns as it was left.
                onSelect={() => (here ? toRoot() : replaceRoot(route))}
              >
                {t(label)}
                {route.screen === "review" && (
                  <span className="text-label-faint tabular-nums">{review}</span>
                )}
              </BarButton>
            );
          })}
        </nav>

        <div className="ml-auto flex items-center gap-(--bar-gap)">
          {job !== null && (
            <p
              role="status"
              className="max-w-[24rem] truncate font-display text-caption text-label-faint"
            >
              {job}
            </p>
          )}
          <AskField label={status?.agent_key ? t("topBar.ask") : t("topBar.search")} />
          <BarButton
            focusKey="settings"
            size="icon"
            className="size-(--bar-control)"
            aria-label={t("topBar.settings")}
            onSelect={openSettings}
          >
            <SettingsIcon />
          </BarButton>
        </div>
      </FocusGroup>
    </header>
  );
}
