// The item menu: `itemActions` for one work, as UI SDK menu parts inside a modal FocusScope, set
// beside the work it was opened from. Opened by X, `m` or right-click on any work (shelves and
// walls call `openItemMenu`); Back (B, Escape) closes it and focus returns to the work. The chosen
// action runs once the menu has closed, so a dialog it opens returns focus to the work too.

import { Menu, MenuContent, MenuGroup, MenuGroupLabel, MenuItem, createMenuController } from "@ui-sdk/react/menu";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { create } from "zustand";
import { FocusScope } from "@/focus/react";
import { itemActions, facetsOf, runAction, useActionContext, type Action, type ActionItem } from "@/lib/actions";
import { useQueryClient } from "@tanstack/react-query";
import * as api from "@/lib/api";
import { remPx } from "@/lib/motion";
import { keys } from "@/lib/queries";

interface ItemMenuState {
  item: ActionItem | null;
  anchor: HTMLElement | null;
  open: boolean;
}

export const useItemMenu = create<ItemMenuState>()(() => ({ item: null, anchor: null, open: false }));

/** Opens the item menu for `item`, beside `anchor` (default: what has focus). */
export function openItemMenu(item: ActionItem, anchor?: HTMLElement | null): void {
  if (useItemMenu.getState().open) return;
  const at = anchor ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
  useItemMenu.setState({ item, anchor: at, open: true });
}

/** Whether the menu is open from this element: its work stays lit under the menu. */
export function useMenuOpenOn(el: { current: HTMLElement | null }): boolean {
  return useItemMenu((s) => s.open && s.anchor !== null && s.anchor === el.current);
}

export interface ItemMenuProps {
  item: ActionItem;
  actions: Action[];
  anchor: HTMLElement | null;
  open: boolean;
  /** The menu wants to close (Back, a press outside, or a choice). */
  onClose: () => void;
  onChoose: (action: Action) => void;
  /** Finished closing, focus returned. */
  onClosed?: () => void;
}


export function ItemMenu({ item, actions, anchor, open, onClose, onChoose, onClosed }: ItemMenuProps) {
  const [menu] = useState(() => createMenuController());
  const latest = useRef({ onClose, actions, onChoose });
  useLayoutEffect(() => {
    latest.current = { onClose, actions, onChoose };
  });

  // Opened from code, the menu puts focus on its first item (rendered open, it would not). The
  // dialog around it mounts closed and opens on the next commit; the menu opens a frame later,
  // once the dialog is shown, since before that its items cannot take focus.
  const [dialogOpen, setDialogOpen] = useState(false);
  const opened = useRef(false);
  useEffect(() => {
    setDialogOpen(open);
    if (!open) menu.close();
  }, [open, menu]);
  useEffect(() => {
    if (!open || !dialogOpen) return;
    const frame = requestAnimationFrame(() => {
      opened.current = true;
      void menu.open({ anchor: anchor ?? undefined }).then(() => latest.current.onClose());
    });
    return () => cancelAnimationFrame(frame);
  }, [open, dialogOpen, anchor, menu]);

  const title = facetsOf(item).title;
  return (
    <FocusScope id="item-menu" modal open={dialogOpen} label={`${title}, actions`} onBack={() => menu.close()} onClosed={() => {
        // A dialog mounted closed may report closing before the menu ever opened.
        if (opened.current) onClosed?.();
      }} className="fixed inset-0 bg-transparent outline-none">
      <Menu
        controller={menu}
        loop={false}
        onAction={(value) => {
          const action = latest.current.actions.find((a) => a.id === value);
          if (action !== undefined) latest.current.onChoose(action);
        }}
      >
        <MenuContent
          side="right"
          align="start"
          sideOffset={remPx() * 1.75}
          collisionPadding={remPx() * 2}
          finalFocus="none"
          className={[
            "z-50 flex w-[22rem] flex-col rounded-control border border-line bg-wall-raised py-3 outline-none",
            "transition-opacity duration-(--dur-screen) data-starting-style:opacity-0 data-ending-style:opacity-0",
          ].join(" ")}
        >
          <MenuGroup>
            <MenuGroupLabel className="truncate px-6 pt-1 pb-3 font-display text-caption text-label-faint">
              {title}
            </MenuGroupLabel>
            {actions.map((a) => (
              <MenuItem
                key={a.id}
                value={a.id}
                disabled={a.disabled !== undefined}
                className={[
                  "relative flex cursor-default flex-col gap-0.5 px-6 py-2.5 font-display text-lead font-ui text-label-dim outline-none",
                  "data-highlighted:bg-lamp/10 data-highlighted:text-label focus:bg-lamp/10 focus:text-label",
                  "before:absolute before:inset-y-2.5 before:left-0 before:w-0.5 before:rounded-full before:bg-lamp before:opacity-0",
                  "data-highlighted:before:opacity-100 focus:before:opacity-100 data-disabled:text-label-faint",
                ].join(" ")}
              >
                <span>{a.label}</span>
                {a.disabled && <span className="text-caption font-text text-label-faint">{a.disabled}</span>}
              </MenuItem>
            ))}
          </MenuGroup>
        </MenuContent>
      </Menu>
    </FocusScope>
  );
}

/** How long the menu waits for the full record before it opens with the summary. */
const DETAIL_WAIT = 250;

/** Mount once near the app's root: the one item menu, for every work. */
export function ItemMenuHost({ fix }: { fix?: (item: ActionItem) => void }) {
  const { item, anchor, open } = useItemMenu();
  const ctx = useActionContext(fix);
  const client = useQueryClient();
  const chosen = useRef<Action | null>(null);
  // The full record says more ("Resume from 1:04:12", "Play S2 E4"). It is a local read, so the
  // menu waits a moment for it; its actions are then fixed while it is open, never swapped under focus.
  const [subject, setSubject] = useState<ActionItem | null>(null);
  useEffect(() => {
    if (!open || item === null) return;
    if ("item" in item) {
      setSubject(item);
      return;
    }
    let done = false;
    const settle = (s: ActionItem) => {
      if (done) return;
      done = true;
      setSubject(s);
    };
    const timer = setTimeout(() => settle(item), DETAIL_WAIT);
    client
      .fetchQuery({ queryKey: keys.item(item.id), queryFn: () => api.item(item.id) })
      .then((d) => settle(d), () => settle(item));
    return () => {
      done = true;
      clearTimeout(timer);
    };
  }, [open, item, client]);

  if (item === null || subject === null) return null;
  return (
    <ItemMenu
      key={facetsOf(subject).id}
      item={subject}
      actions={itemActions(subject)}
      anchor={anchor}
      open={open}
      onClose={() => useItemMenu.setState({ open: false })}
      onChoose={(action) => {
        chosen.current = action;
      }}
      onClosed={() => {
        const action = chosen.current;
        chosen.current = null;
        useItemMenu.setState({ item: null, anchor: null });
        setSubject(null);
        if (action !== null) void runAction(action, subject, ctx);
      }}
    />
  );
}
