// First run: three steps, each one screen with one main action and Skip. A folder (added at
// once, so the library fills in behind the next steps); the media tools that were not found (a
// step only when one is missing, led by downloading both); the TMDB key. Finishing, or skipping
// the last step, goes Home.
// B goes back a step.

import { useQueryClient } from "@tanstack/react-query";
import { motion } from "motion/react";
import { useState, type ReactNode } from "react";
import { BackButton } from "@/components/BackButton";
import { FocusButton } from "@/components/FocusButton";
import { useT } from "@/i18n";
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
      <Stage at={steps.indexOf(step) + 1} of={missing === null ? null : steps.length} onBack={step === "folder" ? undefined : () => go(-1)}>
        {step === "folder" && <FolderStep added={added} onAdded={(s) => setAdded((a) => [...a, s])} onNext={() => go(1)} />}
        {step === "tools" && tools !== undefined && missing !== null && <ToolsStep missing={missing} tools={tools} onNext={() => go(1)} />}
        {step === "tmdb" && <TmdbStep onDone={finish} />}
      </Stage>
    </FocusScope>
  );
}

/** A step: which of how many, then its own content, rising into place as it opens. */
function Stage({ at, of, onBack, children }: { at: number; of: number | null; onBack?: () => void; children: ReactNode }) {
  const t = useT();
  return (
    <motion.div
      className="absolute inset-0 flex flex-col justify-center gap-8 px-edge pb-bar"
      initial={{ opacity: 0, y: length("--shift-screen") }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: seconds("--dur-screen"), ease: easeOut() }}
    >
      {/* Back to the step before, where the bar's Back stands on other screens. */}
      {onBack && (
        <FocusGroup id="first-back" orientation="horizontal" className="absolute inset-x-0 top-0 flex h-bar items-center px-edge">
          <BackButton focusKey="first:back" onBack={onBack} className="-ml-4" />
        </FocusGroup>
      )}
      {/* The count waits for the tools to be detected, so it never changes under the owner. */}
      <p className="h-[1lh] font-display text-body text-label-faint">{of !== null && t("firstRun.step", { at, of })}</p>
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
  const t = useT();
  const client = useQueryClient();
  const { browser, browse } = useBrowse();
  const add = () =>
    browse({
      mode: "folder",
      title: t("settings.addAFolder"),
      placeholder: t("settings.typeFolder"),
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
      <Heading title={t("firstRun.welcome")}>
        <p>{t("firstRun.welcomeBody")}</p>
      </Heading>
      {added.length > 0 && (
        <ul aria-label={t("firstRun.foldersAdded")} className="flex flex-col gap-1">
          {added.map((s) => (
            <li key={s.id} className="font-display text-lead font-ui text-label [overflow-wrap:anywhere]">
              {s.path}
            </li>
          ))}
          <li className={line}>{t("firstRun.reading", { count: added.length })}</li>
        </ul>
      )}
      <Actions step="folder">
        {added.length === 0 ? (
          <>
            <FocusButton focusKey="first:folder:main" variant="solid" onSelect={add}>
              {t("settings.addFolder")}
            </FocusButton>
            <FocusButton focusKey="first:folder:skip" variant="plain" onSelect={onNext}>
              {t("common.skip")}
            </FocusButton>
          </>
        ) : (
          <>
            <FocusButton focusKey="first:folder:main" variant="solid" onSelect={onNext}>
              {t("common.continue")}
            </FocusButton>
            <FocusButton focusKey="first:folder:more" onSelect={add}>
              {t("firstRun.addAnother")}
            </FocusButton>
          </>
        )}
      </Actions>
      {browser}
    </>
  );
}

function ToolsStep({ missing, tools, onNext }: { missing: Tool[]; tools: ToolStatus[]; onNext: () => void }) {
  const t = useT();
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
      <Heading title={t("firstRun.tools")}>
        <p>{t("firstRun.toolsBody", { count: shown.length })}</p>
      </Heading>
      <div className="flex max-w-(--settings-measure) flex-col gap-5">
        {shown.map((tool) => (
          <div key={tool.tool} className="flex flex-col gap-1">
            <p className="font-display text-lead font-ui text-label">{tool.tool}</p>
            <ToolLines tool={tool} extra={tool.tool === "mpv" ? t("firstRun.mpvNeeded") : undefined} />
          </div>
        ))}
      </div>
      <Actions step="tools">
        {ready ? (
          <FocusButton focusKey="first:tools:main" variant="solid" onSelect={onNext}>
            {t("common.continue")}
          </FocusButton>
        ) : (
          <>
            <DownloadButton tools={tools} focusKey="first:tools:main" variant="solid" />
            {shown
              .filter((tool) => !toolFound(tool))
              .map((tool) => (
                <FocusButton key={tool.tool} focusKey={`first:tools:${tool.tool}`} onSelect={() => choose(tool.tool)}>
                  {t("firstRun.setToolPath", { tool: tool.tool })}
                </FocusButton>
              ))}
            {/* The download carries on behind the next step, like the folder's scan. */}
            <FocusButton focusKey="first:tools:skip" variant="plain" onSelect={onNext}>
              {running ? t("common.continue") : t("common.skip")}
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
  const t = useT();
  const saved = () => {
    api.identify("unmatched_and_review").catch((e: unknown) => toast(plain(e)));
    onDone();
  };
  return (
    <>
      <Heading title={t("settings.tmdbKey")}>
        <p>{t("firstRun.tmdbBody")}</p>
        <p>{t("firstRun.tmdbHow")}</p>
      </Heading>
      <SecretForm name="tmdb" label={t("settings.tmdbKey")} isSet={false} paste removable={false} saveLabel={t("firstRun.saveKey")} focusPrefix="first:tmdb" onSaved={saved} />
      <Actions step="tmdb">
        <FocusButton focusKey="first:tmdb:skip" variant="plain" onSelect={onDone} className="-ml-4">
          {t("common.skip")}
        </FocusButton>
      </Actions>
    </>
  );
}
