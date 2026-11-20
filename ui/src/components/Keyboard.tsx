// The on-screen keyboard: how the pad types. A modal scope docked along the bottom of the window,
// over whatever field opened it; that field's text, with a lamp-white caret, sits on the
// keyboard's top edge, so what is being typed is always in view whatever the keyboard covers.
// The keys are one focus group on a ten-column grid, moved by row and column (`gridMove`, the
// group's UI SDK roving `move`): Up and Down keep to the column the owner was in, across the
// wide keys of the last row. The edges hold, never wrap: a held direction stops there.
// A types the lit key, X deletes, Y types a space, the Menu button is Done, B closes.

import { motion } from "motion/react";
import { useLayoutEffect, useRef, useState } from "react";
import { FocusGroup, FocusScope, useFocusItem, type GroupMove } from "@/focus/react";
import { language, t, useT, type Language } from "@/i18n";
import { focusSpring, numberToken } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { HintLine, type Hint } from "./Hints";

type Page = "letters" | "symbols";

/** The four rows of characters on each page, ten to a row. */
const ROWS: Record<Page, string[]> = {
  letters: ["1234567890", "qwertyuiop", "asdfghjkl'", "zxcvbnm,.-"],
  symbols: ["1234567890", "!@#$%&*()_", "+=/\\:;\"?<>", "[]{}~`^|€£"],
};

/**
 * In Spanish, ñ takes the apostrophe's place among the letters, and the symbols page opens with
 * the accented letters and the opening marks (the digits are on the letters page already).
 */
const SPANISH: Record<Page, string[]> = {
  letters: ["1234567890", "qwertyuiop", "asdfghjklñ", "zxcvbnm,.-"],
  symbols: ["áéíóúüñ¿¡'", "!@#$%&*()_", "+=/\\:;\"?<>", "[]{}~`^|€£"],
};

const rowsOf = (lang: Language): Record<Page, string[]> => (lang.startsWith("es") ? SPANISH : ROWS);

type Act = { type: "char"; char: string } | { type: "space" | "delete" | "page" | "done" };

interface KeyDef {
  /** Where the key sits: its row and first column. The same place keeps its key across pages. */
  key: string;
  label: string;
  /** Its first column, and how many of the ten it spans. */
  col: number;
  span: number;
  act: Act;
}

const covers = (def: KeyDef, col: number) => col >= def.col && col < def.col + def.span;

function keysOf(page: Page, lang: Language): KeyDef[][] {
  const rows = rowsOf(lang)[page];
  const chars = rows.map((row, r) =>
    [...row].map((char, c) => ({ key: `key:${r}:${c}`, label: char, col: c, span: 1, act: { type: "char", char } as Act })),
  );
  const r = rows.length;
  return [
    ...chars,
    [
      { key: `key:${r}:0`, label: page === "letters" ? t("keyboard.symbols") : t("keyboard.letters"), col: 0, span: 2, act: { type: "page" } },
      { key: `key:${r}:2`, label: t("keyboard.space"), col: 2, span: 4, act: { type: "space" } },
      { key: `key:${r}:6`, label: t("keyboard.delete"), col: 6, span: 2, act: { type: "delete" } },
      { key: `key:${r}:8`, label: t("common.done"), col: 8, span: 2, act: { type: "done" } },
    ],
  ];
}

/**
 * Where an arrow goes from `from` on `rows`: along the row, or to the key of the next row up or
 * down that covers `col` (the column the owner is keeping to). At an edge it stays: undefined
 * only for a key the grid does not hold.
 */
export function gridMove(rows: KeyDef[][], from: string, key: string, col: number): KeyDef | undefined {
  const r = rows.findIndex((row) => row.some((k) => k.key === from));
  if (r < 0) return undefined;
  const row = rows[r]!;
  const i = row.findIndex((k) => k.key === from);
  const step = (to: KeyDef | undefined) => to ?? row[i];
  switch (key) {
    case "ArrowLeft":
      return step(row[i - 1]);
    case "ArrowRight":
      return step(row[i + 1]);
    case "ArrowUp":
      return step(rows[r - 1]?.find((k) => covers(k, col)));
    case "ArrowDown":
      return step(rows[r + 1]?.find((k) => covers(k, col)));
    case "Home":
      return row[0];
    case "End":
      return row.at(-1);
    default:
      return undefined;
  }
}

/** Where focus lands as the keyboard opens: the first letter. */
const FIRST = "key:1:0";

const keyboardHints = (): Hint[] => [
  { action: "select", label: t("keyboard.type") },
  { action: "menu", label: t("keyboard.delete") },
  { action: "ask", label: t("keyboard.space") },
  { action: "done", label: t("common.done") },
  { action: "back", label: t("keyboard.close") },
];

function Key({ def, onPress, onFocused }: { def: KeyDef; onPress: (act: Act) => void; onFocused: (def: KeyDef) => void }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: def.key,
    onSelect: () => onPress(def.act),
  });
  const word = def.act.type !== "char";
  return (
    <motion.button
      ref={ref}
      {...props}
      onFocus={(e) => {
        props.onFocus(e);
        onFocused(def);
      }}
      type="button"
      style={{ gridColumn: `span ${def.span}` }}
      initial={false}
      animate={{ scale: focused ? numberToken("--work-focus-scale") : 1 }}
      transition={focusSpring()}
      className={cn(
        "relative flex h-(--key-height) items-center justify-center rounded-control outline-none",
        "font-display transition-colors",
        word ? "text-lead font-ui" : "text-title font-title",
        "bg-wall-raised",
        focused ? "z-10 text-label" : "text-label-dim",
      )}
    >
      {focused && (
        <motion.span layoutId="key-light" aria-hidden transition={focusSpring()} className="key-light pointer-events-none absolute inset-0 rounded-control" />
      )}
      <span className="relative">{def.label}</span>
    </motion.button>
  );
}

export interface KeyboardProps {
  open: boolean;
  /** What the field is for (its label or placeholder), set over the text. */
  label: string;
  value: string;
  onChange: (value: string) => void;
  /** Done, or the pad's Menu button: the owner has finished typing. */
  onDone: () => void;
  /** B: the keyboard closes, the text as typed. */
  onClose: () => void;
  /** Shows dots in place of the text. */
  secret?: boolean;
}

export function Keyboard({ open, label, value, onChange, onDone, onClose, secret = false }: KeyboardProps) {
  const t = useT();
  const [page, setPage] = useState<Page>("letters");
  const rows = keysOf(page, language());
  // The column Up and Down keep to: where the owner last moved along a row.
  const column = useRef(0);
  const onFocused = (def: KeyDef) => {
    if (!covers(def, column.current)) column.current = def.col;
  };
  const move: GroupMove = (event, { from }) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return undefined;
    const to = gridMove(rows, from.key, event.key, column.current);
    if (to !== undefined && (event.key === "ArrowLeft" || event.key === "ArrowRight" || event.key === "Home" || event.key === "End")) {
      column.current = to.col;
    }
    return to?.key;
  };
  // Presses can come faster than renders (a held button); each edits the latest text.
  const text = useRef(value);
  useLayoutEffect(() => {
    text.current = value;
  });
  const edit = (next: string) => {
    text.current = next;
    onChange(next);
  };
  const press = (act: Act) => {
    switch (act.type) {
      case "char":
        return edit(text.current + act.char);
      case "space":
        return edit(text.current + " ");
      case "delete":
        return edit(text.current.slice(0, -1));
      case "page":
        return setPage((p) => (p === "letters" ? "symbols" : "letters"));
      case "done":
        return onDone();
    }
  };
  const shown = secret ? "•".repeat(value.length) : value;

  return (
    <FocusScope
      id="keyboard"
      modal
      open={open}
      label={t("keyboard.label", { label })}
      restoreKey={FIRST}
      onBack={onClose}
      onMenu={() => press({ type: "delete" })}
      onAsk={() => press({ type: "space" })}
      onDone={onDone}
      ownHints
      className={cn(
        "fixed inset-x-0 bottom-0 z-50 flex flex-col items-center gap-7 border-t border-line bg-wall px-edge pt-10 pb-8 outline-none",
        "transition-[opacity,translate] duration-(--dur-screen) ease-out",
        "data-starting-style:translate-y-(--shift-screen) data-starting-style:opacity-0",
        "data-ending-style:translate-y-(--shift-screen) data-ending-style:opacity-0",
        "motion-reduce:translate-y-0",
      )}
    >
      <div className="flex w-(--keyboard-width) flex-col gap-1">
        <p className="font-display text-body text-label-faint">{label}</p>
        {/* The end of a long text stays in view; the start runs off the left. */}
        <p
          data-keyboard-text
          className="flex h-[1.2em] items-center justify-end overflow-hidden border-b border-line pb-1 font-display text-title font-title tracking-title whitespace-pre text-label"
        >
          <span className="min-w-0 shrink-0">{shown}</span>
          <span aria-hidden className="ml-1 h-[0.9em] w-0.5 shrink-0 bg-lamp" />
          <span className="flex-1" />
        </p>
      </div>
      <FocusGroup
        id="keyboard-keys"
        orientation="horizontal"
        move={move}
        aria-label={t("keyboard.keys")}
        className="grid w-(--keyboard-width) grid-cols-10 gap-(--key-gap)"
      >
        {rows.flat().map((def) => (
          <Key key={def.key} def={def} onPress={press} onFocused={onFocused} />
        ))}
      </FocusGroup>
      <HintLine hints={keyboardHints()} className="self-end" />
    </FocusScope>
  );
}
