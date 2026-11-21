// A service's mark, in one colour, like FormatMark: the shape the adapter supplies, filled with the
// colour of the text around it. The service decides its own mark (`mark_svg`); the window only
// draws it, so a new adapter brings its mark without a new file here. An origin (whose score it
// is, which can differ from the service that fetched it) is drawn the same way.

import { useMemo, type CSSProperties } from "react";
import type { OriginInfo, ServiceView } from "@/lib/api";
import { useServices } from "@/lib/queries";
import { cn } from "@/lib/utils";

const HEIGHT = {
  hero: "h-(--mark-hero)",
  panel: "h-(--mark-panel)",
  line: "h-(--mark-line)",
};

type Size = keyof typeof HEIGHT;

/** The mark's width over its height, from its viewBox; square when it has none. */
function ratioOf(svg: string): number {
  const box = /viewBox\s*=\s*["']\s*[-\d.e]+[\s,]+[-\d.e]+[\s,]+([\d.e]+)[\s,]+([\d.e]+)/i.exec(svg);
  const w = Number(box?.[1]);
  const h = Number(box?.[2]);
  return w > 0 && h > 0 ? w / h : 1;
}

function Art({ svg, name, size, data }: { svg: string; name: string; size: Size; data: Record<string, string> }) {
  const art = useMemo(() => ({ src: `url("data:image/svg+xml,${encodeURIComponent(svg)}")`, ratio: ratioOf(svg) }), [svg]);
  return (
    <span
      role="img"
      aria-label={name}
      {...data}
      className={cn("inline-block shrink-0 bg-current [mask:var(--art)_center/contain_no-repeat]", HEIGHT[size])}
      style={{ "--art": art.src, aspectRatio: art.ratio } as CSSProperties}
    />
  );
}

/** `service` is the adapter's id; nothing is drawn until the services are known and list it. */
export function ServiceMark({ service, size }: { service: string; size: Size }) {
  const found = useServices().data?.find((s) => s.id === service);
  if (!found) return null;
  return <Art svg={found.markSvg} name={found.name} size={size} data={{ "data-service": found.id }} />;
}

/** Every origin the services report, by id, with its place in the order scores are shown. */
export function originsOf(services: ServiceView[] | undefined): Map<string, OriginInfo & { place: number }> {
  const all = (services ?? []).flatMap((s) => s.origins ?? []);
  return new Map(all.map((o, place) => [o.id, { ...o, place }]));
}

/** The mark of whose score it is (IMDb), as the service that reports it describes it. */
export function OriginMark({ origin, size }: { origin: OriginInfo; size: Size }) {
  return <Art svg={origin.mark_svg} name={origin.name} size={size} data={{ "data-origin": origin.id }} />;
}
