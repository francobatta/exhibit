// The format marks a placard shows: the logo of each picture and sound format a file is in, from
// its owner or the body that labels it (see assets/marks/SOURCES.md). A mark stands only for what
// the file is known to hold: HLG has no mark and stays in words. The sound mark names the best
// sound the file offers, on whichever track it is.

import type { AudioTrack } from "./api";
import { UHD } from "./format";

export type MarkId =
  | "ultra-hd"
  | "hd-1080p"
  | "hd-720p"
  | "sd"
  | "hdr10"
  | "dolby-vision"
  | "dolby-truehd"
  | "dolby-digital-plus"
  | "dolby-digital"
  | "dolby-atmos"
  | "dts"
  | "dts-hd-ma"
  | "dts-x";

export interface Mark {
  id: MarkId;
  /** The format's name, as the mark says it to a screen reader. */
  name: string;
}

const HDR: Record<string, Mark> = {
  HDR10: { id: "hdr10", name: "HDR10" },
  "Dolby Vision": { id: "dolby-vision", name: "Dolby Vision" },
};

/** Sound formats from best to plainest, each with how a track is known to be in it. */
const SOUND: Array<{ mark: Mark; is: (t: AudioTrack) => boolean }> = [
  { mark: { id: "dts-x", name: "DTS:X" }, is: (t) => t.codec === "dts" && has(t, "DTS:X") },
  { mark: { id: "dolby-atmos", name: "Dolby Atmos" }, is: (t) => has(t, "Atmos") },
  { mark: { id: "dts-hd-ma", name: "DTS-HD Master Audio" }, is: (t) => t.codec === "dts" && has(t, "DTS-HD MA") },
  { mark: { id: "dolby-truehd", name: "Dolby TrueHD" }, is: (t) => t.codec === "truehd" },
  { mark: { id: "dolby-digital-plus", name: "Dolby Digital Plus" }, is: (t) => t.codec === "eac3" },
  { mark: { id: "dts", name: "DTS" }, is: (t) => t.codec === "dts" },
  { mark: { id: "dolby-digital", name: "Dolby Digital" }, is: (t) => t.codec === "ac3" },
];

const has = (t: AudioTrack, part: string) => t.profile?.includes(part) ?? false;

/** The picture's size as a mark: 4K, 1080p, 720p or SD. The width names a frame cropped to a wide ratio. */
function sizeMark(height: number, width: number): Mark {
  if (height >= UHD.height || width >= UHD.width) return { id: "ultra-hd", name: "4K Ultra HD" };
  if (height >= 1000 || width >= 1800) return { id: "hd-1080p", name: "1080p" };
  if (height >= 700 || width >= 1200) return { id: "hd-720p", name: "720p" };
  return { id: "sd", name: "SD" };
}

/** The picture as marks, and what of it has no mark, in words ("HLG"). */
export function pictureMarks(
  height: number | null,
  hdr: string | null,
  width?: number | null,
): { marks: Mark[]; words: string | null } {
  if (typeof height !== "number" || !(height > 0)) return { marks: [], words: null };
  const range = hdr?.trim() || null;
  const rangeMark = range === null ? undefined : HDR[range];
  const marks = [sizeMark(height, typeof width === "number" ? width : 0)];
  if (rangeMark) marks.push(rangeMark);
  return { marks, words: rangeMark ? null : range };
}

/** The mark of the best sound among the tracks (a commentary does not count), when it has one. */
export function soundMark(tracks: AudioTrack[]): Mark | null {
  const sound = tracks
    .filter((t) => !t.commentary)
    .map((t) => ({ ...t, codec: t.codec.trim().toLowerCase() }));
  return SOUND.find((f) => sound.some(f.is))?.mark ?? null;
}
