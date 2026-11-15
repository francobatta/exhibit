// Home, as a stub until Tasks 5 and 6: the most recently added work, its backdrop and tint on the
// wall, and its wall label set in our type. Proves the foundation; Placard and shelves replace it.

import { useEffect } from "react";
import { useBackdrop } from "@/components/Backdrop";
import { Lamp } from "@/components/TopBar";
import { FocusGroup, useFocusItem } from "@/focus/react";
import type { ItemSummary } from "@/lib/api";
import { Empty, ErrorState, Loading } from "@/components/states";
import { plain } from "@/lib/api";
import { facts, resolution, runtime } from "@/lib/format";
import { useItem, useShelf } from "@/lib/queries";
import { useNav } from "@/nav/stack";

// Until Task 6 hangs shelves: the newest titles as plain text, so focus has somewhere to go.
function RecentTitle({ item }: { item: ItemSummary }) {
  const push = useNav((s) => s.push);
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: `item:${item.id}`,
    onSelect: () => push({ screen: item.kind === "show" ? "show" : "details", id: item.id }),
  });
  return (
    <button
      ref={ref}
      {...props}
      type="button"
      className="relative max-w-[16rem] truncate py-3 font-display text-lead font-ui text-label-dim outline-none transition-colors focus:text-label"
    >
      {item.title}
      {focused && <Lamp />}
    </button>
  );
}

export function Home() {
  const shelf = useShelf("recently_added", 20);
  const first = shelf.data?.[0] ?? null;
  const detail = useItem(first?.id ?? null);

  useEffect(() => {
    useBackdrop.getState().show(first?.backdrop ?? null, first?.poster ?? null);
  }, [first?.backdrop, first?.poster]);

  if (shelf.isPending) return <Loading />;
  if (shelf.isError) {
    return (
      <ErrorState
        message={plain(shelf.error)}
        hint="The library could not be read. Try again, or restart Exhibit if it keeps happening."
        action={{ label: "Try again", onAction: () => shelf.refetch() }}
      />
    );
  }
  if (first === null) {
    return (
      <Empty
        title="Nothing on the walls yet"
        body="Add a folder of films in Settings and they will hang here."
      />
    );
  }

  const director = detail.data?.item.people.find((p) => p.role === "director")?.name;
  const overview = detail.data?.item.overview ?? null;

  return (
    <section className="flex min-h-full flex-col justify-center px-edge pb-[18vh]">
      <div className="flex max-w-measure flex-col gap-5">
        <h1 className="font-display text-display font-title tracking-display text-label [font-stretch:var(--stretch-poster)] text-balance">
          {first.title}
        </h1>
        <div className="flex flex-col gap-1 font-display text-lead text-label-dim">
          <p>{facts([first.year === null ? null : String(first.year), director])}</p>
          <p>
            {facts([
              runtime(first.runtime_min),
              resolution(first.height, first.hdr, first.width),
              first.watched ? "Watched" : null,
              first.available ? null : "Offline",
            ])}
          </p>
        </div>
        {overview && (
          <p className="line-clamp-2 font-reading text-lead leading-reading text-label-dim">
            {overview}
          </p>
        )}
      </div>
      <FocusGroup
        id="home-recent"
        orientation="horizontal"
        aria-label="Recently added"
        className="mt-14 flex items-center gap-10"
      >
        {(shelf.data ?? []).slice(0, 5).map((item) => (
          <RecentTitle key={item.id} item={item} />
        ))}
      </FocusGroup>
    </section>
  );
}
