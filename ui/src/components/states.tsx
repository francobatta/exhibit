// What a screen shows when it has nothing, is still loading or failed.
// Plain words, sentence case, never apologising; still, nothing loops.

import { useId, type ReactNode } from "react";
import { Lamp } from "@/components/Lamp";
import { Button } from "@/components/ui/button";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";

export interface StateAction {
  label: string;
  onAction: () => unknown;
}

// Its key stays the same in every language, so focus finds it again after the words change.
function ActionButton({ action }: { action: StateAction }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: "state:action",
    onSelect: () => void action.onAction(),
  });
  return (
    <Button ref={ref} {...props} variant="line" className="relative focus:border-lamp">
      {action.label}
      {focused && <Lamp />}
    </Button>
  );
}

/** A state's one action, as a focus item so the pad reaches it. */
function Action({ action }: { action: StateAction }) {
  const id = useId();
  return (
    <FocusGroup id={`state-${id}`} orientation="horizontal" className="mt-2">
      <ActionButton action={action} />
    </FocusGroup>
  );
}

function Panel({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex max-w-measure flex-col items-start gap-4 px-edge py-16", className)}>
      {children}
    </div>
  );
}

const heading = "font-display text-heading font-title tracking-title text-label";
const reading = "font-reading text-lead leading-reading text-label-dim";

/** Nothing here yet: what this place holds, and how to fill it. */
export function Empty({
  title,
  body,
  status,
  action,
}: {
  title: string;
  body?: string;
  /** A quiet line on what is happening now, such as the library's job. */
  status?: string;
  action?: StateAction;
}) {
  return (
    <Panel>
      <h2 className={heading}>{title}</h2>
      {body && <p className={reading}>{body}</p>}
      {status && (
        <p role="status" className="font-display text-body text-label-faint">
          {status}
        </p>
      )}
      {action && <Action action={action} />}
    </Panel>
  );
}

/** A single dim line while data arrives. No spinner, no animation. */
export function Loading({ label }: { label?: string }) {
  const t = useT();
  return (
    <Panel>
      <p role="status" className="font-display text-body text-label-faint">
        {label ?? t("common.loading")}
      </p>
    </Panel>
  );
}

/** What happened, what to do about it, and one action that does it. */
export function ErrorState({
  message,
  hint,
  action,
}: {
  message: string;
  hint: string;
  action?: StateAction;
}) {
  return (
    <Panel>
      <h2 role="alert" className={heading}>
        {message}
      </h2>
      <p className={reading}>{hint}</p>
      {action && <Action action={action} />}
    </Panel>
  );
}
