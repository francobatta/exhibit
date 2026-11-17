// While mpv plays, the app stands still: a modal layer over everything takes no input and says
// "Playing in mpv". When mpv exits the layer closes and focus goes back to what had it (the
// Dialog returns focus only when it is inside the layer, which is why the layer takes it). A
// player that could not start is then told in a dialog whose buttons the pad reaches.

import { useEffect, useRef } from "react";
import { FocusScope } from "@/focus/react";
import { usePlayback, usePlaybackSync } from "@/lib/playback";
import { openSettings } from "@/nav/Screens";
import { Dialog } from "./Dialog";
import { dialogBackdrop } from "./ui/dialog";

const stay = () => {};

/** Mount once near the app's root. */
export function PlaybackHost() {
  usePlaybackSync();
  const playing = usePlayback((s) => s.playing);
  const returned = usePlayback((s) => s.returned);
  return (
    <>
      <FocusScope
        id="playing"
        modal
        open={playing !== null}
        label="Playing in mpv"
        // The layer holds focus itself, so no key, pad button or wheel step reaches the screen under it.
        initialFocus="content"
        onBack={stay}
        onClosed={returned}
        backdropClassName={dialogBackdrop}
        // The whole window, so the line sits alone in the middle of the dimmed wall, clear of any placard.
        className="fixed inset-0 z-50 flex items-center justify-center outline-none transition-opacity duration-(--dur-screen) data-starting-style:opacity-0 data-ending-style:opacity-0"
      >
        <p role="status" className="font-display text-lead font-ui text-label-dim">
          Playing in mpv
        </p>
      </FocusScope>
      <PlayerProblem />
    </>
  );
}

/** "mpv could not start": what was tried, and the way to Settings, where its path is set. */
function PlayerProblem() {
  const { problem, problemOpen, dismiss, cleared } = usePlayback();
  // Settings opens once the dialog has closed and left the page, as no screen opens under a modal.
  const then = useRef<(() => void) | null>(null);
  useEffect(() => {
    if (problem !== null) return;
    const next = then.current;
    then.current = null;
    next?.();
  }, [problem]);
  if (problem === null) return null;
  return (
    <Dialog
      id="player-problem"
      open={problemOpen}
      title="mpv could not start"
      body={`${problem.message}. Download it, or choose where it is, in Settings.`}
      actions={[
        {
          label: "Open Settings",
          primary: true,
          onSelect: () => {
            then.current = () => openSettings("tools");
            dismiss();
          },
        },
        { label: "Close", onSelect: dismiss },
      ]}
      onBack={dismiss}
      onClosed={cleared}
    />
  );
}
