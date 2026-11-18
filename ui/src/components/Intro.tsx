// The intro. As the window opens, the E of the mark swings round out of the dark, turned away from
// the lamp and out of focus, and comes to rest lit, its shadow following it on the wall below; it
// creeps toward the viewer, then zooms through:
// the wall around it is a surface with an E-shaped hole, the lit E over the hole fades so Home
// shows through the letter, and the surface flies past until Home fills the window.
// It plays while the library opens, so it adds little to startup. Any press skips it, and the
// press goes no further. The shell says whether to play it (`window.__exhibitIntro`).

import { useIsFetching, useQueryClient } from "@tanstack/react-query";
import { useAnimate, useReducedMotion, type AnimationPlaybackControls } from "motion/react";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { claimInput } from "@/focus/input/claim";
import { curve, easeOut, length, numberToken, rems, remPx, seconds } from "@/lib/motion";
import { useBoot } from "@/lib/queries";

// The E of the app icon, 60 units wide and 100 tall: the spine and three arms.
const E = "M0 0H57.6V18.1H22.4V40.5H55.2V58.6H22.4V81.4H60V100H0Z";
// The wall: far past any window at the smallest scale, with the E cut out of it.
const WALL = `M-20000 -20000H20060V20100H-20000Z${E}`;
// The surface flies toward the middle of the middle arm: its centre, and its reach from there.
const ARM = { x: 38.8, y: 49.55, half: 9.05, left: 38.8, right: 16.4 };

export function Intro() {
  const [playing, setPlaying] = useState(() => window.__exhibitIntro === true);
  if (!playing) return null;
  return <Opening onDone={() => setPlaying(false)} />;
}

/**
 * True once Home has what it shows: the library has opened (or failed to) and no query has been
 * in flight for two frames, as Home starts its queries a frame after the library opens. Never
 * later than `--delay-intro-cap`; Home has its own loading state for a library that slow.
 */
function useHomeReady(): boolean {
  const boot = useBoot();
  const client = useQueryClient();
  const fetching = useIsFetching();
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const cap = setTimeout(() => setReady(true), seconds("--delay-intro-cap") * 1000);
    return () => clearTimeout(cap);
  }, []);
  useEffect(() => {
    if (ready || boot.isPending || fetching > 0) return;
    let raf = requestAnimationFrame(() => {
      raf = requestAnimationFrame(() => {
        if (client.isFetching() === 0) setReady(true);
      });
    });
    return () => cancelAnimationFrame(raf);
  }, [ready, boot.isPending, fetching, client]);
  return ready;
}

/** How far the surface must scale about the middle arm for the hole to cover the window. */
function throughScale(box: HTMLElement): number {
  const u = box.offsetHeight / 100;
  const ox = innerWidth / 2 + (ARM.x - 30) * u;
  const oy = innerHeight / 2 + (ARM.y - 50) * u;
  return (
    1.08 *
    Math.max(
      Math.max(oy, innerHeight - oy) / (ARM.half * u),
      (innerWidth - ox) / (ARM.right * u),
      ox / (ARM.left * u),
    )
  );
}

function Opening({ onDone }: { onDone: () => void }) {
  const [scope, animate] = useAnimate<HTMLDivElement>();
  const reduce = useReducedMotion() ?? false;
  const ready = useHomeReady();
  const [lit, setLit] = useState(false);
  const phase = useRef<"light" | "open">("light");
  // What a press completes: the spawn while it plays, then the zoom. The creep is left to the zoom.
  const running = useRef<AnimationPlaybackControls[]>([]);
  const clip = useId();

  useLayoutEffect(
    () =>
      claimInput(() => {
        for (const c of running.current) c.complete();
        if (phase.current === "light") setLit(true);
      }),
    [],
  );

  useEffect(() => {
    if (reduce) {
      running.current = [animate("[data-part=mark]", { opacity: [0, 1] }, { duration: seconds("--dur-reduced") })];
      const gate = setTimeout(() => setLit(true), seconds("--dur-intro-spawn") * 1000);
      return () => clearTimeout(gate);
    }
    const out = easeOut();
    const spawn = seconds("--dur-intro-spawn");
    const spring = {
      type: "spring" as const,
      stiffness: numberToken("--intro-stiffness"),
      damping: numberToken("--intro-damping"),
    };
    const pool = seconds("--dur-intro-pool");
    // The E and its shadow turn as one; the shadow stays on the wall, so it drops and softens as
    // the E comes away from it, and darkens only as the lamp's pool rises.
    const turn = {
      scale: [numberToken("--intro-from"), 1],
      rotateX: [numberToken("--intro-tilt-x"), 0],
      rotateY: [numberToken("--intro-tilt-y"), 0],
      rotate: [numberToken("--intro-tilt-z"), 0],
    };
    running.current = [
      animate(
        "[data-part=mark]",
        { ...turn, opacity: [0, 1], filter: [length("--intro-spawn-focus"), length("--intro-rest-focus")] },
        { ...spring, opacity: { duration: spawn * 0.5, ease: out }, filter: { duration: spawn, ease: out } },
      ),
      animate(
        "[data-part=shadow]",
        {
          ...turn,
          y: ["0rem", length("--intro-shadow-drop")],
          opacity: [0, 1],
          filter: [length("--intro-shadow-sharp"), length("--intro-shadow-soft")],
        },
        { ...spring, opacity: { duration: pool, ease: out }, filter: { duration: spawn, ease: out } },
      ),
      animate("[data-part=light]", { opacity: [0, 1] }, { duration: pool, ease: out }),
      animate(
        "[data-part=glint]",
        { x: [-60, 100] },
        { delay: seconds("--delay-intro-glint"), duration: seconds("--dur-intro-glint"), ease: "easeInOut" },
      ),
    ];
    // A slow, steady push toward the viewer, once the E has swung in; the zoom takes over from it.
    animate(
      "[data-part=surface]",
      { scale: [1, numberToken("--intro-creep")] },
      { delay: spawn * 0.5, duration: seconds("--dur-intro-creep"), ease: "linear" },
    );
    const gate = setTimeout(() => setLit(true), (spawn + seconds("--dur-intro-creep-min")) * 1000);
    return () => clearTimeout(gate);
  }, [animate, reduce]);

  useEffect(() => {
    if (!lit || !ready || phase.current === "open") return;
    phase.current = "open";
    const reveal = seconds("--dur-intro-reveal");
    const open = reduce
      ? [animate(scope.current, { opacity: 0 }, { duration: reveal })]
      : [
          animate(
            "[data-part=mark], [data-part=shadow], [data-part=light], [data-part=under]",
            { opacity: 0 },
            { duration: reveal },
          ),
          animate(
            "[data-part=surface]",
            { scale: throughScale(scope.current.querySelector<HTMLElement>("[data-part=surface]")!) },
            { duration: seconds("--dur-intro-open"), ease: curve("--curve-intro-open") },
          ),
        ];
    running.current = open;
    void Promise.all(open).then(onDone);
  }, [lit, ready, reduce, animate, scope, onDone]);

  return (
    <div ref={scope} aria-hidden data-slot="intro" className="fixed inset-0 z-[100] cursor-none overflow-hidden">
      {/* The surface only ever scales flat, so the hole stays true; the E turns in depth inside it. */}
      <div
        data-part="surface"
        className="absolute inset-0 m-auto h-(--intro-mark) w-[calc(var(--intro-mark)*0.6)] will-change-transform"
        style={{ transformOrigin: `${(ARM.x / 60) * 100}% ${ARM.y}%`, perspective: rems("--intro-depth") * remPx() }}
      >
        <svg viewBox="0 0 60 100" className="absolute inset-0 size-full overflow-visible">
          <path d={WALL} fillRule="evenodd" className="fill-wall" />
        </svg>
        {/* The hole is wall until the reveal, lit like the wall around it; a stroke covers its edge. */}
        <svg data-part="under" viewBox="0 0 60 100" className="absolute inset-0 size-full overflow-visible">
          <path d={E} className="fill-wall stroke-wall" strokeWidth={0.6} />
        </svg>
        <div
          data-part="light"
          className="pointer-events-none absolute -inset-x-[260%] -inset-y-[120%] bg-[radial-gradient(closest-side_at_var(--intro-pool-at),var(--intro-pool),transparent)] opacity-0"
        />
        <svg
          data-part="shadow"
          viewBox="0 0 60 100"
          className="absolute inset-0 size-full overflow-visible opacity-0 will-change-transform"
        >
          <path d={E} className="fill-(--intro-shadow)" />
        </svg>
        <div data-part="mark" className="absolute inset-0 opacity-0 will-change-transform">
          <svg viewBox="0 0 60 100" className="absolute inset-0 size-full overflow-visible [filter:var(--intro-bloom)]">
            <path d={E} className="fill-(--intro-halo)" />
          </svg>
          <svg viewBox="0 0 60 100" className="absolute inset-0 size-full overflow-visible">
            <defs>
              <clipPath id={clip}>
                <path d={E} />
              </clipPath>
              <linearGradient id={`${clip}-face`} x1="0" x2="0" y1="0" y2="1">
                <stop offset="0" stopColor="var(--intro-face-top)" />
                <stop offset="1" stopColor="var(--intro-face-bottom)" />
              </linearGradient>
              <linearGradient id={`${clip}-glint`} x1="0" x2="1" y1="0" y2="0">
                <stop offset="0" stopColor="var(--intro-glint)" stopOpacity={0} />
                <stop offset="0.5" stopColor="var(--intro-glint)" />
                <stop offset="1" stopColor="var(--intro-glint)" stopOpacity={0} />
              </linearGradient>
            </defs>
            <path d={E} fill={`url(#${clip}-face)`} stroke={`url(#${clip}-face)`} strokeWidth={0.6} />
            <g clipPath={`url(#${clip})`}>
              <g transform="skewX(-18)">
                <rect
                  data-part="glint"
                  y={-20}
                  width={28}
                  height={140}
                  fill={`url(#${clip}-glint)`}
                  className="mix-blend-plus-lighter"
                />
              </g>
            </g>
          </svg>
        </div>
      </div>
    </div>
  );
}
