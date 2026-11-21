// The row naming a section's views (Movies: Spotlight, All movies), under the bar at its right
// edge, below the search field, where no title reaches. The view showing is lit; the others are
// quiet. Choosing one replaces the page; choosing the one showing returns it to where it was left.

import { FocusGroup, useFocusItem } from "@/focus/react";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import type { SectionView } from "@/nav/sections";
import { samePlace, useNav, type Route } from "@/nav/stack";
import { Lamp } from "./Lamp";

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
        on ? "text-label" : "text-label-faint hover:text-label-dim",
        focused && "text-label",
      )}
    >
      {t(view.label)}
      {/* The view showing stands on a rule, as a section in the bar stands on its lamp. */}
      {on && <span aria-hidden className="absolute inset-x-0 bottom-0 h-(--cue-rule-weight) rounded-full bg-label" />}
      {focused && <Lamp />}
    </button>
  );
}

export function SectionViews({ views, route }: { views: readonly SectionView[]; route: Route }) {
  const t = useT();
  return (
    <FocusGroup
      id="section-views"
      orientation="horizontal"
      aria-label={t("sections.label")}
      className="absolute top-(--bar-height) right-0 z-10 flex h-(--views-row) items-stretch gap-(--views-gap) px-edge"
    >
      {views.map((v) => (
        <ViewButton key={v.id} view={v} on={samePlace(route, v.route)} />
      ))}
    </FocusGroup>
  );
}
