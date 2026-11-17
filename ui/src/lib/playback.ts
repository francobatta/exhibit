// Playing a work: Exhibit hands it to mpv and steps aside. While mpv runs the app ignores input
// and says "Playing in mpv" (components/Playing.tsx); when mpv exits the library is refreshed and
// focus is back exactly where it was, or, on the show being watched, on its next episode.

import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { create } from "zustand";
import { toast } from "@/components/ui/toast";
import { openSettings } from "@/nav/Screens";
import { useNav } from "@/nav/stack";
import * as api from "./api";
import { isApiError, plain, type Id, type PlayEnded } from "./api";
import { keys } from "./queries";

export interface PlaybackState {
  /** The work handed to mpv, until mpv exits. */
  playing: Id | null;
  /** The episode the Show screen should focus once the app is back (the next one of its show). */
  follow: Id | null;
  /** The next episode reported as mpv exited, kept until the "Playing in mpv" layer has gone. */
  next: Id | null;
  /** Plays `id` in mpv, from where it stopped with `resume`. Never rejects: a failure is a toast. */
  start(id: Id, resume: boolean): Promise<void>;
  /** The layer has gone and focus is back where it was: the next episode may now take it. */
  returned(): void;
  /** The Show screen has dealt with `follow`. */
  followed(): void;
}

export const usePlayback = create<PlaybackState>()((set, get) => ({
  playing: null,
  follow: null,
  next: null,
  start: async (id, resume) => {
    if (get().playing !== null) return;
    set({ playing: id, follow: null, next: null });
    try {
      // Resolves once mpv has started; the end arrives as `play-ended`.
      await api.play(id, resume);
    } catch (e) {
      set({ playing: null });
      playFailed(e);
    }
  },
  returned: () => set((s) => ({ follow: s.next, next: null })),
  followed: () => set({ follow: null }),
}));

/** Says why playing failed. A missing or broken player offers Settings, where its path is set. */
export function playFailed(e: unknown): void {
  if (isApiError(e) && e.kind === "tool") {
    toast.error({
      title: "mpv could not start",
      description: plain(e),
      action: { label: "Open Settings", onAction: () => openSettings() },
    });
    return;
  }
  toast.error(plain(e));
}

function ended(p: PlayEnded, client: QueryClient): void {
  void client.invalidateQueries({ queryKey: keys.library });
  if (p.error !== null) playFailed(p.error);
  // Only the show's own screen follows on to the next episode; anywhere else focus stays put.
  const onShow = useNav.getState().current.screen === "show";
  usePlayback.setState({ playing: null, next: onShow ? p.nextEpisode : null });
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
