// Drives a dev window started with `scripts/dev.ps1 -Cdp` over WebView2's DevTools protocol.
//
//   bun scripts/cdp.ts shot <out.jpg>          1920x1080 JPEG of the page
//   bun scripts/cdp.ts shot <out.jpg> --window JPEG at the window's own size and scale
//   bun scripts/cdp.ts keys <key> [<key> ...]  trusted key presses, e.g. ArrowRight Enter Escape m /
//   bun scripts/cdp.ts eval "<js expression>"  prints the value
//
// Port: CDP_PORT (default 9333). Keys go through Input.dispatchKeyEvent, so the app sees them
// exactly as it sees a keyboard; the gamepad reaches the app as the same keys.

const port = process.env.CDP_PORT ?? "9333";

type Target = { type: string; url: string; webSocketDebuggerUrl: string };

const KEYS: Record<string, { code: string; keyCode: number }> = {
  ArrowLeft: { code: "ArrowLeft", keyCode: 37 },
  ArrowUp: { code: "ArrowUp", keyCode: 38 },
  ArrowRight: { code: "ArrowRight", keyCode: 39 },
  ArrowDown: { code: "ArrowDown", keyCode: 40 },
  Enter: { code: "Enter", keyCode: 13 },
  Escape: { code: "Escape", keyCode: 27 },
  Backspace: { code: "Backspace", keyCode: 8 },
  Tab: { code: "Tab", keyCode: 9 },
  " ": { code: "Space", keyCode: 32 },
  "/": { code: "Slash", keyCode: 191 },
  "[": { code: "BracketLeft", keyCode: 219 },
  "]": { code: "BracketRight", keyCode: 221 },
};

function keyInfo(key: string): { code: string; keyCode: number; text?: string } {
  const named = KEYS[key];
  if (named) return { ...named, text: key.length === 1 ? key : undefined };
  if (/^[a-z0-9]$/i.test(key)) {
    const upper = key.toUpperCase();
    const code = /\d/.test(key) ? `Digit${key}` : `Key${upper}`;
    return { code, keyCode: upper.charCodeAt(0), text: key };
  }
  throw new Error(`Unknown key "${key}"`);
}

async function connect(): Promise<(method: string, params?: object) => Promise<any>> {
  const targets: Target[] = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = targets.find((t) => t.type === "page" && t.url.startsWith("http://localhost:1420"));
  if (!page) throw new Error(`No Exhibit page on CDP port ${port}. Start it with scripts/dev.ps1 -Cdp.`);
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((ok, fail) => {
    ws.onopen = ok;
    ws.onerror = fail;
  });
  let next = 0;
  const waiting = new Map<number, { ok: (v: any) => void; fail: (e: Error) => void }>();
  ws.onmessage = (e) => {
    const msg = JSON.parse(String(e.data));
    const w = waiting.get(msg.id);
    if (!w) return;
    waiting.delete(msg.id);
    if (msg.error) w.fail(new Error(msg.error.message));
    else w.ok(msg.result);
  };
  return (method, params = {}) =>
    new Promise((ok, fail) => {
      const id = ++next;
      waiting.set(id, { ok, fail });
      ws.send(JSON.stringify({ id, method, params }));
    });
}

const [cmd, ...args] = process.argv.slice(2);
const send = await connect();

if (cmd === "shot") {
  const out = args.find((a) => !a.startsWith("--")) ?? "shot.jpg";
  // --window keeps the window's own size and scale, to see how the layout meets it.
  const asIs = args.includes("--window");
  if (!asIs) {
    await send("Emulation.setDeviceMetricsOverride", {
      width: 1920,
      height: 1080,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await Bun.sleep(300);
  }
  const { data } = await send("Page.captureScreenshot", { format: "jpeg", quality: 85 });
  if (!asIs) await send("Emulation.clearDeviceMetricsOverride");
  await Bun.write(out, Buffer.from(data, "base64"));
  console.log(out);
} else if (cmd === "keys") {
  for (const key of args) {
    const k = keyInfo(key);
    const base = { key, code: k.code, windowsVirtualKeyCode: k.keyCode };
    await send("Input.dispatchKeyEvent", { type: "keyDown", ...base, text: k.text });
    await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
    await Bun.sleep(180);
  }
} else if (cmd === "eval") {
  const { result } = await send("Runtime.evaluate", {
    expression: args.join(" "),
    returnByValue: true,
    awaitPromise: true,
  });
  console.log(JSON.stringify(result.value, null, 2));
} else {
  console.error("Usage: bun scripts/cdp.ts shot <out.jpg> | keys <key...> | eval <js>");
  process.exit(1);
}
process.exit(0);
