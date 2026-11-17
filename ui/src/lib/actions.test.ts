import { describe, expect, mock, test } from "bun:test";
import { detail, summary } from "../../test/fixtures";
import { itemActions, runAction, type ActionContext } from "./actions";

const labels = (list: ReturnType<typeof itemActions>) => list.map((a) => a.label);

describe("itemActions", () => {
  test("an unwatched movie plays", () => {
    const list = itemActions(summary());
    expect(list[0]).toEqual({ id: "play", label: "Play" });
    expect(labels(list)).toContain("Mark as watched");
    expect(labels(list)).toContain("Love");
    expect(labels(list)).toContain("Details");
    expect(labels(list)).not.toContain("Play from the start");
  });

  test("a half-watched movie resumes from where it stopped, or starts again", () => {
    const list = itemActions(
      detail({ play: { item_id: 1, position_s: 3852, duration_s: 6840, watched: false, last_played_at: 0 } }),
    );
    expect(labels(list).slice(0, 2)).toEqual(["Resume from 1:04:12", "Play from the start"]);
    expect(list[0]!.id).toBe("resume");
    expect(list[1]!.id).toBe("restart");
  });

  test("a half-watched summary says Resume without a time", () => {
    expect(labels(itemActions(summary({ progress: 0.4 }))).slice(0, 2)).toEqual([
      "Resume",
      "Play from the start",
    ]);
  });

  test("a watched movie plays from the start and can be marked not watched", () => {
    const list = labels(itemActions(summary({ watched: true, progress: 0.97 })));
    expect(list[0]).toBe("Play");
    expect(list).toContain("Mark as not watched");
    expect(list).not.toContain("Mark as watched");
    expect(list).not.toContain("Resume");
  });

  test("a show plays its next episode", () => {
    const next = summary({ id: 9, kind: "episode", season: 2, episode: 4 });
    const fromDetail = itemActions(detail({ next_episode: next }, { id: 5, kind: "show" }));
    expect(fromDetail[0]).toEqual({ id: "play", label: "Play S2 E4" });
    expect(itemActions(summary({ id: 5, kind: "show" }))[0]!.label).toBe("Play next episode");
  });

  test("an unavailable item cannot play, and says why", () => {
    const list = itemActions(summary({ available: false, progress: 0.5 }));
    const plays = list.filter((a) => ["play", "resume", "restart"].includes(a.id));
    expect(plays.length).toBe(2);
    for (const a of plays) expect(a.disabled).toBe("Drive is not connected");
    expect(list.find((a) => a.id === "details")?.disabled).toBeUndefined();
  });

  test("a needs-review other item can have its match fixed", () => {
    const list = itemActions(summary({ kind: "other", match_state: "needsreview" }));
    expect(labels(list)).toContain("Fix match");
    expect(labels(list)).not.toContain("Send to Other");
  });

  test("a movie can be sent to Other; an episode cannot, nor fixed", () => {
    expect(labels(itemActions(summary()))).toContain("Send to Other");
    const ep = labels(itemActions(summary({ kind: "episode", season: 1, episode: 2 })));
    expect(ep).not.toContain("Send to Other");
    expect(ep).not.toContain("Fix match");
  });

  test("loved reads as Remove from loved", () => {
    expect(labels(itemActions(summary({ loved: true })))).toContain("Remove from loved");
    expect(labels(itemActions(detail({}, { loved: true })))).toContain("Remove from loved");
  });
});

function context() {
  return {
    play: mock(async (_v: { id: number; resume: boolean }): Promise<unknown> => undefined),
    setWatched: mock(async (_v: { id: number; watched: boolean }): Promise<unknown> => undefined),
    setLoved: mock(async (_v: { id: number; loved: boolean }): Promise<unknown> => undefined),
    moveToOther: mock(async (_v: { id: number }): Promise<unknown> => undefined),
    open: mock((_item: unknown) => {}),
    fix: mock((_item: unknown) => {}),
    confirm: mock(async (_r: { confirm: string }) => true),
    fail: mock((_m: string) => {}),
    failOffline: mock((_id: number) => {}),
  } satisfies ActionContext;
}

describe("runAction", () => {
  test("play, resume and restart call play with resume false, true, false", async () => {
    const ctx = context();
    await runAction({ id: "play", label: "Play" }, summary({ id: 3 }), ctx);
    await runAction({ id: "resume", label: "Resume" }, summary({ id: 3 }), ctx);
    await runAction({ id: "restart", label: "Play from the start" }, summary({ id: 3 }), ctx);
    expect(ctx.play.mock.calls.map((c) => c[0])).toEqual([
      { id: 3, resume: false },
      { id: 3, resume: true },
      { id: 3, resume: false },
    ]);
  });

  test("a show plays by its own id; the core picks the episode", async () => {
    const ctx = context();
    const next = summary({ id: 9, kind: "episode", season: 2, episode: 4 });
    await runAction({ id: "play", label: "Play S2 E4" }, detail({ next_episode: next }, { id: 5, kind: "show" }), ctx);
    expect(ctx.play.mock.calls[0]![0]).toEqual({ id: 5, resume: false });
  });

  test("a disabled action does nothing", async () => {
    const ctx = context();
    await runAction({ id: "play", label: "Play", disabled: "Drive is not connected" }, summary(), ctx);
    expect(ctx.play).not.toHaveBeenCalled();
  });

  test("watched, love, details and fix reach their handlers", async () => {
    const ctx = context();
    const s = summary({ id: 4 });
    await runAction({ id: "watched", label: "" }, s, ctx);
    await runAction({ id: "unwatched", label: "" }, s, ctx);
    await runAction({ id: "love", label: "" }, s, ctx);
    await runAction({ id: "unlove", label: "" }, s, ctx);
    await runAction({ id: "details", label: "" }, s, ctx);
    await runAction({ id: "fix", label: "" }, s, ctx);
    expect(ctx.setWatched.mock.calls.map((c) => c[0])).toEqual([
      { id: 4, watched: true },
      { id: 4, watched: false },
    ]);
    expect(ctx.setLoved.mock.calls.map((c) => c[0])).toEqual([
      { id: 4, loved: true },
      { id: 4, loved: false },
    ]);
    expect(ctx.open).toHaveBeenCalledWith(s);
    expect(ctx.fix).toHaveBeenCalledWith(s);
  });

  test("send to Other asks first, and only moves when confirmed", async () => {
    const no = context();
    no.confirm.mockImplementation(async () => false);
    await runAction({ id: "other", label: "" }, summary({ id: 6 }), no);
    expect(no.moveToOther).not.toHaveBeenCalled();
    const yes = context();
    await runAction({ id: "other", label: "" }, summary({ id: 6, title: "Home video" }), yes);
    expect(yes.confirm.mock.calls[0]![0]).toMatchObject({ confirm: "Send to Other" });
    expect(yes.moveToOther).toHaveBeenCalledWith({ id: 6 });
  });

  test("a failure is reported in plain words, not thrown", async () => {
    const ctx = context();
    ctx.play.mockImplementation(async () => {
      throw { kind: "source_offline", message: "source_offline: D:\\ is not connected" };
    });
    await runAction({ id: "play", label: "Play" }, summary(), ctx);
    expect(ctx.fail).toHaveBeenCalledWith("D:\\ is not connected");
  });
});
