import { expect, test } from "bun:test";
import { activity, audioTrack, detail, mediaFile, service } from "../../test/fixtures";
import type { ItemDetail, PlayState } from "@/lib/api";
import { techLines } from "./TechLines";

const MARCH_3_2019 = Date.UTC(2019, 2, 3) / 1000;
const tech = { duration_s: 6840, width: 1920, height: 1080, video_codec: "h264", hdr: null, audio: [audioTrack()], subtitles: [] };
const play = (over: Partial<PlayState>): PlayState => ({ item_id: 1, position_s: 0, duration_s: 6840, watched: false, last_played_at: 0, ...over });
const playedLine = (d: ItemDetail) => techLines(d, [], [service()]).find((l) => l.label === "Played")?.value;
const film = (over: Partial<ItemDetail>) => detail({ files: [mediaFile({ tech })], ...over });

test("the_played_line_follows_the_merged_state", () => {
  // Logged on Letterboxd, never played here: the eye is lit, and the line says why.
  const logged = activity({ watched: true, watches: [{ service: "letterboxd", on: MARCH_3_2019, rewatch: false }] });
  expect(playedLine(film({ activity: logged }))).toBe("Logged on Letterboxd March 3, 2019");
  // The newest watch from a service speaks; an undated one says only where.
  const undated = activity({ watched: true, watches: [{ service: "letterboxd", on: null, rewatch: false }] });
  expect(playedLine(film({ activity: undated }))).toBe("Logged on Letterboxd");
  // A part-way play here still says where it stopped.
  const stopped = film({ activity: logged, play: play({ position_s: 3852, last_played_at: 0 }) });
  expect(playedLine(stopped)).toBe("Stopped at 1:04:12");
  // Watched in Exhibit says so, as before.
  const seen = film({ activity: activity({ watched: true }), play: play({ watched: true }) });
  expect(playedLine(seen)).toBe("Watched");
  // Marked watched here with no play and no service watch.
  expect(playedLine(film({ activity: activity({ watched: true, watched_choice: true }) }))).toBe("Watched");
  // Nothing says watched.
  expect(playedLine(film({}))).toBe("Not yet");
  // Marked unwatched here over an old play: the line agrees with the eye.
  expect(playedLine(film({ activity: activity({ watched: false, watched_choice: false }), play: play({ watched: true }) }))).toBe("Not yet");
});
