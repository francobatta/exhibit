// A placard's row of cues, the same on Home's hero and on Details: the main action as a pill
// (play, resume), Details where it leads somewhere new, then Loved and Watched as a heart and an
// eye that show their state. Commands come first, states after. Beside the row, one line in words
// for the cue the owner is on: why it cannot play, how much is left, what the heart or eye means.

import type { KeyboardEvent, ReactNode } from "react";
import { PlayGlyph, type Progress } from "@/components/ActionLine";
import { openItemMenu } from "@/components/ItemMenu";
import { buttonVariants } from "@/components/ui/button";
import { LovedIcon, WatchedIcon } from "@/components/ui/state-icon";
import { Toggle } from "@/components/ui/toggle";
import { useFocusState, type Dir } from "@/focus/bridge";
import { FocusGroup, useFocusItem } from "@/focus/react";
import {
  facetsOf,
  mainAction,
  reasonOf,
  runAction,
  runSelected,
  useActionContext,
  type Action,
  type ActionItem,
} from "@/lib/actions";
import { cn } from "@/lib/utils";
import { ToggleIcon } from "@ui-sdk/react/toggle";

export interface CuesProps {
  /** The focus group's id; its cues' focus keys are `<prefix>:main`, `:details`, `:love`, `:watched`. */
  id: string;
  prefix: string;
  item: ActionItem;
  /** The drive a play waits for, to name it. */
  drive: string | undefined;
  /** How far a started work has got, shown beside a Resume. */
  progress: Progress | null;
  /** Offer Details (Home); on the details screen itself it is where the owner already is. */
  details?: boolean;
  /** A cue took focus, by its key. */
  onFocusCue?: (key: string) => void;
  onEdge?: (dir: Dir) => void;
  /** The screen opens on something else (Home's shelves); see FocusGroup `aside`. */
  aside?: boolean;
}

/**
 * Left and Right stop at the row's ends. The nearest-item search would otherwise go on to a shelf
 * or a portrait lying below, within its reach to the side.
 */
function stayInRow(e: KeyboardEvent<HTMLDivElement>): void {
  if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
  const cues = [...e.currentTarget.querySelectorAll<HTMLElement>("[data-focus-key]")];
  const end = e.key === "ArrowLeft" ? cues[0] : cues.at(-1);
  if (e.target === end) {
    e.preventDefault();
    e.stopPropagation();
  }
}

/** What the row says beside it, in words. */
function Said({ children }: { children: ReactNode }) {
  return <p className="cues-said min-w-0 font-display text-body text-label-dim">{children}</p>;
}

export function Cues({ id, prefix, item, drive, progress, details = false, onFocusCue, onEdge, aside = false }: CuesProps) {
  const ctx = useActionContext();
  const f = facetsOf(item);
  const main = mainAction(item);
  const container = f.kind === "show" || f.kind === "season";
  const watched = container ? f.seen === "all" : f.watched;
  const focusedKey = useFocusState((s) => s.key);
  const on = focusedKey?.startsWith(`${prefix}:`) ? focusedKey.slice(prefix.length + 1) : "main";
  const menu = (el: HTMLElement) => openItemMenu(item, el);
  const run = (id: Action["id"]) => void runAction({ id, label: "" }, item, ctx);

  let said: ReactNode = null;
  if (on === "love") said = <Said>{f.loved ? "Loved" : "Love"}</Said>;
  else if (on === "watched") said = <Said>{watched ? "Watched" : container ? "Mark all as watched" : "Mark as watched"}</Said>;
  else if (on === "details") said = null;
  else if (main.disabled) said = <Said>{reasonOf(main, drive)}</Said>;
  else if (progress !== null && main.id === "resume") {
    said = (
      <div className="cues-said flex items-center gap-4 font-display text-body text-label-dim">
        <span aria-hidden className="relative h-(--cue-rule-weight) w-(--cue-rule) overflow-hidden rounded-full bg-(--cue-rule-track)">
          <span
            className="absolute inset-0 origin-left rounded-full bg-label"
            style={{ transform: `scaleX(${Math.min(1, Math.max(0, progress.done))})` }}
          />
        </span>
        <span>{progress.left}</span>
      </div>
    );
  }

  return (
    // The group spans the placard so it is what lies below the bar from anywhere over it; what the
    // row says sits inside it, after the cues.
    <div data-line="action" className="min-h-(--cue-height)" onKeyDownCapture={stayInRow}>
      <FocusGroup
        id={id}
        orientation="horizontal"
        reveals
        aria-label="Actions"
        onFocus={(e) => {
          const key = (e.target as HTMLElement).dataset.focusKey;
          if (key !== undefined) onFocusCue?.(key);
        }}
        // Arriving from elsewhere always lands on the main action, never on a heart or an eye that
        // the next A would change unseen; the pointer still goes where it points.
        entry={`${prefix}:main`}
        onEdge={onEdge}
        aside={aside}
        className="flex w-full items-center gap-3"
      >
        <PillCue
          focusKey={`${prefix}:main`}
          label={main.id === "resume" ? "Resume" : main.label}
          glyph={main.id === "play" || main.id === "resume" || main.id === "restart"}
          strong
          disabled={main.disabled !== undefined}
          onRun={() => runSelected(main, item, ctx)}
          onMenu={menu}
        />
        {details && main.id !== "details" && (
          <PillCue focusKey={`${prefix}:details`} label="Details" onRun={() => run("details")} onMenu={menu} />
        )}
        <StateCue
          focusKey={`${prefix}:love`}
          label="Loved"
          icon={<LovedIcon className="size-(--cue-icon)" />}
          hint={f.loved ? "Unlove" : "Love"}
          pressed={f.loved}
          onPressedChange={(p) => run(p ? "love" : "unlove")}
          onMenu={menu}
        />
        <StateCue
          focusKey={`${prefix}:watched`}
          label="Watched"
          icon={<WatchedIcon className="size-(--cue-icon)" />}
          hint={watched ? "Unmark" : "Mark"}
          pressed={watched}
          onPressedChange={(p) => run(p ? "watched" : "unwatched")}
          onMenu={menu}
        />
        <div key={on + String(said !== null)} className="ml-3 min-w-0">
          {said}
        </div>
      </FocusGroup>
    </div>
  );
}

function PillCue({
  focusKey,
  label,
  glyph = false,
  strong = false,
  disabled = false,
  onRun,
  onMenu,
}: {
  focusKey: string;
  label: string;
  glyph?: boolean;
  /** The main action: label white even at rest. */
  strong?: boolean;
  /** It stays focusable, and A says why it cannot run. */
  disabled?: boolean;
  onRun: () => void;
  onMenu: (el: HTMLElement) => void;
}) {
  const { ref, props } = useFocusItem<HTMLButtonElement>({ key: focusKey, onSelect: onRun, onMenu, hint: label });
  return (
    <button
      ref={ref}
      {...props}
      type="button"
      aria-disabled={disabled ? true : undefined}
      className={cn(buttonVariants({ variant: "cue", size: "cue" }), strong && "text-label")}
    >
      {glyph && <PlayGlyph />}
      {label}
    </button>
  );
}

function StateCue({
  focusKey,
  label,
  icon,
  hint,
  pressed,
  onPressedChange,
  onMenu,
}: {
  focusKey: string;
  label: string;
  icon: ReactNode;
  hint: string;
  pressed: boolean;
  onPressedChange: (pressed: boolean) => void;
  onMenu: (el: HTMLElement) => void;
}) {
  const { ref, props } = useFocusItem<HTMLButtonElement>({ key: focusKey, onMenu, hint });
  // The toggle presses itself on Enter and click; the focus item's click would run nothing.
  const { onClick: _click, ...focus } = props;
  return (
    <Toggle
      ref={ref}
      {...focus}
      aria-label={label}
      pressed={pressed}
      onPressedChange={onPressedChange}
      variant="cue"
      size="cue-icon"
      className="data-pressed:text-label"
    >
      <ToggleIcon>{icon}</ToggleIcon>
    </Toggle>
  );
}
