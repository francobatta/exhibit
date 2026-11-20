// What the file holds, beside the placard: picture, sound, subtitles, which drive, and how far the
// owner got. Shown only for a work whose file has been read (it has technical details). Plain
// text, commas between facts, a missing fact leaves no trace.

import { t, useT } from "@/i18n";
import type { AudioTrack, ItemDetail, ServiceView, Source, SubTrack } from "@/lib/api";
import { EXHIBIT, channels, clock, codec, day, facts, languageName, loggedLine, resolution, soundFormat } from "@/lib/format";
import { useItemSources, useServices } from "@/lib/queries";
import { cn } from "@/lib/utils";

/** Tracks listed by name before the rest are counted. */
const TRACKS = 3;

/** The distinct parts, the first few by name and the rest counted. */
function more(all: string[]): string {
  const parts = [...new Set(all)];
  const shown = parts.slice(0, TRACKS);
  const rest = parts.length - shown.length;
  return facts([...shown, rest > 0 ? t("units.more", { count: rest }) : null]);
}

/** The sound tracks, the film's own first: "English 7.1 DTS-HD MA, Russian 5.1 AC-3, English commentary". */
const audio = (tracks: AudioTrack[]) =>
  more(
    [...tracks]
      .sort((a, b) => Number(b.original) - Number(a.original))
      .map((track) => {
        const name = languageName(track.lang);
        if (track.commentary) return name === null ? t("tech.commentaryUnnamed") : t("tech.commentary", { language: name });
        return [name, channels(track.channels), soundFormat(track.codec, track.profile)].filter(Boolean).join(" ");
      }),
  );

function subtitles(tracks: SubTrack[]): string {
  if (tracks.length === 0) return t("placard.none");
  const names = tracks.map((track) => languageName(track.lang)).filter((n) => n !== null);
  return names.length > 0 ? more(names) : t("units.track", { count: tracks.length });
}

/**
 * How far the owner got, as the eye says it (the merged state): watched here; stopped part-way here;
 * watched by a service's log, newest first ("Logged on Letterboxd 3 March 2019"); else not yet.
 */
function played(detail: ItemDetail, services: ServiceView[] | undefined): string {
  const play = detail.play;
  const { watched, watches } = detail.activity;
  const logged = watches.find((w) => w.service !== EXHIBIT);
  const state =
    watched && play?.watched
      ? t("work.watched")
      : play !== null && !play.watched && play.position_s > 0
        ? t("tech.stoppedAt", { time: clock(play.position_s) })
        : watched
          ? logged === undefined
            ? t("work.watched")
            : loggedLine(logged, services)
          : t("tech.notYet");
  const last = play !== null && play.last_played_at > 0 ? day(play.last_played_at) : null;
  return facts([state, last === null ? null : t("tech.lastPlayed", { day: last })]);
}

/** The lines, label and value, for a full record; none when its file has not been read. */
export function techLines(detail: ItemDetail, sources: Source[], services?: ServiceView[]): Array<{ label: string; value: string }> {
  const tech = detail.files.find((f) => f.tech !== null)?.tech;
  if (!tech) return [];
  const lines = [
    { label: t("tech.video"), value: facts([resolution(tech.height, tech.hdr, tech.width), codec(tech.video_codec)]) },
    { label: t("tech.audio"), value: audio(tech.audio) },
    { label: t("tech.subtitles"), value: subtitles(tech.subtitles) },
    { label: t("tech.drive"), value: facts(sources.map((s) => (s.online ? s.path : t("tech.notConnected", { path: s.path })))) },
    { label: t("tech.played"), value: played(detail, services) },
  ];
  return lines.filter((l) => l.value !== "");
}

export function TechLines({ detail, className }: { detail: ItemDetail; className?: string }) {
  useT();
  const lines = techLines(detail, useItemSources(detail), useServices().data);
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
