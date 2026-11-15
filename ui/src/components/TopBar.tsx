// The bar over every screen: the mark, the top-level sections, the ask field, settings, and a
// quiet line saying what the library is doing.

import type { JobStatus, RefreshPhase } from "@/lib/api";
import { plain } from "@/lib/api";
import { count } from "@/lib/format";
import { useJobStatus, useStatus } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useNav, type Route } from "@/nav/stack";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

const SECTIONS: Array<{ label: string; route: Route }> = [
  { label: "Home", route: { screen: "home" } },
  { label: "Movies", route: { screen: "wall", kind: "movie" } },
  { label: "Shows", route: { screen: "wall", kind: "show" } },
  { label: "Other", route: { screen: "wall", kind: "other" } },
];

const same = (a: Route, b: Route) =>
  a.screen === b.screen && (a.screen !== "wall" || (b.screen === "wall" && a.kind === b.kind));

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

export function TopBar() {
  const root = useNav((s) => s.entries[0]!.route);
  const replaceRoot = useNav((s) => s.replaceRoot);
  const push = useNav((s) => s.push);
  const { data: status } = useStatus();
  const job = jobLine(useJobStatus());
  const review = status?.needs_review ?? 0;
  const sections =
    review > 0 ? [...SECTIONS, { label: "Needs review", route: { screen: "review" } as Route }] : SECTIONS;

  return (
    <header className="fixed inset-x-0 top-0 z-30 flex h-bar items-center gap-12 px-edge">
      <span className="font-display text-lead font-title tracking-title text-label [font-stretch:var(--stretch-mark)]">
        Exhibit
      </span>

      <nav aria-label="Sections" className="-ml-4 flex items-center gap-1">
        {sections.map(({ label, route }) => {
          const here = same(root, route);
          return (
            <Button
              key={label}
              aria-current={here ? "page" : undefined}
              className={cn(here && "text-label")}
              onClick={() => replaceRoot(route)}
            >
              {label}
              {route.screen === "review" && (
                <span className="text-label-faint tabular-nums">{review}</span>
              )}
            </Button>
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
        <Input
          aria-label={status?.agent_key ? "Ask for something" : "Search the library"}
          placeholder={status?.agent_key ? "Ask for something" : "Search the library"}
          className="w-[22rem]"
        />
        <Button size="icon" aria-label="Settings" onClick={() => push({ screen: "settings" })}>
          <SettingsIcon />
        </Button>
      </div>
    </header>
  );
}
