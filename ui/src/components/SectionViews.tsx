// The row naming a section's views (Movies: Spotlight, All movies), under the bar in the right-hand
// column, on the line of a wall's controls (which stop short of it), where no title reaches. The view showing is lit; the others are
// quiet. The row is drawn as a thin outline, and as the section opens a glint runs once round it,
// so the owner sees there is another way to look. One button switches from anywhere on the page (the pad's View, the V key), and its glyph
// leads the row; choosing a view by focus works too.

import { registerShortcut } from "@ui-sdk/core/shortcuts";
import { useEffect } from "react";
import { FocusGroup, useFocusItem, useInputMode } from "@/focus/react";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { nextView, type SectionView } from "@/nav/sections";
import { samePlace, useNav, type Route } from "@/nav/stack";
import { Glyph } from "./Hints";
import { Lamp } from "./Lamp";

/** Switches the section on top of the stack to its next view; nothing over a stacked screen. */
export function switchView(): void {
  const { entries, replaceRoot } = useNav.getState();
  if (entries.length !== 1) return;
  const next = nextView(entries[0]!.route);
  if (next !== null) replaceRoot(next.route);
}

function ViewButton({ view, on }: { view: SectionView; on: boolean }) {
  const t = useT();
  const { replaceRoot, toRoot } = useNav.getState();
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: `view:${view.id}`,
    onSelect: () => (on ? toRoot() : replaceRoot(view.route)),
  });
  return (
    <button
      ref={ref}
      {...props}
      type="button"
      aria-current={on ? "page" : undefined}
      className={cn(
        "relative flex h-full flex-1 items-center justify-center font-display text-(length:--size-views) font-ui outline-none transition-colors",
        on ? "text-label" : "text-label-dim hover:text-label",
        focused && "text-label",
      )}
    >
      {t(view.label)}
      {/* The view showing stands on a rule, as a section in the bar stands on its lamp. */}
      {on && (
        <span
          aria-hidden
          className="absolute inset-x-0 bottom-2 h-(--cue-rule-weight) rounded-full bg-label"
        />
      )}
      {focused && <Lamp />}
    </button>
  );
}

export function SectionViews({
  views,
  route,
  live,
}: {
  views: readonly SectionView[];
  route: Route;
  live: boolean;
}) {
  const t = useT();
  const mode = useInputMode();
  useEffect(() => {
    if (!live) return;
    return registerShortcut("v", switchView, { description: "Switch view" });
  }, [live]);
  return (
    <div
      inert={!live}
      aria-hidden={!live || undefined}
      className={cn(
        "absolute top-(--bar-height) right-0 z-10 transition-opacity duration-(--dur-screen) ease-out",
        !live && "opacity-0",
      )}
    >
      <FocusGroup
        id="section-views"
        orientation="horizontal"
        aria-label={t("sections.label")}
        className="flex h-(--wall-controls) items-center px-edge"
      >
        <div className="relative flex h-(--views-row) w-(--rail) items-stretch gap-(--views-gap) px-(--views-pad)">
          <span aria-hidden className="views-ring" />
          {mode !== "pointer" && (
            <span aria-hidden className="flex shrink-0 items-center gap-1 self-center">
              <Glyph action="views" mode={mode} />
            </span>
          )}
          {views.map((v) => (
            <ViewButton key={v.id} view={v} on={samePlace(route, v.route)} />
          ))}
        </div>
      </FocusGroup>
    </div>
  );
}
