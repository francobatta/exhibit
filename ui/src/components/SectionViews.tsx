// The row naming a section's views (Movies: Spotlight, All movies), under the bar at its right
// edge, below the search field, where no title reaches. The view showing is lit; the others are
// quiet, and a glint crosses them as the section opens, so the owner sees there is another way to
// look. One button switches from anywhere on the page (the pad's View, the V key), and its glyph
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
        "relative flex h-full items-center font-display text-(length:--size-views) font-ui outline-none transition-colors",
        on ? "text-label" : "text-label-dim hover:text-label",
        focused && "text-label",
      )}
    >
      {t(view.label)}
      {/* The view showing stands on a rule, as a section in the bar stands on its lamp. */}
      {on && <span aria-hidden className="absolute inset-x-0 bottom-0 h-(--cue-rule-weight) rounded-full bg-label" />}
      {!on && <span aria-hidden className="views-glint" />}
      {focused && <Lamp />}
    </button>
  );
}

export function SectionViews({ views, route, live }: { views: readonly SectionView[]; route: Route; live: boolean }) {
  const t = useT();
  const mode = useInputMode();
  useEffect(() => {
    if (!live) return;
    return registerShortcut("v", switchView, { description: "Switch view" });
  }, [live]);
  return (
    <FocusGroup
      id="section-views"
      orientation="horizontal"
      aria-label={t("sections.label")}
      className="absolute top-(--bar-height) right-0 z-10 flex h-(--views-row) items-stretch gap-(--views-gap) px-edge"
    >
      {mode !== "pointer" && (
        <span aria-hidden className="flex items-center gap-1 self-center">
          <Glyph action="views" mode={mode} />
        </span>
      )}
      {views.map((v) => (
        <ViewButton key={v.id} view={v} on={samePlace(route, v.route)} />
      ))}
    </FocusGroup>
  );
}
