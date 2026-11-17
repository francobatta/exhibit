// Settings: one screen, five parts and nothing else. Folders (each with whether its drive is
// connected, and Remove), tools (where each was found, or what is missing without it), the TMDB
// key, the agent key, and full screen. Opening it detects the tools again. Opened from the mpv
// error or Fix match, it lands on the part that fixes it.

import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { confirm } from "@/components/Dialog";
import { FocusButton } from "@/components/FocusButton";
import { useBrowse } from "@/components/FolderBrowser";
import { Lamp } from "@/components/Lamp";
import { SecretForm } from "@/components/SecretForm";
import { line, ToolLines, useSetToolPath } from "@/components/ToolLines";
import { ErrorState, Loading } from "@/components/states";
import { Switch } from "@/components/ui/switch";
import { toast } from "@/components/ui/toast";
import { focusItem } from "@/focus/bridge";
import { FocusGroup, FocusScope, useFocusItem } from "@/focus/react";
import * as api from "@/lib/api";
import { plain, type LibraryStatus, type Source, type ToolStatus } from "@/lib/api";
import { keys, useFullscreen, useStatus, useTools } from "@/lib/queries";
import type { SettingsPart } from "@/nav/stack";

/** Where focus lands as Settings opens: "Add folder", or the control that fixes what sent the owner here. */
function settingsLanding(part: SettingsPart | undefined): string {
  return part === "tools" ? "tool:mpv:set" : part === "tmdb" ? "tmdb:field" : "folders:add";
}

export function Settings({ part }: { part?: SettingsPart }) {
  const status = useStatus();
  if (status.isPending) return <Loading />;
  if (status.isError) {
    return (
      <ErrorState
        message={plain(status.error)}
        hint="Settings could not be read. Try again, or restart Exhibit if it keeps happening."
        action={{ label: "Try again", onAction: () => status.refetch() }}
      />
    );
  }
  return <Parts status={status.data} part={part} />;
}

function Parts({ status, part }: { status: LibraryStatus; part?: SettingsPart }) {
  // Settings takes focus as it opens, wherever it was opened from (the top bar, a dialog).
  useEffect(() => {
    focusItem(settingsLanding(part));
  }, [part]);
  return (
    <FocusScope id="settings" restoreKey={settingsLanding(part)} fallbackKey="folders:add">
      {/* The parts scroll under the title, clear of the top bar. */}
      <div className="absolute inset-0 flex flex-col pt-bar">
        <h1 className="px-edge pt-4 pb-6 font-display text-title font-title tracking-title text-label">Settings</h1>
        <div className="wall-fade min-h-0 flex-1 overflow-y-auto px-edge pb-edge">
          <Folders sources={status.sources} />
          <Tools initial={status.tools} />
          <TmdbKey status={status} />
          <Part title="Agent key" note="Used from the next version of Exhibit.">
            <SetLine set={status.agent_key} />
            <SecretForm name="agent" label="Agent key" isSet={status.agent_key} />
          </Part>
          <Display />
        </div>
      </div>
    </FocusScope>
  );
}

/** One part: its name and what it is for on the left, its controls on the right. */
function Part({ title, note, children }: { title: string; note?: string; children: ReactNode }) {
  return (
    <section aria-label={title} className="grid grid-cols-[var(--settings-label)_minmax(0,1fr)] gap-x-16 border-t border-line py-10">
      <div className="flex flex-col gap-2">
        <h2 className="font-display text-heading font-title tracking-title text-label">{title}</h2>
        {note && <p className="font-reading text-body leading-reading text-label-dim">{note}</p>}
      </div>
      <div className="flex max-w-(--settings-measure) min-w-0 flex-col gap-6">{children}</div>
    </section>
  );
}

/** A thing the part lists: its name, then lines about it, with its buttons on the right. */
function Row({ name, lines, actions }: { name: string; lines: ReactNode; actions?: ReactNode }) {
  return (
    <div className="flex items-start gap-8">
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <p className="truncate font-display text-lead font-ui text-label">{name}</p>
        {lines}
      </div>
      {actions}
    </div>
  );
}

function SetLine({ set }: { set: boolean }) {
  return <p className={line}>{set ? "Set" : "Not set"}</p>;
}

function Folders({ sources }: { sources: Source[] }) {
  const client = useQueryClient();
  const { browser, browse } = useBrowse();
  const changed = () => void client.invalidateQueries({ queryKey: keys.library });

  const add = () =>
    browse({
      mode: "folder",
      title: "Add a folder",
      placeholder: "Or type a folder's path",
      onPick: (path) => {
        api.addSource(path).then(changed, (e: unknown) => toast(plain(e)));
      },
    });
  const remove = async (source: Source) => {
    const go = await confirm({
      title: "Remove this folder from Exhibit?",
      body: "Your files are not touched.",
      confirm: "Remove",
    });
    if (go) await api.removeSource(source.id).then(changed, (e: unknown) => toast(plain(e)));
  };

  return (
    <Part title="Folders" note="Where your films, shows and other videos are. Exhibit only reads them.">
      {sources.length === 0 && <p className={line}>No folders yet. Add the folders that hold your films and shows.</p>}
      {sources.map((s) => (
        <Row
          key={s.id}
          name={s.path}
          lines={<p className={line}>{s.online ? "Connected" : "Not connected"}</p>}
          actions={
            <FocusGroup id={`source-${s.id}`} orientation="horizontal">
              <FocusButton focusKey={`source:${s.id}:remove`} variant="plain" onSelect={() => void remove(s)}>
                Remove
              </FocusButton>
            </FocusGroup>
          }
        />
      ))}
      <FocusGroup id="folders-actions" orientation="horizontal" className="flex gap-3">
        <FocusButton focusKey="folders:add" onSelect={add}>
          Add folder
        </FocusButton>
        <FocusButton focusKey="folders:scan" onSelect={() => api.refresh(null).catch((e: unknown) => toast(plain(e)))}>
          Scan now
        </FocusButton>
      </FocusGroup>
      {browser}
    </Part>
  );
}

function Tools({ initial }: { initial: ToolStatus[] }) {
  // Detected again as Settings opens; what the library knew is shown meanwhile.
  const tools = useTools().data ?? initial;
  const { browser, choose, set } = useSetToolPath();
  return (
    <Part title="Tools" note="Free programs Exhibit uses to play your files and read them.">
      {tools.map((t) => (
        <Row
          key={t.tool}
          name={t.tool}
          lines={<ToolLines tool={t} />}
          actions={
            <FocusGroup id={`tool-${t.tool}`} orientation="horizontal" className="flex gap-3">
              {t.manual && (
                <FocusButton focusKey={`tool:${t.tool}:detected`} variant="plain" onSelect={() => void set(t.tool, null)}>
                  Use detected
                </FocusButton>
              )}
              <FocusButton focusKey={`tool:${t.tool}:set`} onSelect={() => choose(t.tool)}>
                Set path
              </FocusButton>
            </FocusGroup>
          }
        />
      ))}
      {browser}
    </Part>
  );
}

/**
 * How the identification started by saving a key ended: its error, if it had one. Watched on the
 * `job-status` events one by one, as the job's start and end can arrive within one render.
 */
function useKeyCheck() {
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const stage = useRef<"idle" | "queued" | "running">("idle");
  useEffect(() => {
    const off = api.onJobStatus((s) => {
      if (stage.current === "queued" && s.phase === "identifying") stage.current = "running";
      else if (stage.current === "running" && s.phase === null) {
        stage.current = "idle";
        setChecking(false);
        setError(s.last_error === null ? null : keyError(s.last_error));
      }
    });
    return () => {
      void off.then((f) => f());
    };
  }, []);
  const start = () => {
    stage.current = "queued";
    setChecking(true);
    setError(null);
  };
  return { checking, error, start };
}

/** The identification's failure as Settings says it: the core's message, then what to do here. */
export function keyError(message: string): string {
  const said = plain(message).replace(/\.$/, "");
  return `${said}. Check the key, then save it again.`;
}

function TmdbKey({ status }: { status: LibraryStatus }) {
  const check = useKeyCheck();
  const saved = () => {
    check.start();
    api.identify("unmatched_and_review").catch((e: unknown) => toast(plain(e)));
  };
  return (
    <Part title="TMDB key" note="Names your films and shows and brings their posters. A key is free from themoviedb.org.">
      <div className="flex flex-col gap-1">
        <SetLine set={status.tmdb_key} />
        {check.checking && <p className={line}>Looking up titles with the new key</p>}
        {check.error !== null && (
          <p role="alert" className="font-display text-body text-label">
            {check.error}
          </p>
        )}
      </div>
      <SecretForm name="tmdb" label="TMDB key" isSet={status.tmdb_key} paste onSaved={saved} />
    </Part>
  );
}

function Display() {
  const { on, set } = useFullscreen();
  return (
    <Part title="Display">
      <FocusGroup id="display" orientation="horizontal" className="flex">
        <FullScreen on={on ?? false} onChange={(next) => set(next).catch((e: unknown) => toast(plain(e)))} />
      </FocusGroup>
    </Part>
  );
}

function FullScreen({ on, onChange }: { on: boolean; onChange: (on: boolean) => void }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: "display:fullscreen" });
  // UI SDK's switch turns with Space and a click (the pad's A); here Enter selects, so it clicks too.
  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      e.currentTarget.click();
      return;
    }
    props.onKeyDown(e);
  };
  return (
    <label className="flex cursor-pointer items-center gap-6 font-display text-lead font-ui text-label">
      <span className="relative pb-1.5">
        Full screen
        {focused && <Lamp />}
      </span>
      <Switch ref={ref} {...props} onKeyDown={onKeyDown} checked={on} onCheckedChange={onChange} />
    </label>
  );
}
