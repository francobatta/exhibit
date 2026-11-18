// A claim on all input: while one is held, every press (a key, a pad button, a click) goes to its
// handler and nowhere else, so the press that skips the intro never also acts on the screen under
// it. Keys and the pointer are caught at the window in the capture phase, ahead of every other
// listener, as these listeners are added when the module loads. The pad asks `takeClaim` first.

let claim: (() => void) | null = null;

/** Holds every press for `onPress` until the returned release runs. */
export function claimInput(onPress: () => void): () => void {
  const mine = onPress;
  claim = mine;
  return () => {
    if (claim === mine) claim = null;
  };
}

/** Gives a press to the claim, if one is held; true when it took it. */
export function takeClaim(): boolean {
  if (claim === null) return false;
  claim();
  return true;
}

// Alt+F4, Win and the other system chords stay the system's.
const SYSTEM = new Set(["Alt", "Control", "Meta", "Shift", "OS", "AltGraph"]);

function onKey(e: KeyboardEvent) {
  if (claim === null || SYSTEM.has(e.key) || e.altKey || e.metaKey || e.ctrlKey) return;
  e.preventDefault();
  e.stopImmediatePropagation();
  if (e.type === "keydown" && !e.repeat) takeClaim();
}

function onPointer(e: Event) {
  if (claim === null) return;
  e.preventDefault();
  e.stopImmediatePropagation();
  if (e.type === "pointerdown") takeClaim();
}

window.addEventListener("keydown", onKey, true);
window.addEventListener("keyup", onKey, true);
for (const type of ["pointerdown", "pointerup", "click", "contextmenu", "wheel"]) {
  window.addEventListener(type, onPointer, { capture: true, passive: false });
}
