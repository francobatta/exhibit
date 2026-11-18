import { describe, expect, test } from "bun:test";
import { clock, confidence, episodeLabel, facts, resolution, runtime, soundFormat } from "./format";

describe("runtime", () => {
  test("hours and minutes", () => {
    expect(runtime(162)).toBe("2 h 42 min");
    expect(runtime(58)).toBe("58 min");
    expect(runtime(120)).toBe("2 h");
  });
  test("nothing for unknown, zero or nonsense", () => {
    expect(runtime(null)).toBeNull();
    expect(runtime(0)).toBeNull();
    expect(runtime(-3)).toBeNull();
    expect(runtime(Number.NaN)).toBeNull();
  });
});

describe("clock", () => {
  test("hours only when there are some", () => {
    expect(clock(3852)).toBe("1:04:12");
    expect(clock(245)).toBe("4:05");
    expect(clock(0)).toBe("0:00");
    expect(clock(59.9)).toBe("0:59");
  });
  test("nonsense reads as zero", () => {
    expect(clock(Number.NaN)).toBe("0:00");
    expect(clock(-5)).toBe("0:00");
  });
});

describe("resolution", () => {
  test("names by height", () => {
    expect(resolution(2160, null)).toBe("4K");
    expect(resolution(1080, null)).toBe("1080p");
    expect(resolution(720, null)).toBe("720p");
    expect(resolution(480, null)).toBe("SD");
    expect(resolution(576, null)).toBe("SD");
  });
  test("with HDR", () => {
    expect(resolution(2160, "Dolby Vision")).toBe("4K Dolby Vision");
    expect(resolution(2160, "HDR10")).toBe("4K HDR10");
  });
  test("a cropped wide frame is named by its width", () => {
    expect(resolution(1608, null, 3840)).toBe("4K");
    expect(resolution(800, null, 1920)).toBe("1080p");
    expect(resolution(536, null, 1280)).toBe("720p");
  });
  test("nothing without a height", () => {
    expect(resolution(null, null)).toBeNull();
    expect(resolution(null, "HDR10")).toBeNull();
    expect(resolution(0, null)).toBeNull();
  });
});

describe("facts", () => {
  test("joins what is present with commas", () => {
    expect(facts(["1979", "Ridley Scott"])).toBe("1979, Ridley Scott");
    expect(facts(["1979", null, undefined, ""])).toBe("1979");
    expect(facts([null, "Ridley Scott"])).toBe("Ridley Scott");
    expect(facts([])).toBe("");
  });
  test("never says null, undefined or NaN, and never leaves a dangling comma", () => {
    expect(facts(["null", "undefined", "NaN", " ", "2 h"])).toBe("2 h");
    expect(facts([String(Number.NaN), "Alien"])).toBe("Alien");
    expect(facts(["Alien,", " 1979 "])).toBe("Alien, 1979");
  });
});

describe("episodeLabel", () => {
  test("season and episode", () => {
    expect(episodeLabel(2, 4)).toBe("S2 E4");
    expect(episodeLabel(null, 4)).toBe("E4");
    expect(episodeLabel(null, null)).toBe("");
    expect(episodeLabel(2, null)).toBe("S2");
  });
});

test("a candidate's score reads as Likely from 0.8, Possible from 0.6, else Unlikely", () => {
  expect([0.8, 0.6, 0.59].map(confidence)).toEqual(["Likely", "Possible", "Unlikely"]);
});

describe("soundFormat", () => {
  test("a profile that names a richer format wins over the codec", () => {
    expect(soundFormat("dts", "DTS-HD MA + DTS:X")).toBe("DTS:X");
    expect(soundFormat("dts", "DTS-HD MA")).toBe("DTS-HD MA");
    expect(soundFormat("truehd", "Dolby TrueHD + Dolby Atmos")).toBe("Dolby Atmos");
    expect(soundFormat("eac3", "Dolby Digital Plus + Dolby Atmos")).toBe("Dolby Atmos");
  });
  test("otherwise the codec as people know it", () => {
    expect(soundFormat("dts", "DTS")).toBe("DTS");
    expect(soundFormat("aac", "LC")).toBe("AAC");
    expect(soundFormat("ac3", null)).toBe("AC-3");
    expect(soundFormat(null, null)).toBeNull();
  });
});
