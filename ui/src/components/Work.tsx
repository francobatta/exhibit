// A work hung on the wall: its poster (2:3) or its frame (16:9), or, with no artwork, a
// typographic poster. Unlit works sit dimmed; the focused one is at full brightness, slightly
// larger, with a pool of lamp light above and behind it and a lamp-white keyline. The light is one
// shared Motion element per `light` id, so it glides from work to work. Purely presentational: the
// shelf or grid around it owns focus and tells it `focused`.

import { motion } from "motion/react";
import { memo, useState } from "react";
import { imageUrl, type ItemSummary } from "@/lib/api";
import { OFFLINE, WATCHED, episodeLabel, facts, started } from "@/lib/format";
import { focusSpring, numberToken } from "@/lib/motion";
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
  /**
   * What a work with no artwork sets in place of its title, when the title already hangs beside
   * it (an episode's number on the show screen).
   */
  mark?: string;
}

/** How a work reads aloud and in its tooltip: "Taxi Driver, 1976, Watched". */
export function workLabel(item: ItemSummary): string {
  const place =
    item.kind === "episode" ? facts([item.show_title, episodeLabel(item.season, item.episode)]) : null;
  return facts([
    item.title,
    place,
    item.year === null ? null : String(item.year),
    item.watched ? WATCHED : null,
    item.available ? null : OFFLINE,
  ]);
}

/** Height over width: posters are 2:3, wide works 16:9. Shelves and walls size rows from it. */
export const ASPECT: Record<WorkShape, number> = { poster: 3 / 2, wide: 9 / 16 };
const RATIO: Record<WorkShape, string> = { poster: "aspect-[2/3]", wide: "aspect-video" };
/** The intrinsic size given to an <img>, so the browser reserves its box before it decodes. */
const IMAGE_WIDTH = 300;

function artwork(item: ItemSummary, shape: WorkShape): string | null {
  return shape === "poster" ? imageUrl(item.poster, "s") : imageUrl(item.backdrop);
}

function Typographic({ item, shape, mark }: { item: ItemSummary; shape: WorkShape; mark?: string }) {
  return (
    <div
      data-typographic
      className="absolute inset-0 flex items-end rounded-work bg-wall-raised p-[8cqi] ring-1 ring-line ring-inset"
    >
      <p
        className={cn(
          "text-left font-display font-poster text-label [font-stretch:var(--stretch-poster)] tracking-title",
          "[overflow-wrap:anywhere] text-pretty",
          mark !== undefined
            ? "text-[14cqi] leading-none"
            : shape === "poster"
              ? "line-clamp-6 text-[12cqi] leading-[1.02]"
              : "line-clamp-3 text-[7cqi] leading-[1.05]",
        )}
      >
        {mark ?? item.title}
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

export const Work = memo(function Work({ item, shape, focused, size, light = "work-light", selected, mark }: WorkProps) {
  const src = artwork(item, shape);
  const [failed, setFailed] = useState<string | null>(null);
  const showArt = src !== null && failed !== src;
  const progress = started(item.watched, item.progress);
  const spring = focusSpring();
  const scale = numberToken("--work-focus-scale");

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
          // A chosen work stays lit, so a selection reads at a glance.
          item.available ? (focused || selected ? "" : "[filter:var(--work-dim)]") : focused ? "grayscale" : "[filter:var(--offline)]",
        )}
        initial={false}
        animate={{ scale: focused ? scale : 1 }}
        transition={spring}
      >
        {showArt ? (
          <img
            src={src}
            alt=""
            width={IMAGE_WIDTH}
            height={Math.round(IMAGE_WIDTH * ASPECT[shape])}
            loading="lazy"
            decoding="async"
            draggable={false}
            onError={() => setFailed(src)}
            className="absolute inset-0 size-full object-cover"
          />
        ) : (
          <Typographic item={item} shape={shape} mark={mark} />
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
          <div
            data-selected
            aria-hidden
            className="absolute top-[5cqi] left-[5cqi] size-[10cqi] rounded-full bg-lamp p-[1.5cqi] text-wall"
          >
            <Check />
          </div>
        )}
      </motion.div>
      {focused && (
        // Outside the poster so its clipping cannot hide the light on its way in; the inset frames the focused scale.
        <motion.div
          data-light
          layoutId={`${light}-key`}
          aria-hidden
          transition={spring}
          className="pointer-events-none absolute inset-(--work-focus-inset) rounded-work shadow-[inset_0_0_0_var(--hairline)_var(--lamp)]"
        />
      )}
    </div>
  );
});
