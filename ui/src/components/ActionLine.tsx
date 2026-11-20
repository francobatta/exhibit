// A placard's last line: what A does on the work, as a quiet pill. The pointer can press it; the
// keys and the pad run the same action on the work that has focus, so the pill names no key and
// never takes focus. A started work shows how far it has got, as a rule and the time left, beside
// a pill that says Resume. An action that cannot run says why instead ("the drive for D:\Movies is
// not connected"). Beside a wall the heart and the eye follow the pill, pressable the same way.

import type { MouseEvent, ReactNode } from "react";
import { useT } from "@/i18n";
import { reasonOf, type Action } from "@/lib/actions";
import { cn } from "@/lib/utils";
import { ToggleIcon } from "@ui-sdk/react/toggle";
import { LovedIcon, WatchedIcon } from "./ui/state-icon";
import { Toggle } from "./ui/toggle";

/** How far a started work has got: a share of it, and the time left in words. */
export interface Progress {
  done: number;
  left: string;
}

/** Why the main action cannot run, as a sentence of its own. */
function whyNot(action: Pick<Action, "disabled" | "offline">, drive: string | undefined): string {
  return reasonOf(action, drive) ?? "";
}

/** The play glyph: a triangle in the text colour. */
export function PlayGlyph() {
  return (
    <svg aria-hidden viewBox="0 0 10 12" className="h-(--cue-glyph) w-auto shrink-0 fill-current">
      <path d="M0 0v12l10-6z" />
    </svg>
  );
}

export interface ActionLineProps {
  action: Pick<Action, "label" | "disabled" | "offline"> & { id?: Action["id"] };
  drive: string | undefined;
  /** Whether A would run it now. A pill under a work that does not have focus is quiet. Default true. */
  active?: boolean;
  /** How far the work has got, when it has been started. */
  progress?: Progress | null;
  /** Runs the action from the pointer. Without it the pill is a picture of the action. */
  onPress?: () => void;
  /** Loved and Watched, as the heart and the eye of Home's and Details' cues, after the pill. */
  states?: States;
}

/** A work's states beside its action; the pointer changes them, as it presses the pill. */
export interface States {
  loved: boolean;
  watched: boolean;
  onLoved: (loved: boolean) => void;
  onWatched: (watched: boolean) => void;
}

/** The pointer presses a cue without taking focus from the work it acts on. */
const keepFocus = (e: MouseEvent) => e.preventDefault();

function StateToggle({ label, icon, pressed, onChange }: { label: string; icon: ReactNode; pressed: boolean; onChange: (on: boolean) => void }) {
  return (
    <Toggle
      tabIndex={-1}
      onMouseDown={keepFocus}
      aria-label={label}
      pressed={pressed}
      onPressedChange={onChange}
      variant="cue"
      size="cue-icon"
      className="data-pressed:text-label"
    >
      <ToggleIcon>{icon}</ToggleIcon>
    </Toggle>
  );
}

export function ActionLine({ action, drive, active = true, progress = null, onPress, states }: ActionLineProps) {
  const t = useT();
  const disabled = Boolean(action.disabled);
  const plays = action.id === "play" || action.id === "resume" || action.id === "restart";
  // The rule says where it stops; the pill needs no clock.
  const label = action.id === "resume" ? t("actions.resume") : action.label;
  const pill = cn(
    "inline-flex h-(--cue-height) shrink-0 items-center gap-3 rounded-full px-(--cue-pad) font-display text-lead font-ui",
    "ring-1 ring-line ring-inset",
    disabled ? "text-label-faint" : active ? "text-label" : "text-label-dim",
    onPress && !disabled && "cursor-pointer hover:bg-(--cue-lit) hover:ring-lamp",
  );
  const content = (
    <>
      {plays && <PlayGlyph />}
      {label}
    </>
  );
  return (
    <div data-line="action" className="flex min-h-(--cue-height) items-center gap-5">
      <div className="flex shrink-0 items-center gap-3">
        {onPress ? (
          <button
            type="button"
            tabIndex={-1}
            disabled={disabled}
            onMouseDown={keepFocus}
            onClick={onPress}
            className={pill}
          >
            {content}
          </button>
        ) : (
          <span className={pill}>{content}</span>
        )}
        {states && (
          <>
            <StateToggle
              label={t("cues.loved")}
              icon={<LovedIcon className="size-(--cue-icon)" />}
              pressed={states.loved}
              onChange={states.onLoved}
            />
            <StateToggle
              label={t("cues.watched")}
              icon={<WatchedIcon className="size-(--cue-icon)" />}
              pressed={states.watched}
              onChange={states.onWatched}
            />
          </>
        )}
      </div>
      {disabled ? (
        <p className="min-w-0 font-display text-body text-label-dim">{whyNot(action, drive)}</p>
      ) : (
        progress !== null && (
          <div className="flex items-center gap-4 font-display text-body text-label-dim">
            <span
              aria-hidden
              className="relative h-(--cue-rule-weight) w-(--cue-rule) overflow-hidden rounded-full bg-(--cue-rule-track)"
            >
              <span
                className="absolute inset-0 origin-left rounded-full bg-label"
                style={{ transform: `scaleX(${Math.min(1, Math.max(0, progress.done))})` }}
              />
            </span>
            <span>{progress.left}</span>
          </div>
        )
      )}
    </div>
  );
}
