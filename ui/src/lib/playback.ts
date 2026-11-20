// Playing a work: Exhibit hands it to mpv and steps aside. While mpv runs the app ignores input
// and says "Playing in mpv" (components/Playing.tsx); when mpv exits the library is refreshed and
// focus is back exactly where it was, or, on the show being watched, on its next episode. A
// failure is told once that layer has gone: a missing or broken player in a dialog that offers
// Settings, anything else in a toast that says what to do.

import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { create } from "zustand";
import { toast } from "@/components/ui/toast";
import { t } from "@/i18n";
import { useNav } from "@/nav/stack";
import * as api from "./api";
import { isApiError, plain, type Id, type PlayEnded } from "./api";
import { keys } from "./queries";

/** A player that could not start, as the dialog tells it. */
export interface PlayerProblem {
  /** The core's message, naming the path that was tried. */
  message: string;
}

export interface PlaybackState {
  /** The work handed to mpv, until mpv exits. */
  playing: Id | null;
  /** The episode the Show screen should focus once the app is back (the next one of its show). */
  follow: Id | null;
  /** The next episode reported as mpv exited, kept until the "Playing in mpv" layer has gone. */
  next: Id | null;
  /** Why playing failed, kept until the layer has gone. */
  failure: unknown;
  /** The player problem the dialog shows, kept while it closes; null when none. */
  problem: PlayerProblem | null;
  /** Whether the dialog is open. */
  problemOpen: boolean;
  /** Plays `id` in mpv, from where it stopped with `resume`. Never rejects: a failure is told. */
  start(id: Id, resume: boolean): Promise<void>;
  /** The layer has gone and focus is back where it was: the next episode may take it, a failure is told. */
  returned(): void;
  /** The Show screen has dealt with `follow`. */
  followed(): void;
  /** The owner has read the problem: the dialog closes. */
  dismiss(): void;
  /** The dialog has closed. */
  cleared(): void;
}

export const usePlayback = create<PlaybackState>()((set, get) => ({
  playing: null,
  follow: null,
  next: null,
  failure: null,
  problem: null,
  problemOpen: false,
  start: async (id, resume) => {
    if (get().playing !== null) return;
    set({ playing: id, follow: null, next: null, failure: null });
    try {
      // Resolves once mpv has started; the end arrives as `play-ended`.
      await api.play(id, resume);
    } catch (e) {
      set({ playing: null, failure: e });
    }
  },
  returned: () => {
    const { next, failure } = get();
    set({ follow: next, next: null, failure: null });
    if (failure !== null) tell(failure);
  },
  followed: () => set({ follow: null }),
  dismiss: () => set({ problemOpen: false }),
  cleared: () => set({ problem: null }),
}));

/** What to do about a failure that is not the player's, in plain words. */
function advice(e: unknown): string {
  switch (isApiError(e) ? e.kind : null) {
    case "source_offline":
      return t("playback.adviceOffline");
    case "not_found":
    case "io":
      return t("playback.adviceMissing");
    default:
      return t("playback.adviceRetry");
  }
}

/** Tells why playing failed: a missing or broken player in the dialog, anything else in a toast. */
export function tell(e: unknown): void {
  if (isApiError(e) && e.kind === "tool") {
    usePlayback.setState({ problem: { message: plain(e) }, problemOpen: true });
    return;
  }
  toast.error({ title: plain(e), description: advice(e) });
}

function ended(p: PlayEnded, client: QueryClient): void {
  void client.invalidateQueries({ queryKey: keys.library });
  // Only the show's own screen follows on to the next episode; anywhere else focus stays put.
  const onShow = useNav.getState().current.screen === "show";
  usePlayback.setState({ playing: null, next: onShow ? p.nextEpisode : null, failure: p.error });
}

/** Mount once: hears mpv exit. */
export function usePlaybackSync(): void {
  const client = useQueryClient();
  useEffect(() => {
    const off = api.onPlayEnded((p) => ended(p, client));
    return () => {
      void off.then((f) => f());
    };
  }, [client]);
}
