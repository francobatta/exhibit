// A format's mark, in one colour: the logo's shape, filled with the colour of the text around it,
// so it sits on the wall like the words of the placard. The logos come from Wikimedia Commons,
// trimmed to their ink (see assets/marks/SOURCES.md), and are sized by height.

import type { CSSProperties } from "react";
import dolbyAtmos from "@/assets/marks/dolby-atmos.svg";
import dolbyDigital from "@/assets/marks/dolby-digital.svg";
import dolbyDigitalPlus from "@/assets/marks/dolby-digital-plus.svg";
import dolbyTrueHd from "@/assets/marks/dolby-truehd.svg";
import dolbyVision from "@/assets/marks/dolby-vision.svg";
import dts from "@/assets/marks/dts.svg";
import dtsHdMa from "@/assets/marks/dts-hd-ma.svg";
import dtsX from "@/assets/marks/dts-x.png";
import hd1080p from "@/assets/marks/hd-1080p.svg";
import hd720p from "@/assets/marks/hd-720p.svg";
import hdr10 from "@/assets/marks/hdr10.svg";
import sd from "@/assets/marks/sd.svg";
import ultraHd from "@/assets/marks/ultra-hd.svg";
import type { Mark, MarkId } from "@/lib/marks";
import { cn } from "@/lib/utils";

/**
 * Each mark's file, its width over its height (from its viewBox), and whether it is drawn tall:
 * a square mark's lettering is small, so it stands higher than a wide one to read as well.
 */
const ART: Record<MarkId, { src: string; ratio: number; tall?: true }> = {
  "ultra-hd": { src: ultraHd, ratio: 195.21 / 76.05 },
  "hd-1080p": { src: hd1080p, ratio: 124.49 / 124.81, tall: true },
  "hd-720p": { src: hd720p, ratio: 168.01 / 121.01, tall: true },
  sd: { src: sd, ratio: 1, tall: true },
  hdr10: { src: hdr10, ratio: 283.22 / 98.7 },
  "dolby-vision": { src: dolbyVision, ratio: 1045.79 / 388.32 },
  "dolby-truehd": { src: dolbyTrueHd, ratio: 219.87 / 55.04 },
  "dolby-digital-plus": { src: dolbyDigitalPlus, ratio: 356.61 / 85.82 },
  "dolby-digital": { src: dolbyDigital, ratio: 303.39 / 76.26 },
  "dolby-atmos": { src: dolbyAtmos, ratio: 300.01 / 76.7 },
  dts: { src: dts, ratio: 344.62 / 135.14 },
  "dts-hd-ma": { src: dtsHdMa, ratio: 310.37 / 92.67 },
  "dts-x": { src: dtsX, ratio: 451 / 160 },
};

const HEIGHT = {
  hero: { wide: "h-(--mark-hero)", tall: "h-(--mark-hero-tall)" },
  panel: { wide: "h-(--mark-panel)", tall: "h-(--mark-panel-tall)" },
};

export function FormatMark({ mark, size }: { mark: Mark; size: "hero" | "panel" }) {
  const art = ART[mark.id];
  return (
    <span
      role="img"
      aria-label={mark.name}
      data-mark={mark.id}
      className={cn(
        "inline-block shrink-0 bg-current [mask:var(--art)_center/contain_no-repeat]",
        HEIGHT[size][art.tall ? "tall" : "wide"],
      )}
      style={{ "--art": `url("${art.src}")`, aspectRatio: art.ratio } as CSSProperties}
    />
  );
}
