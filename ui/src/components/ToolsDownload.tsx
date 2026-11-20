// Downloading mpv and ffmpeg in one press, in Settings and first run. The button keeps its place
// and its focus while the download runs: its words become the progress, and a line of light along
// its foot fills as the bytes arrive. Under it, the note that the builds are not the projects' own,
// and how the last download ended.

import { motion } from "motion/react";
import type { DownloadStatus, ToolStatus } from "@/lib/api";
import { focusSpring } from "@/lib/motion";
import { useToolsDownload } from "@/lib/queries";
import { useT } from "@/i18n";
import { downloadLabel, downloadNote, downloadOutcome, downloadProgress, downloadShare } from "@/lib/tools";
import { FocusButton } from "./FocusButton";

/** The button. A press while a download runs does nothing; the words say it is under way. */
export function DownloadButton({
  tools,
  focusKey,
  variant = "line",
}: {
  tools: ToolStatus[];
  focusKey: string;
  variant?: "solid" | "line";
}) {
  useT();
  const { status, start } = useToolsDownload();
  const progress = status && downloadProgress(status);
  return (
    <FocusButton focusKey={focusKey} variant={variant} onSelect={start} className="min-w-(--download-control) tabular-nums">
      <span aria-live="polite">{progress ?? downloadLabel(tools)}</span>
      {status?.running && <Fill status={status} />}
    </FocusButton>
  );
}

/** The light along the button's foot: a transform only, from the left. */
function Fill({ status }: { status: DownloadStatus }) {
  return (
    <motion.span
      aria-hidden
      className="pointer-events-none absolute inset-x-0 bottom-0 h-0.5 origin-left bg-label/70"
      initial={{ scaleX: 0 }}
      animate={{ scaleX: downloadShare(status) }}
      transition={focusSpring()}
    />
  );
}

/** Where the builds come from, then how the last download ended. */
export function DownloadNote() {
  useT();
  const { status } = useToolsDownload();
  const outcome = status && downloadOutcome(status);
  return (
    <div className="flex max-w-measure flex-col gap-2">
      {outcome && (
        <p role={outcome.failed ? "alert" : "status"} className="font-display text-body text-label">
          {outcome.text}
        </p>
      )}
      <p className="font-reading text-body leading-reading text-label-faint">{downloadNote()}</p>
    </div>
  );
}
