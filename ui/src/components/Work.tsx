// A work hung on the wall: its poster (2:3) or its frame (16:9), or, with no artwork, a
// typographic poster. Unlit works sit dimmed; the focused one is at full brightness, slightly
// larger, with a pool of lamp light above and behind it and a lamp-white keyline. The light is one
// shared Motion element per `light` id, so it glides from work to work. Purely presentational: the
// shelf or grid around it owns focus and tells it `focused`.

import { motion } from "motion/react";
import { memo, useState } from "react";
import { imageUrl, type ItemSummary } from "@/lib/api";
import { episodeLabel, facts } from "@/lib/format";
import { focusSpring } from "@/lib/motion";
import { cn } from "@/lib/utils";

export type WorkShape = "poster" | "wide";

export interface WorkProps {
  item: ItemSummary;
  shape: WorkShape;
  focused: boolean;
  /** Width in rem. Without it the work fills its container's width. */
  size?: number;
  /** The shared light's id; works sharing it pass one light between them. */
  light?: string;
  /** Marked as chosen, for walls that pick several. */
  selected?: boolean;
}

/** How a work reads aloud and in its tooltip: "Taxi Driver, 1976, Watched". */
export function workLabel(item: ItemSummary): string {
  const place =
    item.kind === "episode" ? facts([item.show_title, episodeLabel(item.season, item.episode)]) : null;
  return facts([
    item.title,
    place,
    item.year === null ? null : String(item.year),
    item.watched ? "Watched" : null,
    item.available ? null : "Offline",
  ]);
}

const RATIO: Record<WorkShape, string> = { poster: "aspect-[2/3]", wide: "aspect-video" };
// Intrinsic sizes for the <img>, so the browser reserves the box before it decodes.
const PIXELS: Record<WorkShape, [number, number]> = { poster: [300, 450], wide: [480, 270] };

function artwork(item: ItemSummary, shape: WorkShape): string | null {
  return shape === "poster" ? imageUrl(item.poster, "s") : imageUrl(item.backdrop);
}

function Typographic({ item, shape }: { item: ItemSummary; shape: WorkShape }) {
  return (
    <div data-typographic className="absolute inset-0 flex items-end bg-wall-raised p-[8cqi]">
      <p
        className={cn(
          "font-display font-poster text-label [font-stretch:var(--stretch-poster)] tracking-title",
          "[overflow-wrap:anywhere] text-pretty",
          shape === "poster" ? "line-clamp-6 text-[12cqi] leading-[1.02]" : "line-clamp-3 text-[7cqi] leading-[1.05]",
        )}
      >
        {item.title}
      </p>
    </div>
  );
}

function Check() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden className="size-full">
      <path d="M4 8.4 6.8 11 12 5.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export const Work = memo(function Work({ item, shape, focused, size, light = "work-light", selected }: WorkProps) {
  const src = artwork(item, shape);
  const [failed, setFailed] = useState<string | null>(null);
  const showArt = src !== null && failed !== src;
  const progress = !item.watched && item.progress !== null && item.progress > 0 ? Math.min(1, item.progress) : null;
  const spring = focusSpring();
  const [w, h] = PIXELS[shape];

  return (
    <div
      data-work
      data-lit={focused ? "" : undefined}
      data-offline={item.available ? undefined : ""}
      className="@container relative isolate"
      style={size === undefined ? undefined : { width: `${size}rem` }}
    >
      {focused && (
        <motion.div
          layoutId={`${light}-pool`}
          aria-hidden
          transition={spring}
          className="work-pool pointer-events-none absolute -inset-x-[40%] -top-[22%] bottom-[10%] -z-10"
        />
      )}
      <motion.div
        className={cn(
          "relative overflow-hidden rounded-work bg-wall-raised",
          "transition-[filter] duration-(--dur-screen) ease-out",
          RATIO[shape],
          item.available ? (focused ? "" : "[filter:var(--work-dim)]") : focused ? "grayscale" : "[filter:var(--offline)]",
        )}
        initial={false}
        animate={{ scale: focused ? 1.06 : 1 }}
        transition={spring}
      >
        {showArt ? (
          <img
            src={src}
            alt=""
            width={w}
            height={h}
            loading="lazy"
            decoding="async"
            draggable={false}
            onError={() => setFailed(src)}
            className="absolute inset-0 size-full object-cover"
          />
        ) : (
          <Typographic item={item} shape={shape} />
        )}
        {progress !== null && (
          <div className="absolute inset-x-[6cqi] bottom-[5cqi] h-[1.6cqi] overflow-hidden rounded-full bg-wall/70">
            <div data-progress className="h-full rounded-full bg-lamp" style={{ width: `${Math.round(progress * 100)}%` }} />
          </div>
        )}
        {item.watched && (
          <div
            data-watched
            aria-hidden
            className="absolute top-[5cqi] right-[5cqi] size-[10cqi] rounded-full bg-wall/75 p-[1.5cqi] text-label-dim"
          >
            <Check />
          </div>
        )}
        {selected && (
          <div aria-hidden className="absolute top-[5cqi] left-[5cqi] size-[4cqi] rounded-full bg-lamp" />
        )}
      </motion.div>
      {focused && (
        // Outside the poster so its clipping cannot hide the light on its way in; the insets match the 1.06 scale.
        <motion.div
          data-light
          layoutId={`${light}-key`}
          aria-hidden
          transition={spring}
          className="pointer-events-none absolute -inset-[3%] rounded-work shadow-[inset_0_0_0_1px_var(--lamp)]"
        />
      )}
    </div>
  );
});
