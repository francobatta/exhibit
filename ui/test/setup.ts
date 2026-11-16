// Preloaded by `bun test` (bunfig.toml): a DOM, and a stand-in for the Tauri page bridge so the
// real `@tauri-apps/api` `invoke` and `listen` run against handlers a test registers.

import { GlobalRegistrator } from "@happy-dom/global-registrator";
import { afterEach } from "bun:test";

GlobalRegistrator.register({ url: "http://localhost:1420/" });
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The design tokens, as the app's stylesheet gives them to :root. happy-dom does not read custom
// properties from style sheets, so they are set inline; components read sizes and the focus scale.
{
  const { readFileSync } = await import("node:fs");
  const css = readFileSync(new URL("../src/styles/tokens.css", import.meta.url), "utf8");
  const root = css.slice(css.indexOf(":root {"), css.indexOf("@theme inline {"));
  // Durations are zero: motion in tests is instant.
  for (const m of root.matchAll(/(--[\w-]+):\s*([^;]+);/g)) {
    const value = /^--(dur|delay)-/.test(m[1]!) ? "0ms" : m[2]!.trim();
    document.documentElement.style.setProperty(m[1]!, value);
  }
  // Chromium's root font size follows `html { font-size: calc(100vw / 120) }`; happy-dom has none.
  document.documentElement.style.fontSize = "16px";
}

// happy-dom has no top layer; UI SDK's modal layers (FocusScope modal) need these to mount.
HTMLElement.prototype.showPopover ??= function () {};
HTMLElement.prototype.hidePopover ??= function () {};
Element.prototype.getAnimations ??= () => [];

// What WebView2 has and happy-dom lacks. Scrolling moves the position at once and fires `scroll`,
// as Chromium does at the end of a smooth scroll, so virtualised lists render what scrolled in.
Element.prototype.scrollIntoView = function () {};
Element.prototype.scrollTo = function (this: Element, a?: ScrollToOptions | number, b?: number) {
  const o = typeof a === "number" ? { left: a, top: b } : (a ?? {});
  if (o.left !== undefined) this.scrollLeft = o.left;
  if (o.top !== undefined) this.scrollTop = o.top;
  this.dispatchEvent(new Event("scroll"));
} as Element["scrollTo"];
Object.defineProperty(navigator, "getGamepads", { configurable: true, value: () => [] });
// A FontFaceSet is an EventTarget; UI SDK's popup positioning listens to it.
const fonts = Object.assign(new EventTarget(), { ready: Promise.resolve(), status: "loaded" });
Object.defineProperty(document, "fonts", { configurable: true, value: fonts });

const { mockIPC, mockConvertFileSrc } = await import("@tauri-apps/api/mocks");
const { emit } = await import("@tauri-apps/api/event");

type Handler = (args: Record<string, unknown>) => unknown;

const handlers = new Map<string, Handler>();
const calls: Array<{ cmd: string; args: Record<string, unknown> }> = [];

mockIPC(
  (cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    const h = handlers.get(cmd);
    if (h === undefined) throw { kind: "invalid", message: `invalid: no fake for ${cmd}` };
    return h(args);
  },
  { shouldMockEvents: true },
);
mockConvertFileSrc("windows");

/** The fake bridge: answer commands, read what was sent, fire events. */
export const tauri = {
  /** Answers `cmd` with `h`; a throw (or rejected promise) becomes the command's rejection. */
  handle(cmd: string, h: Handler): void {
    handlers.set(cmd, h);
  },
  /** Commands invoked so far, oldest first, excluding the event plugin's own. */
  calls(): Array<{ cmd: string; args: Record<string, unknown> }> {
    return calls.filter((c) => !c.cmd.startsWith("plugin:event|"));
  },
  /** Delivers an event to every `listen` callback for it. */
  emit(event: string, payload: unknown = null): Promise<void> {
    return emit(event, payload);
  },
  reset(): void {
    handlers.clear();
    calls.length = 0;
  },
};

afterEach(() => tauri.reset());
