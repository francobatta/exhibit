// The item menu: `itemActions` for one work, as UI SDK menu parts inside a modal FocusScope, set
// beside the work it was opened from. Opened by X, `m` or right-click on any work (shelves and
// walls call `openItemMenu`); Back (B, Escape) closes it and focus returns to the work. The chosen
// action runs once the menu has closed, so a dialog it opens returns focus to the work too.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { create } from "zustand";
import { Menu, MenuContent, MenuGroup, MenuItem, MenuLabel, MenuSeparator, createMenuController } from "@/components/ui/menu";
import { FocusScope } from "@/focus/react";
import { itemActions, facetsOf, reasonOf, runAction, useActionContext, type Action, type ActionItem } from "@/lib/actions";
import { useQueryClient } from "@tanstack/react-query";
import * as api from "@/lib/api";
import { remPx, rems } from "@/lib/motion";
import { keys, useItemSources } from "@/lib/queries";
import { useNav } from "@/nav/stack";

/** How long the menu waits for the full record before it opens with the summary, in ms. */
const DETAIL_WAIT = 250;

/** Something the screen the menu was opened from can do, listed after the work's own actions. */
export interface MenuExtra {
  id: string;
  label: string;
  run: () => void;
}

interface ItemMenuState {
  item: ActionItem | null;
  anchor: HTMLElement | null;
  extra: MenuExtra[];
  open: boolean;
}

export const useItemMenu = create<ItemMenuState>()(() => ({ item: null, anchor: null, extra: [], open: false }));

/** Opens the item menu for `item`, beside `anchor` (default: what has focus), with the screen's `extra` choices. */
export function openItemMenu(item: ActionItem, anchor?: HTMLElement | null, extra: MenuExtra[] = []): void {
  if (useItemMenu.getState().open) return;
  const at = anchor ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
  useItemMenu.setState({ item, anchor: at, extra, open: true });
}

const EXTRA = "extra:";

/** Whether the menu is open from this element: its work stays lit under the menu. */
export function useMenuOpenOn(el: { current: HTMLElement | null }): boolean {
  return useItemMenu((s) => s.open && s.anchor !== null && s.anchor === el.current);
}

/** Whether the menu is open from the focus item with `key` (keys are unique on a screen). */
export function useMenuOpenOnKey(key: string): boolean {
  return useItemMenu((s) => s.open && s.anchor?.dataset.focusKey === key);
}

export interface ItemMenuProps {
  item: ActionItem;
  actions: Action[];
  /** The screen's own choices, after a hairline. */
  extra?: MenuExtra[];
  anchor: HTMLElement | null;
  open: boolean;
  /** The menu wants to close (Back, a press outside, or a choice). */
  onClose: () => void;
  onChoose: (choice: Action | MenuExtra) => void;
  /** Finished closing, focus returned. */
  onClosed?: () => void;
}

export function ItemMenu({ item, actions, extra = [], anchor, open, onClose, onChoose, onClosed }: ItemMenuProps) {
  const [menu] = useState(() => createMenuController());
  const latest = useRef({ onClose, actions, extra, onChoose });
  useLayoutEffect(() => {
    latest.current = { onClose, actions, extra, onChoose };
  });

  // Opened from code, the menu puts focus on its first item (rendered open, it would not). The
  // dialog around it mounts closed and opens on the next commit; the menu opens a frame later,
  // once the dialog is shown, since before that its items cannot take focus.
  const [dialogOpen, setDialogOpen] = useState(false);
  useEffect(() => {
    setDialogOpen(open);
    if (!open) menu.close();
  }, [open, menu]);
  useEffect(() => {
    if (!open || !dialogOpen) return;
    const frame = requestAnimationFrame(() => {
      void menu.open({ anchor: anchor ?? undefined }).then(() => latest.current.onClose());
    });
    return () => cancelAnimationFrame(frame);
  }, [open, dialogOpen, anchor, menu]);

  const title = facetsOf(item).title;
  return (
    <FocusScope id="item-menu" modal open={dialogOpen} label={`${title}, actions`} onBack={() => menu.close()} onClosed={onClosed} className="fixed inset-0 bg-transparent outline-none">
      <Menu
        controller={menu}
        loop={false}
        onAction={(value) => {
          const { actions, extra, onChoose } = latest.current;
          const choice = value.startsWith(EXTRA)
            ? extra.find((x) => EXTRA + x.id === value)
            : actions.find((x) => x.id === value);
          if (choice !== undefined) onChoose(choice);
        }}
      >
        <MenuContent
          side="right"
          align="start"
          sideOffset={rems("--space-work-gap") * remPx()}
          collisionPadding={rems("--space-work-gap") * remPx()}
          finalFocus="none"
        >
          <MenuGroup>
            <MenuLabel>{title}</MenuLabel>
            {actions.map((x) => (
              <MenuItem key={x.id} value={x.id} disabled={x.disabled !== undefined} hint={x.disabled}>
                {x.label}
              </MenuItem>
            ))}
          </MenuGroup>
          {extra.length > 0 && <MenuSeparator />}
          {extra.map((x) => (
            <MenuItem key={x.id} value={EXTRA + x.id}>
              {x.label}
            </MenuItem>
          ))}
        </MenuContent>
      </Menu>
    </FocusScope>
  );
}

/** Mount once near the app's root: the one item menu, for every work. */
export function ItemMenuHost({ fix }: { fix?: (item: ActionItem) => void }) {
  const { item, anchor, extra, open } = useItemMenu();
  const ctx = useActionContext(fix);
  const client = useQueryClient();
  const chosen = useRef<Action | MenuExtra | null>(null);
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

  // A play that cannot run names the drive it waits for, once the full record says.
  const drive = useItemSources(subject !== null && "item" in subject ? subject : undefined).find((s) => !s.online)?.path;
  // Details of the work whose own screen this is would open the same screen again.
  const here = useNav((s) => s.current);
  if (item === null || subject === null) return null;
  const id = facetsOf(subject).id;
  const onItsScreen = (here.screen === "show" || here.screen === "details") && here.id === id;
  const actions = itemActions(subject)
    .filter((a) => !(a.id === "details" && onItsScreen))
    .map((a) => ({ ...a, disabled: reasonOf(a, drive) }));
  return (
    <ItemMenu
      key={id}
      item={subject}
      actions={actions}
      extra={extra}
      anchor={anchor}
      open={open}
      onClose={() => useItemMenu.setState({ open: false })}
      onChoose={(choice) => {
        chosen.current = choice;
      }}
      onClosed={() => {
        const choice = chosen.current;
        chosen.current = null;
        useItemMenu.setState({ item: null, anchor: null, extra: [] });
        setSubject(null);
        if (choice === null) return;
        if ("run" in choice) choice.run();
        else void runAction(choice, subject, ctx);
      }}
    />
  );
}
