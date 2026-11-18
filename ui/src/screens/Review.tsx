// Needs review: the files Exhibit could not tell for sure. Each entry names the file (with its
// folder) and what its name was read as, and hangs TMDB's candidates as works with their year and
// how likely each is. Choosing a candidate applies it; "Search by hand" opens the match picker;
// "Send to Other" keeps it as plain media. Whatever is done, the entry leaves the list and focus
// goes to the next entry (the previous one at the end), through the screen's focus fallback.

import { useCallback, useRef, useState, type FocusEvent } from "react";
import { Lamp } from "@/components/Lamp";
import { candidateCaption, candidateWork, openMatchPicker } from "@/components/MatchPicker";
import { itemKey, Shelf } from "@/components/Shelf";
import { Empty, ErrorState, Loading } from "@/components/states";
import { Button } from "@/components/ui/button";
import { useFocusState } from "@/focus/bridge";
import { FocusGroup, FocusScope, useFocusItem } from "@/focus/react";
import { useActionContext } from "@/lib/actions";
import { plain, type Candidate, type Id, type ItemDetail, type Source } from "@/lib/api";
import { count, facts, OFFLINE } from "@/lib/format";
import { revealRow, stopGlide } from "@/lib/motion";
import { useApplyMatch, useNeedsReview, useStatus } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useNav } from "@/nav/stack";

const shelfId = (id: Id) => `review-${id}`;
const searchKey = (id: Id) => `review-${id}:search`;
const otherKey = (id: Id) => `review-${id}:other`;
const ENTRY = /^review-(\d+):/;

/** Where focus lands in an entry: its first candidate, else "Search by hand". */
function firstKey(entry: ItemDetail): string {
  const first = entry.item.candidates[0];
  return first === undefined ? searchKey(entry.item.id) : itemKey(shelfId(entry.item.id), candidateWork(first, 0));
}

/** The entry focus is in, and its neighbours as they stood when focus came to it. */
interface Place {
  id: Id;
  next?: Id;
  prev?: Id;
}

/** Where focus goes when what it was on leaves: the same entry, else the next, else the previous, else the first. */
function landing(entries: ItemDetail[], at: Place | null): string | undefined {
  const find = (id: Id | undefined) => (id === undefined ? undefined : entries.find((e) => e.item.id === id));
  const entry = at === null ? entries[0] : (find(at.id) ?? find(at.next) ?? find(at.prev) ?? entries[0]);
  return entry === undefined ? undefined : firstKey(entry);
}

export function Review() {
  const query = useNeedsReview();
  if (query.isPending) return <Loading />;
  if (query.isError) {
    return (
      <ErrorState
        message={plain(query.error)}
        hint="The list could not be read. Try again, or restart Exhibit if it keeps happening."
        action={{ label: "Try again", onAction: () => query.refetch() }}
      />
    );
  }
  // A match applied or a file sent to Other leaves at once, before the library says so.
  const entries = query.data.filter((d) => d.item.match_state === "needsreview");
  if (entries.length === 0) {
    // The section leaves the top bar with its last entry, so the way on is offered here, and its
    // own scope puts focus on it when the last entry (and focus with it) has just gone.
    return (
      <FocusScope id="review-empty">
        <Empty
          title="Nothing needs review."
          body="When Exhibit cannot tell what a file is, it asks here."
          action={{ label: "Go to Home", onAction: () => useNav.getState().replaceRoot({ screen: "home" }) }}
        />
      </FocusScope>
    );
  }
  return <Entries entries={entries} />;
}

function Entries({ entries }: { entries: ItemDetail[] }) {
  const sources = useStatus().data?.sources ?? [];
  const [at, setAt] = useState<Place | null>(null);
  const list = useRef<HTMLDivElement>(null);

  const onFocusKey = (key: string) => {
    useNav.getState().setFocusKey(key);
    const id = Number(ENTRY.exec(key)?.[1]);
    if (!Number.isFinite(id) || id === at?.id) return;
    const i = entries.findIndex((e) => e.item.id === id);
    setAt({ id, next: entries[i + 1]?.item.id, prev: entries[i - 1]?.item.id });
  };

  const reveal = useCallback((el: HTMLElement) => {
    const scroller = list.current;
    const entry = el.closest("article");
    if (scroller === null || entry === null) return;
    revealRow(scroller, entry.offsetTop, entry.offsetTop + entry.offsetHeight, useFocusState.getState().mode === "pointer");
  }, []);
  const bindList = useCallback((el: HTMLDivElement | null) => {
    list.current = el;
    return () => {
      if (el !== null) stopGlide(el);
    };
  }, []);

  return (
    <FocusScope id="review" restoreKey={landing(entries, at)} fallbackKey={landing(entries, at)} onFocusKey={onFocusKey}>
      <div className="absolute inset-0 flex flex-col pt-bar">
        <header className="flex items-baseline gap-6 px-edge pt-4 pb-2">
          <h1 className="font-display text-title font-title tracking-title text-label">Needs review</h1>
          <p className="font-display text-body text-label-faint tabular-nums">{count(entries.length, "file", "files")}</p>
        </header>
        <div ref={bindList} className="wall-fade relative min-h-0 flex-1 overflow-y-auto">
          <div className="flex flex-col gap-(--review-gap) py-light">
            {entries.map((e) => (
              <Entry key={e.item.id} entry={e} source={sources.find((s) => s.id === e.files[0]?.source_id)} onReveal={reveal} />
            ))}
          </div>
        </div>
      </div>
    </FocusScope>
  );
}

/** "review-dummies/", "Solaris.mkv": the folder it is in (its source's, at the top) and its name. */
function fileParts(entry: ItemDetail, source: Source | undefined): { folder: string; name: string } {
  const rel = entry.files[0]?.rel_path ?? entry.item.title;
  const cut = rel.lastIndexOf("/");
  if (cut >= 0) return { folder: rel.slice(0, cut + 1), name: rel.slice(cut + 1) };
  const root = source?.path.split(/[\\/]/).filter(Boolean).at(-1);
  return { folder: root === undefined ? "" : `${root}/`, name: rel };
}

/** "Read as Solaris", "Read as Heat, 1995". */
const guessLine = (entry: ItemDetail) =>
  facts([`Read as ${entry.item.title}`, entry.item.year === null ? null : String(entry.item.year), entry.available ? null : OFFLINE]);

function Entry({ entry, source, onReveal }: { entry: ItemDetail; source: Source | undefined; onReveal: (el: HTMLElement) => void }) {
  const id = entry.item.id;
  const candidates = entry.item.candidates;
  const apply = useApplyMatch();
  const ctx = useActionContext();
  const { folder, name } = fileParts(entry, source);

  const choose = (c: Candidate) => {
    apply.mutateAsync({ id, tmdbId: c.tmdb_id, kind: c.kind }).catch((e: unknown) => ctx.fail(plain(e)));
  };
  const toOther = () => {
    ctx.moveToOther({ id }).catch((e: unknown) => ctx.fail(plain(e)));
  };

  return (
    <article aria-label={name} className="flex items-start">
      {candidates.length > 0 ? (
        <Shelf
          id={shelfId(id)}
          title={count(candidates.length, "possible match", "possible matches")}
          items={candidates.map(candidateWork)}
          caption={(w) => candidateCaption(candidates[w.id]!)}
          menu={false}
          light="review-light"
          onReveal={onReveal}
          onOpen={(w) => choose(candidates[w.id]!)}
          className="min-w-0 flex-1"
        />
      ) : (
        <p className="min-w-0 flex-1 px-edge pt-12 font-reading text-lead leading-reading text-label-dim">
          TMDB has nothing close to this name. Search by hand, or send it to Other.
        </p>
      )}
      {/* After the candidates in the page, so focus settles on a candidate first; shown on the left. */}
      <div className="order-first flex w-(--review-label) shrink-0 flex-col gap-3 pl-edge">
        <p className="font-display text-body text-label-faint [overflow-wrap:anywhere]">{folder}</p>
        <h2 className="-mt-2 font-display text-heading font-title tracking-title text-label [overflow-wrap:anywhere]">
          {name}
        </h2>
        <p className="font-display text-body text-label-dim">{guessLine(entry)}</p>
        <FocusGroup
          id={`review-${id}-actions`}
          orientation="horizontal"
          reveals
          onFocus={(e: FocusEvent<HTMLDivElement>) => onReveal(e.currentTarget)}
          className="mt-3 -ml-4 flex gap-2"
        >
          {/* Search by hand sits nearest the candidates, so Left from them never lands on Send to Other. */}
          <EntryButton focusKey={otherKey(id)} onSelect={toOther}>
            Send to Other
          </EntryButton>
          <EntryButton focusKey={searchKey(id)} onSelect={() => openMatchPicker(id)}>
            Search by hand
          </EntryButton>
        </FocusGroup>
      </div>
    </article>
  );
}

function EntryButton({ focusKey, onSelect, children }: { focusKey: string; onSelect: () => void; children: string }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey, onSelect });
  return (
    <Button ref={ref} {...props} className={cn("relative focus-visible:bg-transparent focus:text-label")}>
      {children}
      {focused && <Lamp />}
    </Button>
  );
}
