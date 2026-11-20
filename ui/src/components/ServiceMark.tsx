// A service's mark, in one colour, like FormatMark: the shape the adapter supplies, filled with the
// colour of the text around it. The service decides its own mark (`mark_svg`); the window only
// draws it, so a new adapter brings its mark without a new file here.

import { useMemo, type CSSProperties } from "react";
import { useServices } from "@/lib/queries";
import { cn } from "@/lib/utils";

const HEIGHT = {
  hero: "h-(--mark-hero)",
  panel: "h-(--mark-panel)",
  line: "h-(--mark-line)",
};

/** The mark's width over its height, from its viewBox; square when it has none. */
function ratioOf(svg: string): number {
  const box = /viewBox\s*=\s*["']\s*[-\d.e]+[\s,]+[-\d.e]+[\s,]+([\d.e]+)[\s,]+([\d.e]+)/i.exec(svg);
  const w = Number(box?.[1]);
  const h = Number(box?.[2]);
  return w > 0 && h > 0 ? w / h : 1;
}

/** `service` is the adapter's id; nothing is drawn until the services are known and list it. */
export function ServiceMark({ service, size }: { service: string; size: "hero" | "panel" | "line" }) {
  const found = useServices().data?.find((s) => s.id === service);
  const art = useMemo(
    () => (found ? { src: `url("data:image/svg+xml,${encodeURIComponent(found.markSvg)}")`, ratio: ratioOf(found.markSvg) } : null),
    [found?.markSvg],
  );
  if (!found || !art) return null;
  return (
    <span
      role="img"
      aria-label={found.name}
      data-service={found.id}
      className={cn("inline-block shrink-0 bg-current [mask:var(--art)_center/contain_no-repeat]", HEIGHT[size])}
      style={{ "--art": art.src, aspectRatio: art.ratio } as CSSProperties}
    />
  );
}
