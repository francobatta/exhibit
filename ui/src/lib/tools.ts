// What Settings and first run say about each media tool (where it came from, which version, where
// it is, or what is missing without it) and about downloading them.

import { t } from "@/i18n";
import { plain, type DownloadStatus, type Tool, type ToolStatus } from "./api";
import { writeDate } from "./format";

/** What each tool supplies, said when it is missing. */
export const consequence = (tool: Tool): string => (tool === "mpv" ? t("tools.mpvDoes") : t("tools.ffmpegDoes"));

/** Found and answering. A path set by hand that has gone (no version) is not found. */
export function toolFound(tool: ToolStatus): boolean {
  return tool.path !== null && (tool.version !== null || tool.source !== "manual");
}

/** "0.40.0" from "mpv v0.40.0", "N-128915-g3d16b774d" from "ffmpeg version N-128915-g3d16b774d". */
export function toolVersion(tool: ToolStatus): string | null {
  if (tool.version === null) return null;
  const v = tool.version.replace(new RegExp(`^${tool.tool}\\s+(version\\s+)?`, "i"), "").replace(/^v(?=\d)/, "");
  return v === "" ? null : v;
}

/** "November 11, 2026" from a release tag "20261111", in the owner's date format; null for anything else. */
export function releaseDay(release: string | null): string | null {
  const m = release?.match(/^(\d{4})(\d{2})(\d{2})$/);
  if (!m) return null;
  return writeDate(new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3]))), { utc: true });
}

const ORIGIN = {
  manual: "tools.setByYou",
  downloaded: "tools.downloaded",
  detected: "tools.detected",
} as const;

/**
 * Where the tool came from and which version: "Downloaded by Exhibit, version 0.42.0, built
 * November 11, 2026", "Found on this PC, version 7.1", "Not found at D:\gone\mpv.exe", "Not found".
 */
export function toolLine(tool: ToolStatus): string {
  if (tool.path === null || tool.source === null) return t("tools.notFound");
  if (!toolFound(tool)) return t("tools.notFoundAt", { path: tool.path });
  const version = toolVersion(tool);
  const built = tool.source === "downloaded" ? releaseDay(tool.release) : null;
  return [t(ORIGIN[tool.source]), version && t("tools.version", { version }), built && t("tools.built", { day: built })]
    .filter(Boolean)
    .join(", ");
}

/** What clearing a path set by hand hands over to. */
export const clearLabel = (tool: ToolStatus): string =>
  tool.downloaded ? t("tools.useDownloaded") : t("tools.useDetected");

/** The download button: "Download mpv and ffmpeg", or "Get the latest" once Exhibit has downloaded them. */
export const downloadLabel = (tools: ToolStatus[]): string =>
  tools.some((tool) => tool.downloaded) ? t("tools.getLatest") : t("tools.download");

const mb = (n: number) => Math.round(n / 1_000_000);

/** While a download runs: "Looking for the latest build", "Downloading, 34 of 63 MB", "Unpacking". */
export function downloadProgress(s: DownloadStatus): string | null {
  if (!s.running) return null;
  switch (s.step?.step) {
    case "downloading":
      return t("tools.downloading", { done: mb(s.step.done), total: mb(s.step.total) });
    case "unpacking":
      return t("tools.unpacking");
    default:
      return t("tools.checking");
  }
}

/** The share downloaded, 0 to 1; unpacking counts as all of it. */
export function downloadShare(s: DownloadStatus): number {
  if (s.step?.step === "downloading") return s.step.total > 0 ? Math.min(1, s.step.done / s.step.total) : 0;
  return s.step?.step === "unpacking" ? 1 : 0;
}

/** How the last download ended, or null: "Downloaded the build of November 11, 2026." */
export function downloadOutcome(s: DownloadStatus): { text: string; failed: boolean } | null {
  const o = s.outcome;
  if (s.running || o === null) return null;
  if (o.kind === "failed") return { text: plain(o.error), failed: true };
  const day = releaseDay(o.release) ?? o.release;
  return {
    text: o.already_latest ? t("tools.alreadyLatest", { day }) : t("tools.downloadedBuild", { day }),
    failed: false,
  };
}

/** Said before downloading: the builds are not the projects' own. */
export const downloadNote = (): string => t("tools.note");
