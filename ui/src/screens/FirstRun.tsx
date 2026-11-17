// First run: three steps, each one screen with one main action and Skip. A folder (added at
// once, so the library fills in behind the next steps); the media tools that were not found (a
// step only when one is missing); the TMDB key. Finishing, or skipping the last step, goes Home.
// B goes back a step.

import { useQueryClient } from "@tanstack/react-query";
import { motion } from "motion/react";
import { useState, type ReactNode } from "react";
import { FocusButton } from "@/components/FocusButton";
import { useBrowse } from "@/components/FolderBrowser";
import { SecretForm } from "@/components/SecretForm";
import { line, ToolLines, useSetToolPath } from "@/components/ToolLines";
import { toast } from "@/components/ui/toast";
import { FocusGroup, FocusScope } from "@/focus/react";
import * as api from "@/lib/api";
import { plain, type Source, type Tool, type ToolStatus } from "@/lib/api";
import { easeOut, length, seconds } from "@/lib/motion";
import { keys, useStatus, useTools } from "@/lib/queries";
import { toolFound } from "@/lib/tools";
import { useNav } from "@/nav/stack";

type Step = "folder" | "tools" | "tmdb";

/** Where focus lands as each step opens: its main action, or the key's field. */
const LANDING: Record<Step, string> = {
  folder: "first:folder:main",
  tools: "first:tools:main",
  tmdb: "first:tmdb:field",
};

/** The tools step's tools: those not found as the step opens. */
const missingOf = (tools: ToolStatus[]): Tool[] => tools.filter((t) => !toolFound(t)).map((t) => t.tool);

export function FirstRun() {
  const statusTools = useStatus().data?.tools;
  // Detected afresh as first run opens; what the library knew at start is used meanwhile.
  const tools = useTools().data ?? statusTools;
  const [step, setStep] = useState<Step>("folder");
  // Fixed as the tools step opens, so a tool set there stays in view, now found.
  const [missing, setMissing] = useState<Tool[]>([]);
  const [added, setAdded] = useState<Source[]>([]);

  const finish = () => useNav.getState().replaceRoot({ screen: "home" });
  const afterFolder = () => {
    const now = missingOf(tools ?? []);
    setMissing(now);
    setStep(now.length > 0 ? "tools" : "tmdb");
  };
  const back = () => {
    if (step === "tmdb") setStep(missing.length > 0 ? "tools" : "folder");
    else if (step === "tools") setStep("folder");
  };
  // Whether the tools step is among the steps: known for sure once the folder step is done.
  const withTools = step === "folder" ? missingOf(tools ?? []).length > 0 : missing.length > 0;
  const steps: Step[] = withTools ? ["folder", "tools", "tmdb"] : ["folder", "tmdb"];

  return (
    <FocusScope key={step} id={`first-run-${step}`} restoreKey={LANDING[step]} onBack={step === "folder" ? undefined : back}>
      <Stage at={steps.indexOf(step) + 1} of={steps.length}>
        {step === "folder" && <FolderStep added={added} onAdded={(s) => setAdded((a) => [...a, s])} onNext={afterFolder} />}
        {step === "tools" && tools !== undefined && <ToolsStep missing={missing} tools={tools} onNext={() => setStep("tmdb")} />}
        {step === "tmdb" && <TmdbStep onDone={finish} />}
      </Stage>
    </FocusScope>
  );
}

/** A step: which of how many, then its own content, rising into place as it opens. */
function Stage({ at, of, children }: { at: number; of: number; children: ReactNode }) {
  return (
    <motion.div
      className="absolute inset-0 flex flex-col justify-center gap-8 px-edge pb-bar"
      initial={{ opacity: 0, y: length("--shift-screen") }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: seconds("--dur-screen"), ease: easeOut() }}
    >
      <p className="font-display text-body text-label-faint">
        Step {at} of {of}
      </p>
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
  const shown = tools.filter((t) => missing.includes(t.tool));
  const allFound = shown.every(toolFound);
  return (
    <>
      <Heading title="Media tools">
        <p>Exhibit uses free programs to play your files and read them. {shown.length === 1 ? "This one was" : "These were"} not found on this computer.</p>
      </Heading>
      <div className="flex max-w-(--settings-measure) flex-col gap-5">
        {shown.map((t) => (
          <div key={t.tool} className="flex flex-col gap-1">
            <p className="font-display text-lead font-ui text-label">{t.tool}</p>
            <ToolLines tool={t} extra={t.tool === "mpv" ? "Nothing can play until mpv is set." : undefined} />
          </div>
        ))}
      </div>
      <Actions step="tools">
        {allFound ? (
          <FocusButton focusKey="first:tools:main" variant="solid" onSelect={onNext}>
            Continue
          </FocusButton>
        ) : (
          <>
            {shown
              .filter((t) => !toolFound(t))
              .map((t, i) => (
                <FocusButton key={t.tool} focusKey={i === 0 ? "first:tools:main" : `first:tools:${t.tool}`} variant={i === 0 ? "solid" : "line"} onSelect={() => choose(t.tool)}>
                  Set {t.tool} path
                </FocusButton>
              ))}
            <FocusButton focusKey="first:tools:skip" variant="plain" onSelect={onNext}>
              Skip
            </FocusButton>
          </>
        )}
      </Actions>
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
