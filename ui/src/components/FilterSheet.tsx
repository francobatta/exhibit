// Everything a wall can be narrowed by, in a sheet at its right. Each choice applies at once, so the
// wall behind it rehangs and the count above says how many works are left; B or Done closes it. A
// section shows only where the wall has something to choose (no runtimes on Shows, no genres in
// Other). Genres take three states: wanted, left out, neither. "Your activity" is there only while a
// service brings the owner's activity in, each choice with how many works it leaves.

import type { ReactNode } from "react";
import { Lamp } from "@/components/Lamp";
import { RangeField } from "@/components/RangeField";
import { Button } from "@/components/ui/button";
import { DialogTitle } from "@/components/ui/dialog";
import { Toggle } from "@/components/ui/toggle";
import { FocusGroup, FocusScope, useFocusItem } from "@/focus/react";
import { useT } from "@/i18n";
import { hasActivity } from "@/lib/actions";
import type { Count, Facets, Filters, ItemKind } from "@/lib/api";
import {
  ACTIVITY,
  PICTURES,
  RESOLUTIONS,
  SEEN,
  SOUNDS,
  activityName,
  compact,
  cycleGenre,
  formatName,
  genreState,
  langName,
  minutes,
  seenName,
  span,
  toggled,
  type GenreState,
} from "@/lib/filters";
import { number } from "@/lib/format";
import { useFacets, useServices } from "@/lib/queries";
import { cn } from "@/lib/utils";

const sheet = cn(
  "fixed inset-y-0 right-0 z-50 flex w-(--filter-sheet) max-w-full flex-col border-l border-line bg-wall-raised text-label outline-none",
  "transition-[opacity,translate] duration-(--dur-screen) ease-out",
  "data-starting-style:translate-x-12 data-starting-style:opacity-0",
  "data-ending-style:translate-x-12 data-ending-style:opacity-0",
  "motion-reduce:transition-opacity",
);

// Light enough that the wall rehanging behind the sheet stays in view.
const veil = cn(
  "fixed inset-0 z-40 bg-wall/45 transition-opacity duration-(--dur-screen)",
  "data-starting-style:opacity-0 data-ending-style:opacity-0",
);

/** A choice that is on or off; with `excludes`, A turns it on, then to left out, then off. */
function Chip({
  focusKey,
  state,
  excludes = false,
  count,
  onPress,
  children,
}: {
  focusKey: string;
  state: GenreState;
  excludes?: boolean;
  /** How many works choosing it leaves, after its name. */
  count?: number;
  onPress: () => void;
  children: ReactNode;
}) {
  const t = useT();
  const hint = { off: t("hints.include"), in: excludes ? t("hints.exclude") : t("hints.remove"), out: t("hints.remove") }[state];
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey, hint });
  return (
    <Toggle
      ref={ref}
      {...props}
      variant="line"
      pressed={state === "in"}
      onPressedChange={onPress}
      aria-description={state === "out" ? t("filters.left") : undefined}
      data-out={state === "out" ? "" : undefined}
      className={cn(
        "relative focus:text-label",
        // Chosen reads at a distance as a filled chip; focus is the lamp, on either.
        "data-pressed:border-label data-pressed:bg-label data-pressed:text-wall",
        "data-pressed:focus-visible:border-lamp data-pressed:focus-visible:bg-lamp",
        "data-out:border-label-dim data-out:text-label-dim data-out:line-through",
      )}
    >
      {state === "out" && <span aria-hidden>−</span>}
      {children}
      {count !== undefined && (
        <span data-slot="count" className="tabular-nums opacity-60">
          {number(count)}
        </span>
      )}
      {focused && <Lamp />}
    </Toggle>
  );
}

function Section({ title, value, children }: { title: string; value?: string | null; children: ReactNode }) {
  return (
    <section aria-label={title} className="flex flex-col gap-3">
      <h3 className="flex items-baseline justify-between font-display text-lead font-ui text-label-dim">
        {title}
        {value && <span className="text-body text-label tabular-nums">{value}</span>}
      </h3>
      {children}
    </section>
  );
}

/** Chips for the values of one facet, in `order`; a chosen value the wall no longer has stays, so it can be taken off. */
function Choices<T extends string>({
  id,
  order,
  present,
  chosen,
  name,
  onToggle,
}: {
  id: string;
  order: readonly T[];
  present: readonly T[];
  chosen: readonly T[] | undefined;
  name: (value: T) => string;
  onToggle: (value: T) => void;
}) {
  const shown = order.filter((v) => present.includes(v) || chosen?.includes(v));
  if (shown.length === 0) return null;
  return (
    <FocusGroup id={id} orientation="spatial" className="-mx-1 flex flex-wrap gap-2">
      {shown.map((v) => (
        <Chip key={v} focusKey={`${id}:${v}`} state={chosen?.includes(v) ? "in" : "off"} onPress={() => onToggle(v)}>
          {name(v)}
        </Chip>
      ))}
    </FocusGroup>
  );
}

const values = <T,>(list: Count<T>[] | undefined) => (list ?? []).map((c) => c.value);

function ActionButton({ focusKey, onSelect, children }: { focusKey: string; onSelect: () => void; children: ReactNode }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey, onSelect });
  return (
    <Button ref={ref} {...props} variant="line" className="relative focus:border-lamp">
      {children}
      {focused && <Lamp />}
    </Button>
  );
}

export interface FilterSheetProps {
  open: boolean;
  kind: ItemKind;
  filters: Filters;
  onFilters: (filters: Filters) => void;
  /** "212 movies": what the filters leave, as last counted. */
  count: string | null;
  /** A changed choice is being counted. */
  counting: boolean;
  onClose: () => void;
}

/** How many keywords the sheet offers: the most shared, enough to browse by. */
const KEYWORDS = 24;

export function FilterSheet({ open, kind, filters: f, onFilters, count, counting, onClose }: FilterSheetProps) {
  const t = useT();
  const facets: Facets | undefined = useFacets(kind).data;
  const set = (patch: Filters) => onFilters({ ...f, ...patch });
  const genres = [...values(facets?.genres), ...(f.genres ?? []), ...(f.without_genres ?? [])].filter(
    (g, i, all) => all.indexOf(g) === i,
  );
  const included = f.genres?.length ?? 0;
  // The keywords most works share, and any chosen that a narrower listing no longer counts.
  const keywords = [...values(facets?.keywords).slice(0, KEYWORDS), ...(f.keywords ?? [])].filter((k, i, all) => all.indexOf(k) === i);
  const languages = (present: string[], chosen: string[] | undefined) =>
    [...present, ...(chosen ?? []).filter((c) => !present.includes(c))];
  const audio = languages(values(facets?.audio), f.audio);
  const subtitles = languages(values(facets?.subtitles), f.subtitles);
  const pictureShown = [...values(facets?.resolution), ...values(facets?.picture)].length > 0;
  // A choice with nothing to leave is not offered, unless it is chosen and so can be taken off.
  const activity = hasActivity(useServices().data)
    ? ACTIVITY.filter((k) => (facets?.activity?.[k] ?? 0) > 0 || f[k])
    : [];

  return (
    <FocusScope
      id="filters"
      modal
      open={open}
      label={t("filters.title")}
      onBack={onClose}
      restoreKey="filters-seen:unwatched"
      className={sheet}
      backdropClassName={veil}
    >
      <header className="flex items-center gap-4 border-b border-line px-(--dialog-pad) py-6">
        <div className="mr-auto flex flex-col">
          <DialogTitle>{t("filters.title")}</DialogTitle>
          <p role="status" className={cn("font-display text-body text-label-dim tabular-nums transition-opacity", counting && "opacity-50")}>
            {count ?? " "}
          </p>
        </div>
        <FocusGroup id="filters-actions" orientation="horizontal" className="flex gap-3">
          {Object.keys(compact(f)).length > 0 && (
            <ActionButton focusKey="filters:clear" onSelect={() => onFilters({})}>
              {t("filters.clear")}
            </ActionButton>
          )}
          <ActionButton focusKey="filters:done" onSelect={onClose}>
            {t("common.done")}
          </ActionButton>
        </FocusGroup>
      </header>

      <div className="flex min-h-0 flex-1 scroll-pt-8 scroll-pb-(--filter-sheet-foot) flex-col gap-9 overflow-y-auto px-(--dialog-pad) pt-8 pb-(--filter-sheet-foot)">
        <Section title={t("filters.watching")}>
          <Choices id="filters-seen" order={SEEN} present={SEEN} chosen={f.seen} name={seenName} onToggle={(v) => set({ seen: toggled(f.seen, v) })} />
        </Section>
        {activity.length > 0 && (
          <Section title={t("filters.activity")}>
            <FocusGroup id="filters-activity" orientation="spatial" className="-mx-1 flex flex-wrap gap-2">
              {activity.map((k) => (
                <Chip
                  key={k}
                  focusKey={`filters:${k}`}
                  state={f[k] ? "in" : "off"}
                  count={facets?.activity?.[k]}
                  onPress={() => set({ [k]: f[k] ? null : true })}
                >
                  {activityName(k)}
                </Chip>
              ))}
            </FocusGroup>
          </Section>
        )}
        <Section title={t("filters.collection")}>
          <FocusGroup id="filters-collection" orientation="spatial" className="-mx-1 flex flex-wrap gap-2">
            <Chip focusKey="filters:loved" state={f.loved ? "in" : "off"} onPress={() => set({ loved: f.loved ? null : true })}>
              {t("filters.loved")}
            </Chip>
            <Chip focusKey="filters:available" state={f.available ? "in" : "off"} onPress={() => set({ available: f.available ? null : true })}>
              {t("filters.available")}
            </Chip>
          </FocusGroup>
        </Section>

        {genres.length > 0 && (
          <Section title={t("filters.genres")}>
            <FocusGroup id="filters-genres" orientation="spatial" className="-mx-1 flex flex-wrap gap-2">
              {genres.map((g) => (
                <Chip key={g} focusKey={`filters:genre:${g}`} state={genreState(f, g)} excludes onPress={() => onFilters(cycleGenre(f, g))}>
                  {g}
                </Chip>
              ))}
            </FocusGroup>
            {included > 1 && (
              <FocusGroup id="filters-genre-match" orientation="horizontal" className="-mx-1 flex gap-2">
                <Chip focusKey="filters:all-genres" state={f.any_genre ? "off" : "in"} onPress={() => set({ any_genre: false })}>
                  {t("filters.allOf")}
                </Chip>
                <Chip focusKey="filters:any-genre" state={f.any_genre ? "in" : "off"} onPress={() => set({ any_genre: true })}>
                  {t("filters.anyOf")}
                </Chip>
              </FocusGroup>
            )}
          </Section>
        )}

        {keywords.length > 0 && (
          <Section title={t("filters.keywords")}>
            <FocusGroup id="filters-keywords" orientation="spatial" className="-mx-1 flex flex-wrap gap-2">
              {keywords.map((k) => (
                <Chip
                  key={k}
                  focusKey={`filters:keyword:${k}`}
                  state={f.keywords?.includes(k) ? "in" : "off"}
                  onPress={() => set({ keywords: toggled(f.keywords, k) })}
                >
                  {k}
                </Chip>
              ))}
            </FocusGroup>
            {(f.keywords?.length ?? 0) > 1 && (
              <FocusGroup id="filters-keyword-match" orientation="horizontal" className="-mx-1 flex gap-2">
                <Chip focusKey="filters:all-keywords" state={f.any_keyword ? "off" : "in"} onPress={() => set({ any_keyword: false })}>
                  {t("filters.allOf")}
                </Chip>
                <Chip focusKey="filters:any-keyword" state={f.any_keyword ? "in" : "off"} onPress={() => set({ any_keyword: true })}>
                  {t("filters.anyOf")}
                </Chip>
              </FocusGroup>
            )}
          </Section>
        )}

        {(facets?.years.length ?? 0) > 1 && (
          <Section title={t("filters.year")} value={span(f.year_min, f.year_max, String)}>
            <RangeField
              id="filters-year"
              label={t("filters.year")}
              counts={facets!.years}
              step={1}
              leap={{ by: 10, label: t("filters.leapYears") }}
              min={f.year_min}
              max={f.year_max}
              onChange={(year_min, year_max) => set({ year_min, year_max })}
              name={String}
            />
          </Section>
        )}

        {(facets?.runtimes.length ?? 0) > 1 && (
          <Section title={t("filters.runtime")} value={span(f.runtime_min, f.runtime_max, minutes)}>
            <RangeField
              id="filters-runtime"
              label={t("filters.runtime")}
              counts={facets!.runtimes}
              bucket={10}
              step={5}
              leap={{ by: 30, label: t("filters.leapMinutes") }}
              min={f.runtime_min}
              max={f.runtime_max}
              onChange={(runtime_min, runtime_max) => set({ runtime_min, runtime_max })}
              name={minutes}
            />
          </Section>
        )}

        {pictureShown && (
          <Section title={t("filters.picture")}>
            <Choices id="filters-resolution" order={RESOLUTIONS} present={values(facets?.resolution)} chosen={f.resolution} name={formatName} onToggle={(v) => set({ resolution: toggled(f.resolution, v) })} />
            <Choices id="filters-range" order={PICTURES} present={values(facets?.picture)} chosen={f.picture} name={formatName} onToggle={(v) => set({ picture: toggled(f.picture, v) })} />
          </Section>
        )}

        {(facets?.sound.length ?? 0) > 0 && (
          <Section title={t("filters.soundTitle")}>
            <Choices id="filters-sound" order={SOUNDS} present={values(facets?.sound)} chosen={f.sound} name={formatName} onToggle={(v) => set({ sound: toggled(f.sound, v) })} />
          </Section>
        )}

        {audio.length > 0 && (
          <Section title={t("filters.audio")}>
            <Choices id="filters-audio" order={audio} present={audio} chosen={f.audio} name={langName} onToggle={(v) => set({ audio: toggled(f.audio, v) })} />
          </Section>
        )}
        {subtitles.length > 0 && (
          <Section title={t("filters.subtitles")}>
            <Choices id="filters-subtitles" order={subtitles} present={subtitles} chosen={f.subtitles} name={langName} onToggle={(v) => set({ subtitles: toggled(f.subtitles, v) })} />
          </Section>
        )}
      </div>
    </FocusScope>
  );
}
