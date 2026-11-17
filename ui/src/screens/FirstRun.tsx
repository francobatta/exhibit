// First run: three steps, each one screen with one main action and Skip. A folder (added at
// once, so the library fills in behind the next steps); the media tools that were not found (a
// step only when one is missing, led by downloading both); the TMDB key. Finishing, or skipping
// the last step, goes Home.
// B goes back a step.

import { useQueryClient } from "@tanstack/react-query";
import { motion } from "motion/react";
import { useState, type ReactNode } from "react";
import { FocusButton } from "@/components/FocusButton";
import { useBrowse } from "@/components/FolderBrowser";
import { SecretForm } from "@/components/SecretForm";
import { line, ToolLines, useSetToolPath } from "@/components/ToolLines";
import { DownloadButton, DownloadNote } from "@/components/ToolsDownload";
import { toast } from "@/components/ui/toast";
import { FocusGroup, FocusScope } from "@/focus/react";
import * as api from "@/lib/api";
import { plain, type Source, type Tool, type ToolStatus } from "@/lib/api";
import { easeOut, length, seconds } from "@/lib/motion";
import { keys, useTools, useToolsDownload } from "@/lib/queries";
import { toolFound } from "@/lib/tools";
import { useNav } from "@/nav/stack";

type Step = "folder" | "tools" | "tmdb";

/** Where focus lands as each step opens: its main action, or the key's field. */
const LANDING: Record<Step, string> = {
  folder: "first:folder:main",
  tools: "first:tools:main",
  tmdb: "first:tmdb:field",
};

/** The tools not found. */
const missingOf = (tools: ToolStatus[]): Tool[] => tools.filter((t) => !toolFound(t)).map((t) => t.tool);

export function FirstRun() {
  // Detected afresh as first run opens.
  const tools = useTools().data;
  const [step, setStep] = useState<Step>("folder");
  // The tools the tools step offers, fixed by the first detection, so the steps (and their count)
  // are known from the start and a tool set there stays in view, now found.
  const [missing, setMissing] = useState<Tool[] | null>(null);
  if (missing === null && tools !== undefined) setMissing(missingOf(tools));
  const [added, setAdded] = useState<Source[]>([]);

  const steps: Step[] = (missing?.length ?? 0) > 0 ? ["folder", "tools", "tmdb"] : ["folder", "tmdb"];
  const finish = () => useNav.getState().replaceRoot({ screen: "home" });
  const go = (delta: 1 | -1) => setStep(steps[steps.indexOf(step) + delta] ?? step);

  return (
    <FocusScope key={step} id={`first-run-${step}`} restoreKey={LANDING[step]} onBack={step === "folder" ? undefined : () => go(-1)}>
      <Stage at={steps.indexOf(step) + 1} of={missing === null ? null : steps.length}>
        {step === "folder" && <FolderStep added={added} onAdded={(s) => setAdded((a) => [...a, s])} onNext={() => go(1)} />}
        {step === "tools" && tools !== undefined && missing !== null && <ToolsStep missing={missing} tools={tools} onNext={() => go(1)} />}
        {step === "tmdb" && <TmdbStep onDone={finish} />}
      </Stage>
    </FocusScope>
  );
}

/** A step: which of how many, then its own content, rising into place as it opens. */
function Stage({ at, of, children }: { at: number; of: number | null; children: ReactNode }) {
  return (
    <motion.div
      className="absolute inset-0 flex flex-col justify-center gap-8 px-edge pb-bar"
      initial={{ opacity: 0, y: length("--shift-screen") }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: seconds("--dur-screen"), ease: easeOut() }}
    >
      {/* The count waits for the tools to be detected, so it never changes under the owner. */}
      <p className="h-[1lh] font-display text-body text-label-faint">{of !== null && `Step ${at} of ${of}`}</p>
      {children}
    </motion.div>
  );
}

function Heading({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex max-w-measure flex-col gap-5">
      <h1 className="font-display text-display font-title tracking-display text-label">{title}</h1>
      <div className="flex flex-col gap-3 font-reading text-lead leading-reading text-label-dim">{children}</div>
    </div>
  );
}

/** The step's buttons: the main action first, focused as the step opens. */
function Actions({ step, children }: { step: Step; children: ReactNode }) {
  return (
    <FocusGroup id={`first-${step}-actions`} orientation="horizontal" className="flex gap-4">
      {children}
    </FocusGroup>
  );
}

function FolderStep({ added, onAdded, onNext }: { added: Source[]; onAdded: (s: Source) => void; onNext: () => void }) {
  const client = useQueryClient();
  const { browser, browse } = useBrowse();
  const add = () =>
    browse({
      mode: "folder",
      title: "Add a folder",
      placeholder: "Or type a folder's path",
      onPick: (path) => {
        // Adding queues the folder's first scan: the library fills in behind the next steps.
        api.addSource(path).then(
          (s) => {
            onAdded(s);
            void client.invalidateQueries({ queryKey: keys.library });
          },
          (e: unknown) => toast(plain(e)),
        );
      },
    });
  return (
    <>
      <Heading title="Welcome to Exhibit">
        <p>Start with a folder that holds your films, shows or other videos. Exhibit reads it and never changes, moves or deletes anything in it.</p>
      </Heading>
      {added.length > 0 && (
        <ul aria-label="Folders added" className="flex flex-col gap-1">
          {added.map((s) => (
            <li key={s.id} className="font-display text-lead font-ui text-label [overflow-wrap:anywhere]">
              {s.path}
            </li>
          ))}
          <li className={line}>Exhibit is reading {added.length === 1 ? "it" : "them"} now.</li>
        </ul>
      )}
      <Actions step="folder">
        {added.length === 0 ? (
          <>
            <FocusButton focusKey="first:folder:main" variant="solid" onSelect={add}>
              Add folder
            </FocusButton>
            <FocusButton focusKey="first:folder:skip" variant="plain" onSelect={onNext}>
              Skip
            </FocusButton>
          </>
        ) : (
          <>
            <FocusButton focusKey="first:folder:main" variant="solid" onSelect={onNext}>
              Continue
            </FocusButton>
            <FocusButton focusKey="first:folder:more" onSelect={add}>
              Add another folder
            </FocusButton>
          </>
        )}
      </Actions>
      {browser}
    </>
  );
}

function ToolsStep({ missing, tools, onNext }: { missing: Tool[]; tools: ToolStatus[]; onNext: () => void }) {
  const { browser, choose } = useSetToolPath();
  const { status } = useToolsDownload();
  const shown = tools.filter((t) => missing.includes(t.tool));
  const allFound = shown.every(toolFound);
  const running = status?.running ?? false;
  // A finished download is ready at once, before the tools are detected again, so the button
  // never shows its old words on the way to Continue.
  const ready = (allFound || status?.outcome?.kind === "done") && !running;
  return (
    <>
      <Heading title="Media tools">
        <p>
          Exhibit plays your files with mpv and makes thumbnails with ffmpeg, two free programs. {shown.length === 1 ? "This one was" : "These were"} not found on
          this computer. Exhibit can download both for you.
        </p>
      </Heading>
      <div className="flex max-w-(--settings-measure) flex-col gap-5">
        {shown.map((t) => (
          <div key={t.tool} className="flex flex-col gap-1">
            <p className="font-display text-lead font-ui text-label">{t.tool}</p>
            <ToolLines tool={t} extra={t.tool === "mpv" ? "Nothing can play until mpv is here." : undefined} />
          </div>
        ))}
      </div>
      <Actions step="tools">
        {ready ? (
          <FocusButton focusKey="first:tools:main" variant="solid" onSelect={onNext}>
            Continue
          </FocusButton>
        ) : (
          <>
            <DownloadButton tools={tools} focusKey="first:tools:main" variant="solid" />
            {shown
              .filter((t) => !toolFound(t))
              .map((t) => (
                <FocusButton key={t.tool} focusKey={`first:tools:${t.tool}`} onSelect={() => choose(t.tool)}>
                  Set {t.tool} path
                </FocusButton>
              ))}
            {/* The download carries on behind the next step, like the folder's scan. */}
            <FocusButton focusKey="first:tools:skip" variant="plain" onSelect={onNext}>
              {running ? "Continue" : "Skip"}
            </FocusButton>
          </>
        )}
      </Actions>
      <DownloadNote />
      {browser}
    </>
  );
}

function TmdbStep({ onDone }: { onDone: () => void }) {
  const saved = () => {
    api.identify("unmatched_and_review").catch((e: unknown) => toast(plain(e)));
    onDone();
  };
  return (
    <>
      <Heading title="TMDB key">
        <p>With a key from TMDB, Exhibit names your films and shows and hangs their posters and backdrops.</p>
        <p>The key is free: make an account at themoviedb.org, then copy the API key from your account's settings. You can skip this; Exhibit shows everything under its file names, and the key can be added later in Settings.</p>
      </Heading>
      <SecretForm name="tmdb" label="TMDB key" isSet={false} paste removable={false} saveLabel="Save key" focusPrefix="first:tmdb" onSaved={saved} />
      <Actions step="tmdb">
        <FocusButton focusKey="first:tmdb:skip" variant="plain" onSelect={onDone} className="-ml-4">
          Skip
        </FocusButton>
      </Actions>
    </>
  );
}
