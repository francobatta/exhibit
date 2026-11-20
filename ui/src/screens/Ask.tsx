// Ask: an overlay over any screen, opened by Y, `/`, Ctrl+K or the top bar's field, closed by B
// back to exactly where focus was. In this stage it is plain search: one large field and, as the
// owner types (after a 200 ms pause), the library's matches as a shelf per kind. A on a result
// runs its main action, X opens its menu. With nothing typed, the last six searches are offered.
// Stage 4 swaps only `useAsk`'s source for the agent.

import { useCallback, useEffect, useRef, useState } from "react";
import { Lamp } from "@/components/Lamp";
import { itemKey, Shelf } from "@/components/Shelf";
import { ErrorState } from "@/components/states";
import { TextField } from "@/components/TextField";
import { dialogBackdrop } from "@/components/ui/dialog";
import { focusItem, useFocusState } from "@/focus/bridge";
import { FocusGroup, FocusScope, useFocusItem } from "@/focus/react";
import { useT } from "@/i18n";
import { runMain, useActionContext } from "@/lib/actions";
import { plain, type ItemSummary } from "@/lib/api";
import { revealRow } from "@/lib/motion";
import { useAsk, type AskResult } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { openSettings } from "@/nav/Screens";
import { closeAsk, leaveAsk, useAskOverlay } from "@/nav/stack";

const FIELD = "ask:field";
/** How long typing pauses before the library is searched, in ms. */
const DEBOUNCE = 200;

// The last searches, newest first, kept on this machine.
const RECENT_KEY = "exhibit.ask.recent";
const RECENT_MAX = 6;

/** The recent searches, newest first; an empty list when what is stored cannot be read. */
export function recentSearches(): string[] {
  let stored: unknown;
  try {
    stored = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
  } catch {
    return [];
  }
  return Array.isArray(stored) ? stored.filter((q): q is string => typeof q === "string") : [];
}

/** Puts `text` first among the recent searches: one of each (whatever the case), six at most. */
export function rememberSearch(text: string): void {
  const q = text.trim();
  if (q === "") return;
  const rest = recentSearches().filter((r) => r.toLowerCase() !== q.toLowerCase());
  localStorage.setItem(RECENT_KEY, JSON.stringify([q, ...rest].slice(0, RECENT_MAX)));
}

/** `value`, once it has stopped changing for `ms`; an empty value at once. `flush` settles it now. */
function useDebounced(value: string, ms: number): [settled: string, flush: () => void] {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    if (value === "") return setSettled("");
    const timer = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return [value === "" ? "" : settled, () => setSettled(value)];
}

const groupId = (title: string) => `ask-${title.toLowerCase()}`;
const recentKey = (text: string) => `ask:recent:${text}`;

/** Mount once near the app's root. Each opening starts afresh. */
export function AskHost() {
  const open = useAskOverlay((s) => s.open);
  const [serial, setSerial] = useState(0);
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (open) setShown(true);
  }, [open]);
  if (!open && !shown) return null;
  return (
    <Ask
      key={serial}
      open={open}
      onClosed={() => {
        setShown(false);
        setSerial((n) => n + 1);
        const then = useAskOverlay.getState().then;
        useAskOverlay.setState({ then: null });
        then?.();
      }}
    />
  );
}

export function Ask({ open, onClosed }: { open: boolean; onClosed?: () => void }) {
  const t = useT();
  const placeholder = t("topBar.search");
  const [text, setText] = useState(() => useAskOverlay.getState().seed);
  const [query, flush] = useDebounced(text.trim(), DEBOUNCE);
  const results = useAsk(query);
  const [recent] = useState(recentSearches);

  // Enter keeps the search and goes to its first result, once the results are those of the text
  // as typed (not of what was typed before the pause). Typing on forgets the jump.
  const [jump, setJump] = useState<string | null>(null);
  const type = (next: string) => {
    setText(next);
    setJump(null);
  };
  const submit = () => {
    rememberSearch(text);
    if (text.trim() === "") return;
    flush();
    setJump(text.trim());
  };
  const ready = jump !== null && jump === query && !results.isPlaceholderData ? results.data : undefined;
  useEffect(() => {
    if (ready === undefined) return;
    setJump(null);
    const group = ready.groups[0];
    if (group !== undefined) focusItem(itemKey(groupId(group.title), group.items[0]!));
  }, [ready]);

  // The results glide into view (lib/motion), as lists elsewhere do.
  const scroller = useRef<HTMLDivElement>(null);
  const reveal = useCallback((el: HTMLElement) => {
    if (scroller.current !== null) {
      revealRow(scroller.current, el.offsetTop, el.offsetTop + el.offsetHeight, useFocusState.getState().mode === "pointer");
    }
  }, []);

  return (
    <FocusScope
      id="ask"
      modal
      open={open}
      label={placeholder}
      restoreKey={FIELD}
      fallbackKey={FIELD}
      onBack={() => closeAsk()}
      onAsk={() => focusItem(FIELD)}
      // The pad's Menu opens Settings over Ask too.
      onDone={() => leaveAsk(openSettings)}
      onClosed={onClosed}
      backdropClassName={cn(dialogBackdrop, "bg-wall")}
      className={cn(
        "fixed inset-0 z-50 flex flex-col pt-(--ask-top) outline-none",
        "transition-opacity duration-(--dur-screen) data-starting-style:opacity-0 data-ending-style:opacity-0",
      )}
    >
      <FocusGroup id="ask-field" orientation="horizontal" className="px-edge">
        <TextField
          focusKey={FIELD}
          value={text}
          onChange={type}
          onSubmit={submit}
          placeholder={placeholder}
          className={cn(
            "h-(--ask-field) rounded-none border-0 border-b border-line bg-transparent px-0",
            "text-title font-title tracking-title focus:border-lamp/60 focus:bg-transparent",
          )}
        />
      </FocusGroup>
      <div ref={scroller} className="wall-fade relative min-h-0 flex-1 overflow-y-auto pt-8">
        {query === "" ? (
          <Recent recent={recent} onChoose={type} />
        ) : results.isError ? (
          <ErrorState
            message={plain(results.error)}
            hint={t("ask.searchFailed")}
          />
        ) : results.data === undefined ? null : (
          <Results result={results.data} text={query} onChosen={() => rememberSearch(query)} onReveal={reveal} />
        )}
      </div>
    </FocusScope>
  );
}

function Results({
  result,
  text,
  onChosen,
  onReveal,
}: {
  result: AskResult;
  text: string;
  onChosen: () => void;
  onReveal: (el: HTMLElement) => void;
}) {
  const ctx = useActionContext();
  if (result.groups.length === 0) {
    return (
      <div className="px-edge">
        <h2 className="font-display text-heading font-title tracking-title text-label">Nothing matches “{text}”</h2>
        <p className="mt-4 font-reading text-lead leading-reading text-label-dim">
          Check the spelling, or search for someone who made it or is in it.
        </p>
      </div>
    );
  }
  const open = (item: ItemSummary) => {
    onChosen();
    runMain(item, ctx);
  };
  return (
    <div className="flex flex-col gap-4 pb-edge">
      {result.groups.map((g) => (
        <Shelf
          key={g.title}
          id={groupId(g.title)}
          title={g.title}
          items={g.items}
          shape={g.items[0]!.kind === "movie" || g.items[0]!.kind === "show" ? "poster" : "wide"}
          light="ask-light"
          onReveal={onReveal}
          onOpen={open}
        />
      ))}
    </div>
  );
}

function Recent({ recent, onChoose }: { recent: string[]; onChoose: (text: string) => void }) {
  const t = useT();
  if (recent.length === 0) {
    return (
      <p className="px-edge font-reading text-lead leading-reading text-label-dim">
        {t("ask.hint")}
      </p>
    );
  }
  return (
    <section aria-label={t("ask.recent")} className="px-edge">
      <h2 className="mb-4 font-display text-lead font-ui text-label-dim">{t("ask.recent")}</h2>
      <FocusGroup id="ask-recent" orientation="vertical" className="-ml-4 flex flex-col items-start">
        {recent.map((q) => (
          <RecentLine key={q} text={q} onChoose={() => onChoose(q)} />
        ))}
      </FocusGroup>
    </section>
  );
}

function RecentLine({ text, onChoose }: { text: string; onChoose: () => void }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: recentKey(text), onSelect: onChoose });
  return (
    <button
      ref={ref}
      {...props}
      type="button"
      className={cn(
        "relative px-4 py-2 font-display text-heading font-ui outline-none transition-colors",
        focused ? "text-label" : "text-label-dim",
      )}
    >
      {text}
      {focused && <Lamp />}
    </button>
  );
}
