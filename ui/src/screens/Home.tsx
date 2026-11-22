// Home: the programme for now. The part of the day names it ("Tonight", "After hours") and sets its
// shelves; one work is put forward on the hero, with focus on its Play, so one press starts the
// evening. A line above the placard greets the owner with the programme's name and why that work
// was chosen; it goes once they look elsewhere. Beside Play, when the work would end if started now.

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ErrorState, Loading } from "@/components/states";
import { useT } from "@/i18n";
import { hasActivity, hasReception } from "@/lib/actions";
import { plain } from "@/lib/api";
import { momentOf, type Moment } from "@/lib/moment";
import { remPx, rems } from "@/lib/motion";
import { onHome, pick, programme, type RowId } from "@/lib/programme";
import { useJobStatus, useRows, useServices, useStatus } from "@/lib/queries";
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
  const job = useJobStatus();
  const status = useStatus().data;

  if (services.isPending || queries.some((q) => q.isPending)) return <Loading />;
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

  return (
    <Stage
      rows={rows}
      verdicts={activity || hasReception(services.data)}
      first={chosen.item}
      onCues
      clock={clock}
      head={(shown) => <Programme moment={moment} why={whyLine(chosen.row, moment, t)} shown={shown.id === chosen.item.id} />}
    />
  );
}

type T = ReturnType<typeof useT>;

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
 * The programme's name and why the work on the hero was put forward, while it is the one shown.
 * It stands above the placard out of its flow; a pick whose title leaves no room under the bar
 * goes without it rather than run into the bar.
 */
function Programme({ moment, why, shown }: { moment: Moment; why: string; shown: boolean }) {
  const t = useT();
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
      <span className="font-display text-title font-title tracking-title text-label">{t(`home.programme.${moment.daypart}`)}</span>
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

