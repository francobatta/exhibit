// Development only: every work component on one wall, with real and awkward data, to look at
// them before the real screens exist. Opened with `?gallery` (or VITE_EXHIBIT_GALLERY=1); never
// linked from the interface and left out of production builds.

import { useEffect, useMemo, useState } from "react";
import { useBackdrop } from "@/components/Backdrop";
import { Placard } from "@/components/Placard";
import { Shelf } from "@/components/Shelf";
import { WallGrid } from "@/components/WallGrid";
import { FocusScope } from "@/focus/react";
import type { ItemSummary } from "@/lib/api";
import { useItem, useItems, useShelf } from "@/lib/queries";

const LONG = "The Assassination of Jesse James by the Coward Robert Ford, as Told in Ninety Characters!!";

/** Works no real library is kind enough to have. Negative ids, so they never meet real ones. */
function awkward(real: ItemSummary[]): ItemSummary[] {
  const art = (i: number) => real[i % Math.max(1, real.length)]?.poster ?? null;
  const base: ItemSummary = {
    id: 0,
    kind: "movie",
    title: "",
    year: 2001,
    season: null,
    episode: null,
    show_title: null,
    poster: null,
    backdrop: null,
    runtime_min: 104,
    width: 1920,
    height: 1080,
    hdr: null,
    watched: false,
    progress: null,
    loved: false,
    available: true,
    match_state: "matched",
  };
  return [
    { ...base, id: -1, title: LONG, year: 2007, runtime_min: 160 },
    { ...base, id: -2, title: "千と千尋の神隠し", year: 2001, runtime_min: 125 },
    { ...base, id: -3, title: "Untitled home video", kind: "other", year: null, runtime_min: null, width: null, height: null },
    { ...base, id: -4, title: "No year, no overview", year: null, runtime_min: 0 },
    { ...base, id: -5, title: "On the unplugged drive", poster: art(0), available: false },
    { ...base, id: -6, title: "Seen it", poster: art(1), watched: true, progress: 0.97 },
    { ...base, id: -7, title: "Halfway through", poster: art(2), progress: 0.48, width: 3840, height: 1608, hdr: "Dolby Vision" },
    { ...base, id: -8, title: "Needs review", kind: "other", match_state: "needsreview", poster: art(3) },
  ];
}

export function Gallery() {
  const recent = useShelf("recently_added", 300);
  const movies = useItems({ kind: "movie", sort: "title" });
  const real = useMemo(() => recent.data ?? [], [recent.data]);
  const odd = useMemo(() => awkward(real), [real]);
  const [hero, setHero] = useState<ItemSummary | null>(null);
  const [panel, setPanel] = useState<ItemSummary | null>(null);
  const shown = hero ?? real[0] ?? null;
  const detail = useItem(shown !== null && shown.id > 0 ? shown.id : null).data;
  const panelDetail = useItem(panel !== null && panel.id > 0 ? panel.id : null).data;

  useEffect(() => {
    useBackdrop.getState().show(shown?.backdrop ?? null, shown?.poster ?? null);
  }, [shown?.backdrop, shown?.poster]);

  // A scope settles its focus as it mounts, so it waits for the data its first group shows.
  if (recent.isPending || movies.isPending) return null;
  return (
    <div className="absolute inset-0 overflow-x-hidden overflow-y-auto pt-bar">
      <FocusScope id="gallery" restoreKey={real[0] ? `item:${real[0].id}` : null}>
        <div className="flex flex-col gap-14 pb-24">
          <div className="flex min-h-[26rem] items-end px-edge pt-10">
            {shown && <Placard item={shown} detail={detail?.item.id === shown.id ? detail : undefined} size="hero" />}
          </div>
          <Shelf id="g-recent" title="Recently added" items={real} onOpen={() => {}} onFocusItem={setHero} />
          <section className="flex flex-col gap-4">
            <h2 className="px-edge font-display text-lead font-ui text-label-dim">All movies</h2>
            <div className="flex h-[44rem] gap-10 pr-edge">
              <WallGrid
                id="g-wall"
                items={movies.items}
                columns={7}
                shape="poster"
                onOpen={() => {}}
                onFocusItem={(i) => {
                  setPanel(i);
                  setHero(i);
                }}
                className="min-w-0 flex-1 pr-0"
                aria-label="All movies"
              />
              <div className="w-[24rem] shrink-0 pt-12">
                {panel && <Placard item={panel} detail={panelDetail?.item.id === panel.id ? panelDetail : undefined} size="panel" />}
              </div>
            </div>
          </section>
          <Shelf id="g-awkward" title="Awkward works" items={odd} onOpen={() => {}} onFocusItem={setHero} />
          <Shelf id="g-wide" title="Wide works" items={[...odd.slice(2, 4), ...real]} shape="wide" onOpen={() => {}} onFocusItem={setHero} />
        </div>
      </FocusScope>
    </div>
  );
}
