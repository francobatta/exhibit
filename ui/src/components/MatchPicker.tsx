// Fix match: look a work up on TMDB by hand and choose what it is. A dialog with the kind (Movie
// or Show), the title and an optional year; it looks up the work's own guess as it opens, and
// choosing a result applies it. Without a TMDB key it says so and offers Settings instead, and
// never looks anything up. Opened by "Fix match" anywhere (`openMatchPicker`) and by Review's
// "Search by hand".

import { useQuery } from "@tanstack/react-query";
import { useRef, useState, type ReactNode } from "react";
import { create } from "zustand";
import { FocusGroup, useFocusItem } from "@/focus/react";
import * as api from "@/lib/api";
import { plain, type Candidate, type Id, type ItemDetail, type ItemKind, type ItemSummary } from "@/lib/api";
import { confidence, facts } from "@/lib/format";
import { useApplyMatch, useItem, useStatus } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { openSettings } from "@/nav/Screens";
import { leaveAsk } from "@/nav/stack";
import { Dialog } from "./Dialog";
import { Lamp } from "./Lamp";
import { Shelf } from "./Shelf";
import { TextField } from "./TextField";
import { Button } from "./ui/button";
import { toast } from "./ui/toast";
import { Toggle } from "./ui/toggle";

/** A candidate hung as a work: a typographic poster of its title. Its id is its place in the list. */
export function candidateWork(c: Candidate, index: number): ItemSummary {
  return {
    id: index,
    kind: c.kind,
    title: c.title,
    year: c.year,
    season: null,
    episode: null,
    show_title: null,
    poster: null,
    backdrop: null,
    runtime_min: null,
    width: null,
    height: null,
    hdr: null,
    watched: false,
    progress: null,
    loved: false,
    available: true,
    match_state: "matched",
    genres: [],
    path: null,
  };
}

/** Under a candidate: "1972, Likely". */
export const candidateCaption = (c: Candidate) => facts([c.year === null ? null : String(c.year), confidence(c.score)]);

interface PickerState {
  itemId: Id | null;
  open: boolean;
}

const usePicker = create<PickerState>()(() => ({ itemId: null, open: false }));

/** Opens Fix match for the work `itemId`. */
export function openMatchPicker(itemId: Id): void {
  if (usePicker.getState().open) return;
  usePicker.setState({ itemId, open: true });
}

/** Mount once near the app's root, for `openMatchPicker`. */
export function MatchPickerHost() {
  const { itemId, open } = usePicker();
  if (itemId === null) return null;
  return (
    <MatchPicker
      key={itemId}
      itemId={itemId}
      open={open}
      onClose={() => usePicker.setState({ open: false })}
      onClosed={() => usePicker.setState({ itemId: null })}
    />
  );
}

export interface MatchPickerProps {
  itemId: Id;
  open: boolean;
  onClose: () => void;
  /** Finished closing, focus returned. */
  onClosed?: () => void;
}

const TITLE = "Fix match";

export function MatchPicker({ itemId, open, onClose, onClosed }: MatchPickerProps) {
  const status = useStatus().data;
  const detail = useItem(itemId).data;
  // What to do once the dialog has gone (Settings opens only with no overlay in the way).
  const after = useRef<(() => void) | null>(null);
  const closed = () => {
    after.current?.();
    after.current = null;
    onClosed?.();
  };
  const ready = status !== undefined && detail !== undefined;

  if (ready && !status.tmdb_key) {
    return (
      <Dialog
        id="match"
        open={open}
        title={TITLE}
        body="Matching looks works up on TMDB, which needs a key. Add one in Settings, then fix the match again."
        actions={[
          {
            label: "Open Settings",
            primary: true,
            onSelect: () => {
              after.current = () => leaveAsk(openSettings);
              onClose();
            },
          },
          { label: "Cancel", onSelect: onClose },
        ]}
        onBack={onClose}
        onClosed={closed}
      />
    );
  }
  return (
    <Dialog
      id="match"
      open={open && ready}
      title={TITLE}
      body={fileName(detail)}
      actions={[{ label: "Cancel", onSelect: onClose }]}
      onBack={onClose}
      onClosed={closed}
      restoreKey="match:text"
      className="w-[min(var(--dialog-wide),calc(100vw-4rem))]"
    >
      {detail !== undefined && <Search detail={detail} onDone={onClose} />}
    </Dialog>
  );
}

/** The work's first file by its name alone, what the owner knows it by; else its title. */
const fileName = (detail: ItemDetail | undefined) => detail?.files[0]?.rel_path.split("/").at(-1) ?? detail?.item.title;

interface Asked {
  text: string;
  kind: ItemKind;
  year: number | null;
}

const yearOf = (text: string) => (/^\d{4}$/.test(text.trim()) ? Number(text.trim()) : null);

function Search({ detail, onDone }: { detail: ItemDetail; onDone: () => void }) {
  const item = detail.item;
  const [kind, setKind] = useState<ItemKind>(item.kind === "show" ? "show" : "movie");
  const [text, setText] = useState(item.title);
  const [year, setYear] = useState(item.year === null ? "" : String(item.year));
  // The work's own guess is looked up as the dialog opens.
  const [asked, setAsked] = useState<Asked>({ text: item.title, kind, year: item.year });
  const ask = () => setAsked({ text: text.trim(), kind, year: yearOf(year) });

  const results = useQuery({
    queryKey: ["lookup", asked],
    queryFn: () => api.lookup(asked.text, asked.kind, asked.year),
    enabled: asked.text !== "",
    retry: false,
    staleTime: Infinity,
  });
  const apply = useApplyMatch();
  const choose = (c: Candidate) => {
    onDone();
    apply.mutateAsync({ id: item.id, tmdbId: c.tmdb_id, kind: c.kind }).catch((e: unknown) => toast(plain(e)));
  };
  const candidates = results.data ?? [];

  return (
    <div className="flex flex-col gap-6">
      <FocusGroup id="match-form" orientation="horizontal" aria-label="Search" className="flex items-center gap-3">
        {(["movie", "show"] as const).map((k) => (
          <KindToggle key={k} kind={k} pressed={kind === k} onPress={() => setKind(k)} />
        ))}
        <TextField focusKey="match:text" value={text} onChange={setText} onSubmit={ask} placeholder="Title" className="ml-3 min-w-0 flex-1" />
        <TextField focusKey="match:year" value={year} onChange={setYear} onSubmit={ask} placeholder="Year" inputMode="numeric" className="w-28" />
        <FormButton focusKey="match:search" onSelect={ask}>
          Search
        </FormButton>
      </FocusGroup>
      {/* The shelf's edge padding is the panel's here, so the works line up with the fields. */}
      <div className="-mx-8 flex min-h-(--match-results) flex-col justify-center [--space-edge:2rem]">
        {asked.text === "" ? (
          <Note>Type a title to look up.</Note>
        ) : results.isPending ? (
          <Note>Looking up {asked.text}</Note>
        ) : results.isError ? (
          <Note>{plain(results.error)}</Note>
        ) : candidates.length === 0 ? (
          <Note>
            Nothing on TMDB matches “{asked.text}”{asked.year !== null && ` from ${asked.year}`}. Try another spelling, or
            leave out the year.
          </Note>
        ) : (
          <Shelf
            id="match-results"
            title={candidates.length === 1 ? "1 result" : `${candidates.length} results`}
            items={candidates.map(candidateWork)}
            caption={(w) => {
              const c = candidates[w.id]!;
              const current = c.tmdb_id === item.tmdb_id && c.kind === item.kind;
              return facts([candidateCaption(c), current ? "Matched now" : null]);
            }}
            menu={false}
            light="match-light"
            onOpen={(w) => choose(candidates[w.id]!)}
          />
        )}
      </div>
    </div>
  );
}

function Note({ children }: { children: ReactNode }) {
  return <p className="px-8 font-reading text-lead leading-reading text-label-dim">{children}</p>;
}

const control = "relative focus-visible:bg-transparent focus:text-label";

function KindToggle({ kind, pressed, onPress }: { kind: "movie" | "show"; pressed: boolean; onPress: () => void }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: `match:kind:${kind}` });
  return (
    <Toggle ref={ref} {...props} variant="plain" pressed={pressed} onPressedChange={(on) => on && onPress()} className={control}>
      {kind === "movie" ? "Movie" : "Show"}
      {focused && <Lamp />}
    </Toggle>
  );
}

function FormButton({ focusKey, onSelect, children }: { focusKey: string; onSelect: () => void; children: ReactNode }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey, onSelect });
  return (
    <Button ref={ref} {...props} variant="line" className={cn("relative focus:border-lamp")}>
      {children}
      {focused && <Lamp />}
    </Button>
  );
}
