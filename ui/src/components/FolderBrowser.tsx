// Choosing a folder (or a tool's program) with the pad: a native file dialog cannot be driven by
// a gamepad, so this dialog walks the disks through `listFolders`. Drives first, then a folder's
// folders (and, choosing a program, its `.exe` files after them). A opens a folder or picks a
// program; "Add this folder" picks the folder being shown; B goes up a level and closes at the
// drives. A path typed into the field is picked as it is.

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { focusItem } from "@/focus/bridge";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { listFolders, plain, type Folder } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Dialog, type DialogAction } from "./Dialog";
import { Lamp } from "./Lamp";
import { TextField } from "./TextField";

export interface FolderBrowserProps {
  open: boolean;
  /** `folder` picks a folder; `exe` picks a program file (a tool's `.exe`). */
  mode: "folder" | "exe";
  title: string;
  /** What the path field asks for, such as "Or type the path to mpv.exe". */
  placeholder: string;
  onPick: (path: string) => void;
  onClose: () => void;
  /** Finished closing, focus returned. */
  onClosed?: () => void;
}

/** The folder above `path`: `C:\Films` from `C:\Films\New`, `C:\` from `C:\Films`; null above a drive. */
export function parentOf(path: string): string | null {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  if (trimmed === "" || cut < 0) return null;
  const head = trimmed.slice(0, cut);
  if (/^[A-Za-z]:$/.test(head)) return `${head}\\`;
  return head === "" ? "/" : head;
}

const entryKey = (i: number) => `browser:${i}`;

export function FolderBrowser({ open, mode, title, placeholder, onPick, onClose, onClosed }: FolderBrowserProps) {
  // Null is the list of drives.
  const [path, setPath] = useState<string | null>(null);
  const [typed, setTyped] = useState("");
  // Which entry takes focus once the folder shown has loaded: the first, or the one just left.
  const land = useRef<string | null>(null);
  const ext = mode === "exe" ? "exe" : null;
  const listing = useQuery({
    queryKey: ["folders", path, ext],
    queryFn: () => listFolders(path, ext),
    enabled: open,
    staleTime: 0,
    retry: false,
    // The last folder's entries stay until the next one's arrive, so focus has somewhere to be.
    placeholderData: keepPreviousData,
  });
  const entries = listing.isPlaceholderData ? [] : (listing.data ?? []);
  const loading = listing.isPending || listing.isPlaceholderData;

  useEffect(() => {
    if (loading || listing.isError) return;
    const at = Math.max(0, entries.findIndex((e) => e.path === land.current));
    land.current = null;
    if (entries.length > 0) focusItem(entryKey(at));
    // Once per folder shown: `entries` is derived from `listing.data`.
  }, [listing.data, loading, listing.isError]);

  const go = (to: string | null, from: string | null = null) => {
    land.current = from;
    setPath(to);
  };
  const up = () => (path === null ? onClose() : go(parentOf(path), path));
  const choose = (entry: Folder) => (entry.file ? onPick(entry.path) : go(entry.path));
  const pickTyped = (text: string) => {
    if (text.trim() !== "") onPick(text.trim());
  };

  const actions: DialogAction[] = [
    ...(mode === "folder" && path !== null ? [{ label: "Add this folder", onSelect: () => onPick(path), primary: true }] : []),
    { label: "Cancel", onSelect: onClose },
  ];

  return (
    <Dialog
      id="browser"
      open={open}
      title={title}
      body={path ?? "Drives"}
      actions={actions}
      onBack={up}
      onClosed={onClosed}
      restoreKey={entryKey(0)}
      className="w-[min(var(--dialog-browser),calc(100vw-4rem))]"
    >
      <div className="-mx-(--dialog-pad) h-(--dialog-list) overflow-y-auto border-y border-line px-(--dialog-pad) py-3">
        {listing.isError ? (
          <Note>{plain(listing.error)}. Go back and choose another folder.</Note>
        ) : loading ? (
          <Note>Opening {path ?? "the drives"}</Note>
        ) : entries.length === 0 ? (
          <Note>{mode === "exe" ? "No folders or programs here." : "No folders here."}</Note>
        ) : (
          <FocusGroup id="browser-list" orientation="vertical" aria-label={path ?? "Drives"} className="flex flex-col">
            {entries.map((e, i) => (
              <Entry key={e.path} index={i} entry={e} onChoose={() => choose(e)} />
            ))}
          </FocusGroup>
        )}
      </div>
      <FocusGroup id="browser-path" orientation="horizontal" className="flex">
        <TextField focusKey="browser:path" value={typed} onChange={setTyped} onSubmit={pickTyped} placeholder={placeholder} />
      </FocusGroup>
    </Dialog>
  );
}

function Note({ children }: { children: string | string[] }) {
  return <p className="py-2 font-reading text-lead leading-reading text-label-dim">{children}</p>;
}

function Entry({ index, entry, onChoose }: { index: number; entry: Folder; onChoose: () => void }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: entryKey(index),
    onSelect: onChoose,
    hint: entry.file ? "Choose" : "Open",
  });
  return (
    <button
      ref={ref}
      {...props}
      type="button"
      className={cn(
        "flex w-full items-baseline gap-4 py-2 text-left font-display text-lead outline-none transition-colors",
        "text-label-dim hover:text-label focus:text-label",
      )}
    >
      <span className="relative min-w-0 truncate pb-1.5">
        {entry.name}
        {focused && <Lamp />}
      </span>
      {entry.file && <span className="shrink-0 font-display text-body text-label-faint">Program</span>}
    </button>
  );
}

/** What to browse for, and what to do with the path picked. */
export type BrowseRequest = Pick<FolderBrowserProps, "mode" | "title" | "placeholder" | "onPick">;

/**
 * A FolderBrowser for a screen: render `browser`, call `browse` to open it. It closes as a path is
 * picked (then `onPick` runs) and leaves the page once closed, so the next one starts at the drives.
 */
export function useBrowse(): { browser: ReactNode; browse: (request: BrowseRequest) => void } {
  const [request, setRequest] = useState<BrowseRequest | null>(null);
  const [open, setOpen] = useState(false);
  const [serial, setSerial] = useState(0);
  const browse = (r: BrowseRequest) => {
    setRequest(r);
    setSerial((n) => n + 1);
    setOpen(true);
  };
  const browser = request && (
    <FolderBrowser
      key={serial}
      {...request}
      open={open}
      onPick={(path) => {
        setOpen(false);
        request.onPick(path);
      }}
      onClose={() => setOpen(false)}
      onClosed={() => setRequest(null)}
    />
  );
  return { browser, browse };
}
