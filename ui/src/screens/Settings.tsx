// Settings: one screen on the bare wall, five parts and nothing else. Folders (each with whether its drive is
// connected, and Remove), tools (where each came from, or what is missing without it, and one
// download for both), the TMDB key, the agent key, and the display: full screen and the intro.
// Opening it detects the tools again. Opened from the mpv error or Fix match, it lands on the part
// that fixes it.

import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useScreenBackdrop } from "@/components/Backdrop";
import { confirm } from "@/components/Dialog";
import { FocusButton } from "@/components/FocusButton";
import { useBrowse } from "@/components/FolderBrowser";
import { Lamp } from "@/components/Lamp";
import { SecretForm } from "@/components/SecretForm";
import { line, ToolLines, useSetToolPath } from "@/components/ToolLines";
import { DownloadButton, DownloadNote } from "@/components/ToolsDownload";
import { ErrorState, Loading } from "@/components/states";
import { Switch } from "@/components/ui/switch";
import { toast } from "@/components/ui/toast";
import { Toggle } from "@/components/ui/toggle";
import { focusItem } from "@/focus/bridge";
import { FocusGroup, FocusScope, useFocusItem } from "@/focus/react";
import {
  DATE_FORMATS,
  LANGUAGE_NAMES,
  LANGUAGES,
  currentDateFormat,
  language,
  setLocale,
  useT,
  type DateFormat,
  type Language,
} from "@/i18n";
import * as api from "@/lib/api";
import { keyRejected, plain, type ApiError, type LibraryStatus, type Source, type ToolStatus } from "@/lib/api";
import { writeDate } from "@/lib/format";
import { keys, useFullscreen, useStatus, useTools } from "@/lib/queries";
import { clearLabel } from "@/lib/tools";
import type { SettingsPart } from "@/nav/stack";

/**
 * Where focus lands as Settings opens: "Add folder", or the control that fixes what sent the owner
 * here. For mpv, the download when it is missing; its path when one set by hand fails.
 */
function settingsLanding(part: SettingsPart | undefined, tools: ToolStatus[]): string {
  if (part === "tmdb") return "tmdb:field";
  if (part !== "tools") return "folders:add";
  const mpv = tools.find((t) => t.tool === "mpv");
  return mpv?.source === "manual" ? "tool:mpv:set" : "tools:download";
}

export function Settings({ part }: { part?: SettingsPart }) {
  // Settings stands on the bare wall: no work's picture and no tint behind it. Back gives the
  // screen beneath its own backdrop again.
  useScreenBackdrop(null, null);
  const t = useT();
  const status = useStatus();
  // Detected again as Settings opens, before it shows, so it can land on a tool.
  const tools = useTools();
  if (status.isPending || tools.isPending) return <Loading />;
  if (status.isError || tools.isError) {
    const failed = status.isError ? status : tools;
    return (
      <ErrorState
        message={plain(failed.error)}
        hint={t("settings.readFailed")}
        action={{ label: t("common.tryAgain"), onAction: () => void failed.refetch() }}
      />
    );
  }
  return <Parts status={status.data} tools={tools.data} part={part} />;
}

function Parts({ status, tools, part }: { status: LibraryStatus; tools: ToolStatus[]; part?: SettingsPart }) {
  // Settings takes focus as it opens, wherever it was opened from (the top bar, a dialog).
  // Where to land is fixed as Settings opens, so detecting the tools again never moves focus.
  const t = useT();
  const [landing] = useState(() => settingsLanding(part, tools));
  useEffect(() => {
    focusItem(landing);
  }, [landing]);
  return (
    <FocusScope id="settings" restoreKey={landing} fallbackKey="folders:add">
      {/* The parts scroll under the title, clear of the top bar. */}
      <div className="absolute inset-0 flex flex-col pt-bar">
        <h1 className="px-edge pt-4 pb-6 font-display text-title font-title tracking-title text-label">{t("settings.title")}</h1>
        <div className="wall-fade min-h-0 flex-1 overflow-y-auto px-edge pb-edge">
          <Folders sources={status.sources} />
          <Tools tools={tools} />
          <TmdbKey status={status} />
          <Part title={t("settings.agentKey")} note={t("settings.agentNote")}>
            <SetLine set={status.agent_key} />
            <SecretForm name="agent" label={t("settings.agentKey")} isSet={status.agent_key} />
          </Part>
          <LanguagePart />
          <DatesPart />
          <Display intro={status.intro} />
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
  const t = useT();
  return <p className={line}>{set ? t("settings.set") : t("settings.notSet")}</p>;
}

function Folders({ sources }: { sources: Source[] }) {
  const t = useT();
  const client = useQueryClient();
  const { browser, browse } = useBrowse();
  const changed = () => void client.invalidateQueries({ queryKey: keys.library });

  const add = () =>
    browse({
      mode: "folder",
      title: t("settings.addAFolder"),
      placeholder: t("settings.typeFolder"),
      onPick: (path) => {
        api.addSource(path).then(changed, (e: unknown) => toast(plain(e)));
      },
    });
  const remove = async (source: Source) => {
    const go = await confirm({
      title: t("settings.removeTitle"),
      body: t("settings.removeBody"),
      confirm: t("common.remove"),
    });
    if (go) await api.removeSource(source.id).then(changed, (e: unknown) => toast(plain(e)));
  };

  return (
    <Part title={t("settings.folders")} note={t("settings.foldersNote")}>
      {sources.length === 0 && <p className={line}>{t("settings.noFolders")}</p>}
      {sources.map((s) => (
        <Row
          key={s.id}
          name={s.path}
          lines={<p className={line}>{s.online ? t("settings.connected") : t("settings.notConnected")}</p>}
          actions={
            <FocusGroup id={`source-${s.id}`} orientation="horizontal">
              <FocusButton focusKey={`source:${s.id}:remove`} variant="plain" onSelect={() => void remove(s)}>
                {t("common.remove")}
              </FocusButton>
            </FocusGroup>
          }
        />
      ))}
      <FocusGroup id="folders-actions" orientation="horizontal" className="flex gap-3">
        <FocusButton focusKey="folders:add" onSelect={add}>
          {t("settings.addFolder")}
        </FocusButton>
        <FocusButton focusKey="folders:scan" onSelect={() => api.refresh(null).catch((e: unknown) => toast(plain(e)))}>
          {t("settings.scanNow")}
        </FocusButton>
      </FocusGroup>
      {browser}
    </Part>
  );
}

function Tools({ tools }: { tools: ToolStatus[] }) {
  const t = useT();
  const { browser, choose, set } = useSetToolPath();
  return (
    <Part title={t("settings.tools")} note={t("settings.toolsNote")}>
      {tools.map((tool) => (
        <Row
          key={tool.tool}
          name={tool.tool}
          lines={<ToolLines tool={tool} />}
          actions={
            <FocusGroup id={`tool-${tool.tool}`} orientation="horizontal" className="flex gap-3">
              {tool.source === "manual" && (
                <FocusButton focusKey={`tool:${tool.tool}:clear`} variant="plain" onSelect={() => void set(tool.tool, null)}>
                  {clearLabel(tool)}
                </FocusButton>
              )}
              <FocusButton focusKey={`tool:${tool.tool}:set`} onSelect={() => choose(tool.tool)}>
                {t("settings.setPath")}
              </FocusButton>
            </FocusGroup>
          }
        />
      ))}
      <FocusGroup id="tools-download" orientation="horizontal" className="flex">
        <DownloadButton tools={tools} focusKey="tools:download" />
      </FocusGroup>
      <DownloadNote />
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
  const [error, setError] = useState<ApiError | null>(null);
  const stage = useRef<"idle" | "queued" | "running">("idle");
  useEffect(() => {
    const off = api.onJobStatus((s) => {
      if (stage.current === "queued" && s.phase === "identifying") stage.current = "running";
      else if (stage.current === "running" && s.phase === null) {
        stage.current = "idle";
        setChecking(false);
        setError(s.last_error);
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

/** The identification's failure as Settings says it, beside the key: a rejected key says to check it here. */
export function keyError(e: ApiError, t: ReturnType<typeof useT>): string {
  return keyRejected(e) ? t("settings.keyRejected") : plain(e);
}

function TmdbKey({ status }: { status: LibraryStatus }) {
  const t = useT();
  const check = useKeyCheck();
  const saved = () => {
    check.start();
    api.identify("unmatched_and_review").catch((e: unknown) => toast(plain(e)));
  };
  return (
    <Part title={t("settings.tmdbKey")} note={t("settings.tmdbNote")}>
      <div className="flex flex-col gap-1">
        <SetLine set={status.tmdb_key} />
        {check.checking && <p className={line}>{t("settings.checkingKey")}</p>}
        {check.error !== null && (
          <p role="alert" className="font-display text-body text-label">
            {keyError(check.error, t)}
          </p>
        )}
      </div>
      <SecretForm name="tmdb" label={t("settings.tmdbKey")} isSet={status.tmdb_key} paste onSaved={saved} />
    </Part>
  );
}

/**
 * The language: Exhibit's own words and the library's text from TMDB. A choice speaks at once;
 * the library's text in a language it has not held before follows as the worker fetches it.
 */
function LanguagePart() {
  const t = useT();
  const client = useQueryClient();
  const choose = (lang: Language) => {
    if (lang === language()) return;
    const before = language();
    setLocale(lang, null);
    api.setLanguage(lang).then(
      () => client.invalidateQueries({ queryKey: keys.library }),
      (e: unknown) => {
        setLocale(before, null);
        toast(plain(e));
      },
    );
  };
  return (
    <Part title={t("settings.language")} note={t("settings.languageNote")}>
      <Choices
        id="language"
        label={t("settings.language")}
        options={LANGUAGES.map((l) => ({ value: l, label: LANGUAGE_NAMES[l], lang: l }))}
        value={language()}
        onChoose={choose}
      />
    </Part>
  );
}

/** How dates are written, each choice shown by today's date written that way. */
function DatesPart() {
  const t = useT();
  const client = useQueryClient();
  const today = new Date();
  const example = (format: DateFormat) => writeDate(today, { format });
  const choose = (format: DateFormat) => {
    if (format === currentDateFormat()) return;
    const before = currentDateFormat();
    setLocale(null, format);
    api.setDateFormat(format).then(
      () => client.invalidateQueries({ queryKey: keys.status }),
      (e: unknown) => {
        setLocale(null, before);
        toast(plain(e));
      },
    );
  };
  return (
    <Part title={t("settings.dates")} note={t("settings.datesNote")}>
      <Choices
        id="dates"
        label={t("settings.dates")}
        options={DATE_FORMATS.map((f) => ({
          value: f,
          label: f === "auto" ? t("settings.automaticAs", { example: example(f) }) : example(f),
        }))}
        value={currentDateFormat()}
        onChoose={choose}
      />
    </Part>
  );
}

/**
 * One of a few, as the wall's Sort is: toggles in a row, the chosen one pressed. Moving onto a
 * choice never makes it; A (Enter, a click) does.
 */
function Choices<V extends string>({
  id,
  label,
  options,
  value,
  onChoose,
}: {
  id: string;
  label: string;
  options: Array<{ value: V; label: string; lang?: string }>;
  value: V;
  onChoose: (value: V) => void;
}) {
  return (
    <FocusGroup id={id} orientation="horizontal" aria-label={label} className="-ml-4 flex flex-wrap items-center gap-1">
      {options.map((o) => (
        <Choice key={o.value} focusKey={`${id}:${o.value}`} lang={o.lang} pressed={o.value === value} onPress={() => onChoose(o.value)}>
          {o.label}
        </Choice>
      ))}
    </FocusGroup>
  );
}

function Choice({
  focusKey,
  lang,
  pressed,
  onPress,
  children,
}: {
  focusKey: string;
  lang?: string;
  pressed: boolean;
  onPress: () => void;
  children: ReactNode;
}) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey });
  return (
    <Toggle
      ref={ref}
      {...props}
      lang={lang}
      variant="plain"
      pressed={pressed}
      // A pressed choice stays pressed: another one is chosen instead.
      onPressedChange={(on) => on && onPress()}
      className="relative focus-visible:bg-transparent focus:text-label"
    >
      {children}
      {focused && <Lamp />}
    </Toggle>
  );
}

function Display({ intro }: { intro: boolean }) {
  const t = useT();
  const client = useQueryClient();
  const { on, set } = useFullscreen();
  const setIntro = (next: boolean) =>
    api.setIntro(next).then(
      () => client.invalidateQueries({ queryKey: keys.status }),
      (e: unknown) => toast(plain(e)),
    );
  return (
    <Part title={t("settings.display")} note={t("settings.displayNote")}>
      <FocusGroup id="display" orientation="vertical" className="flex flex-col items-start gap-6">
        {/* The switch appears once the window has said whether it is full screen, never showing a guess. */}
        {on !== undefined && (
          <SettingSwitch
            focusKey="display:fullscreen"
            label={t("settings.fullScreen")}
            on={on}
            onChange={(next) => set(next).catch((e: unknown) => toast(plain(e)))}
          />
        )}
        <SettingSwitch focusKey="display:intro" label={t("settings.intro")} on={intro} onChange={(next) => void setIntro(next)} />
      </FocusGroup>
    </Part>
  );
}

function SettingSwitch({
  focusKey,
  label,
  on,
  onChange,
}: {
  focusKey: string;
  label: string;
  on: boolean;
  onChange: (on: boolean) => void;
}) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey });
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
        {label}
        {focused && <Lamp />}
      </span>
      <Switch ref={ref} {...props} onKeyDown={onKeyDown} checked={on} onCheckedChange={onChange} />
    </label>
  );
}
