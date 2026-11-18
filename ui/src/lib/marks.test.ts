import { describe, expect, test } from "bun:test";
import { audioTrack } from "../../test/fixtures";
import { pictureMarks, soundMark } from "./marks";

const ids = (marks: Array<{ id: string }>) => marks.map((m) => m.id);

describe("pictureMarks", () => {
  test("every size has a mark, followed by its HDR's", () => {
    expect(ids(pictureMarks(2160, "HDR10").marks)).toEqual(["ultra-hd", "hdr10"]);
    expect(ids(pictureMarks(1608, "Dolby Vision", 3840).marks)).toEqual(["ultra-hd", "dolby-vision"]);
    expect(ids(pictureMarks(1080, null).marks)).toEqual(["hd-1080p"]);
    expect(ids(pictureMarks(536, null, 1280).marks)).toEqual(["hd-720p"]);
    expect(ids(pictureMarks(576, null).marks)).toEqual(["sd"]);
    expect(pictureMarks(2160, "HDR10").words).toBeNull();
  });
  test("a cropped wide frame is named by its width", () => {
    expect(ids(pictureMarks(800, null, 1920).marks)).toEqual(["hd-1080p"]);
  });
  test("HLG has no mark and stays in words", () => {
    expect(pictureMarks(2160, "HLG")).toEqual({ marks: [{ id: "ultra-hd", name: "4K Ultra HD" }], words: "HLG" });
  });
  test("an unknown picture has neither", () => {
    expect(pictureMarks(null, "HDR10")).toEqual({ marks: [], words: null });
    expect(pictureMarks(0, null)).toEqual({ marks: [], words: null });
  });
});

describe("soundMark", () => {
  const t = (codec: string, profile: string | null = null, commentary = false) =>
    audioTrack({ codec, profile, commentary });
  test("the best sound on any track, not the first", () => {
    expect(soundMark([t("ac3"), t("eac3", "Dolby Digital Plus + Dolby Atmos")])?.id).toBe("dolby-atmos");
    expect(soundMark([t("ac3"), t("dts", "DTS-HD MA + DTS:X")])?.id).toBe("dts-x");
    expect(soundMark([t("dts", "DTS"), t("dts", "DTS-HD MA")])?.id).toBe("dts-hd-ma");
    expect(soundMark([t("truehd", "Dolby TrueHD + Dolby Atmos")])?.id).toBe("dolby-atmos");
    expect(soundMark([t("ac3"), t("truehd")])?.id).toBe("dolby-truehd");
    expect(soundMark([t("ac3"), t("eac3")])?.id).toBe("dolby-digital-plus");
    expect(soundMark([t("ac3"), t("dts", "DTS-HD HRA")])?.id).toBe("dts");
    expect(soundMark([t("aac"), t("ac3")])?.id).toBe("dolby-digital");
  });
  test("a commentary does not count, and a format without a mark has none", () => {
    expect(soundMark([t("aac"), t("truehd", null, true)])).toBeNull();
    expect(soundMark([t("aac")])).toBeNull();
    expect(soundMark([])).toBeNull();
  });
});
