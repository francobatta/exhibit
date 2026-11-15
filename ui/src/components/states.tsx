// What a screen shows when it has nothing, is still loading, failed, or its drive is away.
// Plain words, sentence case, never apologising; still, nothing loops.

import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export interface StateAction {
  label: string;
  onAction: () => unknown;
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
  action,
}: {
  title: string;
  body?: string;
  action?: StateAction;
}) {
  return (
    <Panel>
      <h2 className={heading}>{title}</h2>
      {body && <p className={reading}>{body}</p>}
      {action && (
        <Button variant="line" className="mt-2" onClick={action.onAction}>
          {action.label}
        </Button>
      )}
    </Panel>
  );
}

/** A single dim line while data arrives. No spinner, no animation. */
export function Loading({ label = "Loading" }: { label?: string }) {
  return (
    <Panel>
      <p role="status" className="font-display text-body text-label-faint">
        {label}
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
      {action && (
        <Button variant="line" className="mt-2" onClick={action.onAction}>
          {action.label}
        </Button>
      )}
    </Panel>
  );
}

/** A quiet line saying a drive is not connected and what still works. */
export function OfflineNote({ drive, className }: { drive: string; className?: string }) {
  return (
    <p className={cn("font-display text-body text-label-dim", className)}>
      {drive} is not connected. Connect it to play what is on it.
    </p>
  );
}
