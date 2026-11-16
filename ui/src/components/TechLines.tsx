// What the file holds, beside the placard: picture, sound, subtitles, which drive, and how far the
// owner got. Shown only for a work whose file has been read (it has technical details). Plain
// text, commas between facts, a missing fact leaves no trace.

import type { AudioTrack, ItemDetail, Source, SubTrack } from "@/lib/api";
import { channels, clock, codec, count, day, facts, language, resolution, WATCHED } from "@/lib/format";
import { useItemSources } from "@/lib/queries";
import { cn } from "@/lib/utils";

/** Tracks listed by name before the rest are counted. */
const TRACKS = 3;

/** The distinct parts, the first few by name and the rest counted. */
function more(all: string[]): string {
  const parts = [...new Set(all)];
  const shown = parts.slice(0, TRACKS);
  const rest = parts.length - shown.length;
  return facts([...shown, rest > 0 ? `${rest} more` : null]);
}

const audio = (tracks: AudioTrack[]) =>
  more(tracks.map((t) => [language(t.lang), channels(t.channels), codec(t.codec)].filter(Boolean).join(" ")));

function subtitles(tracks: SubTrack[]): string {
  if (tracks.length === 0) return "None";
  const names = tracks.map((t) => language(t.lang)).filter((n) => n !== null);
  return names.length > 0 ? more(names) : count(tracks.length, "track", "tracks");
}

function played(detail: ItemDetail): string {
  const play = detail.play;
  const state = play?.watched
    ? WATCHED
    : play !== null && play.position_s > 0
      ? `Stopped at ${clock(play.position_s)}`
      : "Not yet";
  return facts([state, play !== null && play.last_played_at > 0 ? `last played ${day(play.last_played_at)}` : null]);
}

/** The lines, label and value, for a full record; none when its file has not been read. */
export function techLines(detail: ItemDetail, sources: Source[]): Array<{ label: string; value: string }> {
  const tech = detail.files.find((f) => f.tech !== null)?.tech;
  if (!tech) return [];
  const lines = [
    { label: "Video", value: facts([resolution(tech.height, tech.hdr, tech.width), codec(tech.video_codec)]) },
    { label: "Audio", value: audio(tech.audio) },
    { label: "Subtitles", value: subtitles(tech.subtitles) },
    { label: "Drive", value: facts(sources.map((s) => (s.online ? s.path : `${s.path} is not connected`))) },
    { label: "Played", value: played(detail) },
  ];
  return lines.filter((l) => l.value !== "");
}

export function TechLines({ detail, className }: { detail: ItemDetail; className?: string }) {
  const lines = techLines(detail, useItemSources(detail));
  if (lines.length === 0) return null;
  return (
    <dl className={cn("grid grid-cols-[auto_minmax(0,1fr)] gap-x-5 gap-y-1.5 font-display text-body", className)}>
      {lines.map((l) => (
        <div key={l.label} className="contents">
          <dt className="text-label-faint">{l.label}</dt>
          <dd className="truncate text-label-dim">{l.value}</dd>
        </div>
      ))}
    </dl>
  );
}
