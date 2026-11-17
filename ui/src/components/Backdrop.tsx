// The full-window layer behind every screen: the focused work's backdrop under gradients to the
// wall, and the wall's tint. Screens say what is focused with `useBackdrop.getState().show()`;
// the picture and tint follow once focus has settled, crossfading per the Design direction.

import { animate, AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { create } from "zustand";
import { imageUrl } from "@/lib/api";
import { seconds } from "@/lib/motion";
import { useTint } from "@/lib/tint";

interface BackdropState {
  /** The backdrop image name of the focused work. */
  image: string | null;
  /** Its poster image name, where the tint comes from. */
  poster: string | null;
  show(image: string | null, poster: string | null): void;
}

export const useBackdrop = create<BackdropState>()((set) => ({
  image: null,
  poster: null,
  show: (image, poster) => set({ image, poster }),
}));

/**
 * A pushed screen shows its work's backdrop while it is up, and gives back the one it found when it
 * goes, unless the screen that replaced it (which may arrive before it has finished leaving) has
 * already shown its own.
 */
export function useScreenBackdrop(image: string | null, poster: string | null): void {
  useEffect(() => {
    const before = useBackdrop.getState();
    before.show(image, poster);
    return () => {
      const now = useBackdrop.getState();
      if (now.image === image && now.poster === poster) now.show(before.image, before.poster);
    };
  }, [image, poster]);
}

/** `value`, once it has stopped changing for `delayS` seconds. */
export function useSettled<T>(value: T, delayS: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setSettled(value), delayS * 1000);
    return () => clearTimeout(t);
  }, [value, delayS]);
  return settled;
}

// The wall's tint is animated as oklch numbers so hue takes the short way round and an unknown
// tint fades to grey rather than jumping.
type Oklch = { l: number; c: number; h: number };
const NEUTRAL: Oklch = { l: 0.2, c: 0, h: 0 };

function parse(tint: string | null, from: Oklch): Oklch {
  const m = tint?.match(/oklch\(([\d.]+) ([\d.]+) ([\d.]+)\)/);
  if (!m) return { ...NEUTRAL, h: from.h };
  return { l: Number(m[1]), c: Number(m[2]), h: Number(m[3]) };
}

function useWallTint(tint: string | null, duration: number) {
  const now = useRef<Oklch>(NEUTRAL);
  useEffect(() => {
    const from = now.current;
    const to = parse(tint, from);
    // A grey start or end keeps the other end's hue, so only chroma moves.
    const h0 = from.c === 0 ? to.h : from.h;
    const dh = ((to.h - h0 + 540) % 360) - 180;
    const root = document.documentElement;
    const controls = animate(0, 1, {
      duration,
      ease: "easeInOut",
      onUpdate: (t) => {
        const v = { l: from.l + (to.l - from.l) * t, c: from.c + (to.c - from.c) * t, h: (h0 + dh * t + 360) % 360 };
        now.current = v;
        root.style.setProperty("--tint", `oklch(${v.l.toFixed(3)} ${v.c.toFixed(4)} ${v.h.toFixed(1)})`);
      },
    });
    return () => controls.stop();
  }, [tint, duration]);
}

export function Backdrop() {
  const image = useBackdrop((s) => s.image);
  const poster = useBackdrop((s) => s.poster);
  const delay = seconds("--delay-backdrop");
  const fade = seconds("--dur-backdrop");
  const settledImage = useSettled(image, delay);
  const settledPoster = useSettled(poster, delay);
  const tint = useTint(settledPoster);
  useWallTint(tint, fade);
  const url = imageUrl(settledImage);

  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 bg-wall-tinted">
      <AnimatePresence initial={false}>
        {url !== null && (
          <motion.div
            key={url}
            className="absolute inset-0"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: fade, ease: "easeInOut" }}
          >
            <img
              src={url}
              alt=""
              width={1920}
              height={1080}
              decoding="async"
              className="absolute top-0 right-0 h-[82%] w-[78%] object-cover object-top backdrop-mask"
            />
          </motion.div>
        )}
      </AnimatePresence>
      {/* Gradients to the wall: the left for the placard, the bottom for the shelves. */}
      <div className="absolute inset-0 backdrop-scrim" />
    </div>
  );
}
