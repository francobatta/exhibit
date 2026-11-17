// What Settings and first run say about each media tool: whether it is found, where, which
// version, and what it supplies when it is missing.

import type { Tool, ToolStatus } from "./api";

/** What each tool supplies, said when it is missing. */
export const CONSEQUENCE: Record<Tool, string> = {
  mpv: "mpv plays your files",
  ffprobe: "ffprobe reads resolution, HDR and tracks faster; mpv is used instead",
  ffmpeg: "ffmpeg makes thumbnails for Other; mpv is used instead",
};

/** Found and answering. A path set by hand that has gone (no version) is not found. */
export function toolFound(t: ToolStatus): boolean {
  return t.path !== null && (t.version !== null || !t.manual);
}

/** "0.40.0" from "mpv v0.40.0", "7.1-full_build" from "ffprobe version 7.1-full_build". */
export function toolVersion(t: ToolStatus): string | null {
  if (t.version === null) return null;
  const v = t.version.replace(new RegExp(`^${t.tool}\\s+(version\\s+)?`, "i"), "").replace(/^v(?=\d)/, "");
  return v === "" ? null : v;
}

/** "Found at C:\mpv\mpv.exe, version 0.40.0", "Found at C:\mpv\mpv.exe", "Not found at D:\gone\mpv.exe", "Not found". */
export function toolLine(t: ToolStatus): string {
  if (t.path === null) return "Not found";
  if (!toolFound(t)) return `Not found at ${t.path}`;
  const v = toolVersion(t);
  return v === null ? `Found at ${t.path}` : `Found at ${t.path}, version ${v}`;
}
