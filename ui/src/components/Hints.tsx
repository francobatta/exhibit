// Bottom right: what the buttons do here, drawn for the device in hand: the pad's buttons, or the
// keys. The mouse sees what it can click and gets no line. Built from the focused item's handlers
// and what the bridge knows (a modal on top, whether Y works there).

import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { focusedHandlers, useFocusState, type InputMode, type ItemHandlers } from "@/focus/bridge";
import { useInputMode } from "@/focus/react";
import { t, useLanguage, useT } from "@/i18n";
import { useStatus } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useNav } from "@/nav/stack";

/** A, Left and Right, X, Y, LB/RB, the Menu button and B; as keys Enter, the arrows, M, /, [ ], none, Esc. */
export type HintAction = "select" | "adjust" | "menu" | "ask" | "sections" | "done" | "back";

export interface Hint {
  action: HintAction;
  label: string;
}

export interface HintContext {
  canBack: boolean;
  canAsk: boolean;
  /** LB and RB change section: outside dialogs and first run. */
  sections: boolean;
  /** What Y is called here: Search, or Ask once an agent is set up. */
  askLabel: string;
  /** Keys are typing into a field: Enter, `/` and `[ ]` belong to it, and only Esc acts. */
  typing?: boolean;
}

/** What each button does from the focused item, in the order the pad shows them. */
export function hints(item: ItemHandlers | null, at: HintContext): Hint[] {
  const out: Hint[] = [];
  if (at.typing) return at.canBack ? [{ action: "back", label: t("hints.back") }] : out;
  // A toggle has no onSelect (it presses itself), but names what A does.
  if ((item?.onSelect || item?.hint) && !item.disabled) out.push({ action: "select", label: item.hint ?? t("hints.select") });
  if (item?.adjust) out.push({ action: "adjust", label: item.adjust });
  if (item?.onMenu) out.push({ action: "menu", label: t("hints.menu") });
  if (at.canAsk) out.push({ action: "ask", label: at.askLabel });
  if (item?.leap) out.push({ action: "sections", label: item.leap });
  else if (at.sections) out.push({ action: "sections", label: t("hints.sections") });
  if (at.canBack) out.push({ action: "back", label: t("hints.back") });
  return out;
}

const PAD: Record<Exclude<HintAction, "sections" | "done" | "adjust">, string> = { select: "A", menu: "X", ask: "Y", back: "B" };

/** A face button: a filled disc with its letter. */
function Disc({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex size-(--hint-glyph) items-center justify-center rounded-full bg-label/85 font-title text-caption leading-none text-wall">
      {children}
    </span>
  );
}

/** A shoulder button: a filled pill. */
function Shoulder({ children }: { children: string }) {
  return (
    <span className="inline-flex h-(--hint-glyph) items-center rounded-full bg-label/85 px-2.5 font-title text-caption leading-none text-wall">
      {children}
    </span>
  );
}

/** A key cap: an outlined key with its name, deeper at the bottom. */
function Cap({ children }: { children: string }) {
  return (
    <span className="inline-flex h-(--hint-glyph) min-w-(--hint-glyph) items-center justify-center rounded-control border border-b-2 border-label-dim px-2 font-title text-caption leading-none text-label">
      {children}
    </span>
  );
}

/** The d-pad pressed left and right: a pill with a triangle each way. */
function DpadSides() {
  return (
    <span className="inline-flex h-(--hint-glyph) items-center gap-1 rounded-full bg-label/85 px-2 text-wall">
      <svg viewBox="0 0 20 10" aria-hidden className="h-[0.7em]" fill="currentColor">
        <path d="M0 5l6-5v10zM20 5l-6 5V0z" />
      </svg>
    </span>
  );
}

/** The pad's Menu button: three short lines in a disc. */
function MenuGlyph() {
  return (
    <Disc>
      <svg viewBox="0 0 12 12" aria-hidden className="size-[0.8em]" fill="none">
        <path d="M2 3h8M2 6h8M2 9h8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
    </Disc>
  );
}

/** The glyphs for `action` on `mode`'s device; null when the device has no such button. */
export function Glyph({ action, mode }: { action: HintAction; mode: Exclude<InputMode, "pointer"> }) {
  if (mode === "pad") {
    if (action === "sections") {
      return (
        <>
          <Shoulder>LB</Shoulder>
          <Shoulder>RB</Shoulder>
        </>
      );
    }
    if (action === "done") return <MenuGlyph />;
    if (action === "adjust") return <DpadSides />;
    return <Disc>{PAD[action]}</Disc>;
  }
  switch (action) {
    case "select":
      return <Cap>{t("keys.enter")}</Cap>;
    case "adjust":
      return (
        <>
          <Cap>←</Cap>
          <Cap>→</Cap>
        </>
      );
    case "menu":
      return <Cap>M</Cap>;
    case "ask":
      return <Cap>/</Cap>;
    case "sections":
      return (
        <>
          <Cap>[</Cap>
          <Cap>]</Cap>
        </>
      );
    case "back":
      return <Cap>{t("keys.esc")}</Cap>;
    case "done":
      return null;
  }
}

/**
 * The hints for the device in hand, each glyph before what it does; nothing for the mouse. With
 * `above`, the line stands in the top layer, raised again whenever `above` changes (a dialog opened),
 * since dialogs are top-layer popovers that no z-index reaches over.
 */
export function HintLine({ hints, above, className }: { hints: Hint[]; above?: string; className?: string }) {
  useT();
  const mode = useInputMode();
  const ref = useRef<HTMLParagraphElement>(null);
  // The keyboard has no Menu button.
  const shown = mode === "pointer" ? [] : mode === "keys" ? hints.filter((h) => h.action !== "done") : hints;
  const visible = shown.length > 0;
  useLayoutEffect(() => {
    const el = ref.current;
    if (el === null || above === undefined) return;
    el.hidePopover();
    el.showPopover();
  }, [above, visible]);
  if (!visible || mode === "pointer") return null;
  return (
    <p
      ref={ref}
      popover={above === undefined ? undefined : "manual"}
      data-hints={mode}
      className={cn("pointer-events-none flex items-center gap-(--hint-gap) font-display text-body text-label-dim", className)}
    >
      {shown.map((h) => (
        <span key={h.action} data-hint={h.action} className="flex items-center gap-2">
          <span className="flex items-center gap-1">
            <Glyph action={h.action} mode={mode} />
          </span>
          {h.label}
        </span>
      ))}
    </p>
  );
}

export function Hints() {
  useT();
  // Subscribed to, then read afresh: a label held since focus came would miss a language change.
  // The focused control takes its new label as it commits, after this renders; one more render then reads it.
  const snapshot = useFocusState((s) => s.item);
  const language = useLanguage();
  const [, reread] = useState(0);
  useEffect(() => reread((n) => n + 1), [language]);
  const item = snapshot === null ? null : (focusedHandlers() ?? snapshot);
  const canBack = useFocusState((s) => s.canBack);
  const canAsk = useFocusState((s) => s.canAsk);
  const modal = useFocusState((s) => s.modal);
  const ownHints = useFocusState((s) => s.ownHints);
  const field = useFocusState((s) => s.field);
  const mode = useInputMode();
  const firstRun = useNav((s) => s.entries[0]!.route.screen === "firstRun");
  const agent = Boolean(useStatus().data?.agent_key);
  if (ownHints) return null;
  const at: HintContext = {
    canBack,
    canAsk,
    sections: modal === null && !firstRun,
    // In Ask, Y goes back to its field.
    askLabel: agent && modal === null ? t("hints.ask") : t("hints.search"),
    // The pad's A opens the on-screen keyboard on a field, so the pad keeps its hints there.
    typing: field && mode === "keys",
  };
  return (
    <HintLine
      hints={hints(item, at)}
      above={modal ?? ""}
      className={cn(
        "fixed inset-auto right-[calc(var(--space-edge)-var(--hint-pad))] bottom-6 m-0 overflow-visible",
        "rounded-full border-0 bg-(--hint-scrim) px-(--hint-pad) py-2",
      )}
    />
  );
}
