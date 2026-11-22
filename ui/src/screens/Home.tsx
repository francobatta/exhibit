// Home: the programme for now. The part of the day names it ("Tonight", "After hours") and sets its
// shelves; one work is put forward on the hero, with focus on its Play, so one press starts the
// evening. A line above the placard greets the owner with the programme's name and why that work
// was chosen; it goes once they look elsewhere. On an exhibition (a curator's shelf) the same line
// is the work's wall label: the exhibition's title and the curator's note on it. Beside Play, when
// the work would end if started now.

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ErrorState, Loading } from "@/components/states";
import { useT } from "@/i18n";
import { hasActivity, hasReception } from "@/lib/actions";
import { plain } from "@/lib/api";
import { momentOf, type Moment } from "@/lib/moment";
import { remPx, rems } from "@/lib/motion";
import { onHome, pick, programme, type RowId } from "@/lib/programme";
import type { Exhibition } from "@/lib/api";
import { useExhibitions, useJobStatus, useRows, useServices, useStatus } from "@/lib/queries";
import { NothingToShow, Stage, type Row } from "./Spotlight";

export function Home() {
  const t = useT();
  // The programme is set as the owner arrives, so shelves never change under them; the clock on
  // the placards keeps time.
  const [moment] = useState(() => momentOf(new Date()));
  const clock = useClock();
  const services = useServices();
  const activity = hasActivity(services.data);
  const plan = useMemo(() => programme(moment, activity), [moment, activity]);
  const queries = useRows(plan.map((r) => r.source));
  const exhibitions = useExhibitions();
  const job = useJobStatus();
  const status = useStatus().data;

  if (services.isPending || exhibitions.isPending || queries.some((q) => q.isPending)) return <Loading />;
  const failed = queries.find((q) => q.isError);
  if (failed) {
    return (
      <ErrorState
        message={plain(failed.error)}
        hint={t("common.libraryReadFailed")}
        action={{ label: t("common.tryAgain"), onAction: () => queries.forEach((q) => void q.refetch()) }}
      />
    );
  }
  const shelves = plan
    .map((r, i) => ({ id: r.id, items: (queries[i]!.data ?? []).filter((w) => onHome(w, r.id)) }))
    .filter((r) => r.items.length > 0);
  const chosen = pick(shelves, moment);
  if (chosen === null) return <NothingToShow job={job} status={status} />;
  const rows: Row[] = shelves.map((r) => ({ key: r.id, title: rowTitle(r.id, moment, t), items: r.items }));
  // Exhibitions hang after what is under way: a curator's shows, each with its wall text and a
  // note under every work. One that has lost its works (gone from the library) does not hang.
  const hung = (exhibitions.data ?? []).filter((e) => e.works.length > 0).map(exhibitionRow);
  rows.splice(rows[0]?.key === "continue" ? 1 : 0, 0, ...hung);

  return (
    <Stage
      rows={rows}
      verdicts={activity || hasReception(services.data)}
      first={chosen.item}
      onCues
      clock={clock}
      head={(shown, shelf) => {
        const hung = (exhibitions.data ?? []).find((e) => `exhibition-${e.id}` === shelf);
        const note = hung?.works.find((w) => w.item.id === shown.id)?.note;
        return hung !== undefined && note ? (
          <Label name={hung.title} why={note} shown />
        ) : (
          <Label name={t(`home.programme.${moment.daypart}`)} why={whyLine(chosen.row, moment, t)} shown={shown.id === chosen.item.id} />
        );
      }}
    />
  );
}

type T = ReturnType<typeof useT>;

function exhibitionRow(e: Exhibition): Row {
  return {
    key: `exhibition-${e.id}`,
    title: e.title,
    note: e.wall_text,
    items: e.works.map((w) => w.item),
  };
}

function rowTitle(id: RowId, moment: Moment, t: T): string {
  switch (id) {
    case "continue":
      return t("home.continueWatching");
    case "recent":
      return t("home.recentlyAdded");
    case "loved":
      return t("home.loved");
    case "season":
      return t(`home.rows.${moment.season}`);
    default:
      return t(`home.rows.${id}`);
  }
}

function whyLine(id: RowId, moment: Moment, t: T): string {
  return id === "season" ? t(`home.rows.${moment.season}`) : t(`home.why.${id}`);
}

/**
 * A name and a line of why above the placard: the programme and why the pick was put forward, or
 * an exhibition and the curator's note on the work. It stands out of the placard's flow; a title
 * that leaves no room under the bar goes without it rather than run into the bar.
 */
function Label({ name, why, shown }: { name: string; why: string; shown: boolean }) {
  const ref = useRef<HTMLParagraphElement>(null);
  const [fits, setFits] = useState(true);
  useLayoutEffect(() => {
    const el = ref.current;
    if (el === null) return;
    // Where it stands in the layout, untouched by a screen's entering transform; it must clear
    // the bar's controls, which stand in the middle of the bar.
    const check = () => {
      let top = 0;
      for (let e: HTMLElement | null = el; e !== null; e = e.offsetParent as HTMLElement | null) top += e.offsetTop;
      const clear = (rems("--bar-height") + rems("--bar-control")) / 2 + rems("--home-greeting-clear");
      setFits(top >= clear * remPx());
    };
    check();
    // The placard grows with its title (a font arriving, a new size of window).
    const observer = new ResizeObserver(check);
    if (el.parentElement?.parentElement) observer.observe(el.parentElement.parentElement);
    return () => observer.disconnect();
  }, []);
  return (
    <p
      ref={ref}
      data-programme
      data-shown={shown && fits}
      className="flex items-baseline gap-6 transition-opacity duration-(--dur-programme) data-[shown=false]:opacity-0"
    >
      <span className="shrink-0 font-display text-title font-title tracking-title text-label">{name}</span>
      <span className="font-reading text-lead text-label-soft">{why}</span>
    </p>
  );
}

/** Now, in ms, kept to the minute. */
function useClock(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    // Tick on the minute, then every minute.
    let timer = setTimeout(function tick() {
      setNow(Date.now());
      timer = setTimeout(tick, 60_000 - (Date.now() % 60_000));
    }, 60_000 - (Date.now() % 60_000));
    return () => clearTimeout(timer);
  }, []);
  return now;
}

