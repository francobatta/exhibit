// A dialog: the owned UI SDK dialog parts inside a modal FocusScope, so focus is trapped, Back
// (B, Escape) closes it and focus returns to what opened it. Its buttons are focus items, reached
// by the pad like everything else. `confirm()` asks a yes-or-no question from anywhere.

import { useState, type ReactNode } from "react";
import { create } from "zustand";
import { Lamp } from "@/components/Lamp";
import { Button } from "@/components/ui/button";
import { DialogDescription, DialogTitle, dialogBackdrop, dialogPanel } from "@/components/ui/dialog";
import { FocusGroup, FocusScope, useFocusItem } from "@/focus/react";
import { cn } from "@/lib/utils";
import { useT } from "@/i18n";

export interface DialogAction {
  label: string;
  onSelect: () => void;
  /** The action that goes ahead: drawn solid. */
  primary?: boolean;
}

export interface DialogProps {
  id: string;
  open: boolean;
  title: string;
  body?: ReactNode;
  /** Buttons along the bottom, left to right. */
  actions: DialogAction[];
  /** Back: B, Escape, Backspace or a press outside. */
  onBack: () => void;
  /** Finished closing, with focus returned. */
  onClosed?: () => void;
  /** The button focused when it opens. Default the first. */
  initial?: number;
  /** An item among `children` to focus when it opens, instead of a button. */
  restoreKey?: string;
  /** Classes for the panel, such as a wider measure. */
  className?: string;
  children?: ReactNode;
}

function DialogButton({ id, index, action }: { id: string; index: number; action: DialogAction }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: `${id}:${index}`,
    onSelect: action.onSelect,
  });
  return (
    <Button
      ref={ref}
      {...props}
      variant={action.primary ? "solid" : "line"}
      className="relative focus:border-lamp"
    >
      {action.label}
      {focused && <Lamp />}
    </Button>
  );
}

export function Dialog({ id, open, title, body, actions, onBack, onClosed, initial = 0, restoreKey, className, children }: DialogProps) {
  return (
    <FocusScope
      id={id}
      modal
      open={open}
      label={title}
      onBack={onBack}
      onClosed={onClosed}
      restoreKey={restoreKey ?? `${id}:${initial}`}
      className={cn(dialogPanel, className)}
      backdropClassName={dialogBackdrop}
    >
      <DialogTitle>{title}</DialogTitle>
      {body !== undefined && <DialogDescription>{body}</DialogDescription>}
      {children}
      <FocusGroup id={`${id}-actions`} orientation="horizontal" className="mt-2 flex gap-4">
        {actions.map((a, i) => (
          <DialogButton key={i} id={id} index={i} action={a} />
        ))}
      </FocusGroup>
    </FocusScope>
  );
}

/** A yes-or-no question before something that moves a work. */
export interface ConfirmRequest {
  title: string;
  body: string;
  /** The label of the button that goes ahead. */
  confirm: string;
}

interface ConfirmState {
  request: ConfirmRequest | null;
  open: boolean;
  resolve: ((go: boolean) => void) | null;
}

const useConfirm = create<ConfirmState>()(() => ({ request: null, open: false, resolve: null }));

/** Asks `request.title`; resolves true when the owner chooses `request.confirm`, false on Cancel or Back. */
export function confirm(request: ConfirmRequest): Promise<boolean> {
  useConfirm.getState().resolve?.(false);
  return new Promise((resolve) => useConfirm.setState({ request, open: true, resolve }));
}

/** Mount once near the app's root, for `confirm()`. */
export function ConfirmHost() {
  const t = useT();
  const { request, open, resolve } = useConfirm();
  // Each question gets a fresh scope, so a second confirm never reuses a closing one.
  const [serial, setSerial] = useState(0);
  if (request === null) return null;
  const answer = (go: boolean) => {
    resolve?.(go);
    useConfirm.setState({ open: false, resolve: null });
  };
  return (
    <Dialog
      key={serial}
      id="confirm"
      open={open}
      title={request.title}
      body={request.body}
      // Cancel first and focused: going ahead takes a deliberate move.
      actions={[
        { label: t("common.cancel"), onSelect: () => answer(false) },
        { label: request.confirm, onSelect: () => answer(true), primary: true },
      ]}
      onBack={() => answer(false)}
      onClosed={() => {
        if (!useConfirm.getState().open) {
          useConfirm.setState({ request: null });
          setSerial((n) => n + 1);
        }
      }}
    />
  );
}
