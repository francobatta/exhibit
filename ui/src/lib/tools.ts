// What Settings and first run say about each media tool (where it came from, which version, where
// it is, or what is missing without it) and about downloading them.

import { plain, type DownloadStatus, type Tool, type ToolStatus } from "./api";

/** What each tool supplies, said when it is missing. */
export const CONSEQUENCE: Record<Tool, string> = {
  mpv: "mpv plays your files and reads their details",
  ffmpeg: "ffmpeg makes thumbnails for Other; mpv is used instead",
};

/** Found and answering. A path set by hand that has gone (no version) is not found. */
export function toolFound(t: ToolStatus): boolean {
  return t.path !== null && (t.version !== null || t.source !== "manual");
}

/** "0.40.0" from "mpv v0.40.0", "N-128915-g3d16b774d" from "ffmpeg version N-128915-g3d16b774d". */
export function toolVersion(t: ToolStatus): string | null {
  if (t.version === null) return null;
  const v = t.version.replace(new RegExp(`^${t.tool}\\s+(version\\s+)?`, "i"), "").replace(/^v(?=\d)/, "");
  return v === "" ? null : v;
}

/** "11 November 2026" from a release tag "20261111"; null for anything else. */
export function releaseDay(release: string | null): string | null {
  const m = release?.match(/^(\d{4})(\d{2})(\d{2})$/);
  if (!m) return null;
  const date = new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3])));
  return date.toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
}

const ORIGIN = {
  manual: "Set by you",
  downloaded: "Downloaded by Exhibit",
  detected: "Found on this PC",
} as const;

/**
 * Where the tool came from and which version: "Downloaded by Exhibit, version 0.42.0, built 11
 * November 2026", "Found on this PC, version 7.1", "Not found at D:\gone\mpv.exe", "Not found".
 */
export function toolLine(t: ToolStatus): string {
  if (t.path === null || t.source === null) return "Not found";
  if (!toolFound(t)) return `Not found at ${t.path}`;
  const v = toolVersion(t);
  const built = t.source === "downloaded" ? releaseDay(t.release) : null;
  return [ORIGIN[t.source], v && `version ${v}`, built && `built ${built}`].filter(Boolean).join(", ");
}

/** What clearing a path set by hand hands over to. */
export const clearLabel = (t: ToolStatus): string => (t.downloaded ? "Use downloaded" : "Use detected");

/** The download button: "Download mpv and ffmpeg", or "Get the latest" once Exhibit has downloaded them. */
export const downloadLabel = (tools: ToolStatus[]): string =>
  tools.some((t) => t.downloaded) ? "Get the latest" : "Download mpv and ffmpeg";

const mb = (n: number) => Math.round(n / 1_000_000);

/** While a download runs: "Looking for the latest build", "Downloading, 34 of 63 MB", "Unpacking". */
export function downloadProgress(s: DownloadStatus): string | null {
  if (!s.running) return null;
  switch (s.step?.step) {
    case "downloading":
      return `Downloading, ${mb(s.step.done)} of ${mb(s.step.total)} MB`;
    case "unpacking":
      return "Unpacking";
    default:
      return "Looking for the latest build";
  }
}

/** The share downloaded, 0 to 1; unpacking counts as all of it. */
export function downloadShare(s: DownloadStatus): number {
  if (s.step?.step === "downloading") return s.step.total > 0 ? Math.min(1, s.step.done / s.step.total) : 0;
  return s.step?.step === "unpacking" ? 1 : 0;
}

/**
 * The core's "what happened; what to do (detail)" as sentences without the detail: "could not
 * reach GitHub (dns error); check the internet connection, then try again" reads "Could not reach
 * GitHub. Check the internet connection, then try again."
 */
export function sentences(message: string): string {
  return plain(message)
    .replace(/\s*\([^)]*\)/g, "")
    .split(/;\s*/)
    .map((s) => s.trim())
    .filter((s) => s !== "")
    .map((s) => s.charAt(0).toUpperCase() + s.slice(1).replace(/\.$/, ""))
    .join(". ")
    .concat(".");
}

/** How the last download ended, or null: "Downloaded the build of 11 November 2026." */
export function downloadOutcome(s: DownloadStatus): { text: string; failed: boolean } | null {
  const o = s.outcome;
  if (s.running || o === null) return null;
  if (o.kind === "failed") return { text: sentences(o.message), failed: true };
  const day = releaseDay(o.release) ?? o.release;
  return {
    text: o.already_latest ? `You already have the latest build, from ${day}.` : `Downloaded the build of ${day}.`,
    failed: false,
  };
}

/** Said before downloading: the builds are not the projects' own. */
export const DOWNLOAD_NOTE =
  "The download comes from shinchiro's builds on GitHub. A volunteer makes them, not the mpv or FFmpeg projects; mpv's own website lists them first.";
