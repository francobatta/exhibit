// The bar over every screen: the mark, the top-level sections, the ask field, settings, and a
// quiet line saying what the library is doing.

import { isTypeaheadKey } from "@ui-sdk/core";
import type { KeyboardEvent, ReactNode } from "react";
import { FocusGroup, useFocusItem } from "@/focus/react";
import type { JobStatus, RefreshPhase } from "@/lib/api";
import { plain } from "@/lib/api";
import { count } from "@/lib/format";
import { useJobStatus, useStatus } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { openSettings } from "@/nav/Screens";
import { openAsk, samePlace, useNav, type Route } from "@/nav/stack";
import { Lamp } from "./Lamp";
import { Button } from "./ui/button";

const SECTIONS: Array<{ label: string; route: Route }> = [
  { label: "Home", route: { screen: "home" } },
  { label: "Movies", route: { screen: "wall", kind: "movie" } },
  { label: "Shows", route: { screen: "wall", kind: "show" } },
  { label: "Other", route: { screen: "wall", kind: "other" } },
];

const PHASE: Record<RefreshPhase, string> = {
  scanning: "Scanning",
  identifying: "Looking up titles",
  probing: "Reading file details",
  rescoring: "Checking matches",
  thumbnails: "Making thumbnails",
};

/** "Scanning, 214 files", "Looking up titles", or the last error; null when there is nothing to say. */
export function jobLine(job: JobStatus | null): string | null {
  if (job === null) return null;
  if (job.phase !== null) {
    return job.phase === "scanning" && job.done > 0
      ? `${PHASE.scanning}, ${count(job.done, "file", "files")}`
      : PHASE[job.phase];
  }
  return job.last_error ? plain(job.last_error) : null;
}

function SettingsIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden className="size-[1.375rem]" fill="none">
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
      className={cn("relative focus-visible:bg-transparent focus:text-label", className)}
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
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: "ask", onSelect: () => openAsk() });
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
        "flex h-11 w-[22rem] items-center rounded-control border border-line bg-wall/40 px-4 outline-none",
        "font-display text-body text-label-faint transition-[border-color,background-color]",
        "hover:border-label-faint",
        focused && "border-lamp/60 bg-wall/70 text-label-dim",
      )}
    >
      {label}
    </button>
  );
}

export function TopBar() {
  const root = useNav((s) => s.entries[0]!.route);
  const replaceRoot = useNav((s) => s.replaceRoot);
  const { data: status } = useStatus();
  const job = jobLine(useJobStatus());
  const review = status?.needs_review ?? 0;
  const sections =
    review > 0 ? [...SECTIONS, { label: "Needs review", route: { screen: "review" } as Route }] : SECTIONS;

  return (
    <header className="fixed inset-x-0 top-0 z-30 h-bar px-edge">
      <FocusGroup id="topbar" orientation="horizontal" className="flex h-full items-center gap-12">
        <span className="font-display text-lead font-title tracking-title text-label [font-stretch:var(--stretch-mark)]">
          Exhibit
        </span>

        <nav aria-label="Sections" className="-ml-4 flex items-center gap-1">
          {sections.map(({ label, route }) => {
            const here = samePlace(root, route);
            return (
              <BarButton
                key={label}
                focusKey={`section:${label}`}
                aria-current={here ? "page" : undefined}
                className={cn(here && "text-label")}
                onSelect={() => replaceRoot(route)}
              >
                {label}
                {route.screen === "review" && (
                  <span className="text-label-faint tabular-nums">{review}</span>
                )}
              </BarButton>
            );
          })}
        </nav>

        <div className="ml-auto flex items-center gap-6">
          {job !== null && (
            <p
              role="status"
              className="max-w-[24rem] truncate font-display text-caption text-label-faint"
            >
              {job}
            </p>
          )}
          <AskField label={status?.agent_key ? "Ask for something" : "Search the library"} />
          <BarButton
            focusKey="settings"
            size="icon"
            aria-label="Settings"
            onSelect={openSettings}
          >
            <SettingsIcon />
          </BarButton>
        </div>
      </FocusGroup>
    </header>
  );
}
