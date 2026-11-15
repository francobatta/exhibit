// Preloaded by `bun test` (bunfig.toml): a DOM, and a stand-in for the Tauri page bridge so the
// real `@tauri-apps/api` `invoke` and `listen` run against handlers a test registers.

import { GlobalRegistrator } from "@happy-dom/global-registrator";
import { afterEach } from "bun:test";

GlobalRegistrator.register({ url: "http://localhost:1420/" });
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

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
