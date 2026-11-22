// When the owner is sitting down: the part of the day, whether it is the weekend, and the season
// where they live. Home's programme follows it, and so does the colour of the lamp (tokens.css,
// `[data-daypart]`). Nothing here is a setting: the clock and the time zone decide.

export type Daypart = "morning" | "afternoon" | "evening" | "late";
export type Season = "spring" | "summer" | "autumn" | "winter";

export interface Moment {
  daypart: Daypart;
  /** Friday evening through Sunday: time for a long film. */
  weekend: boolean;
  season: Season;
  /** Days since 1970 for the evening this moment belongs to (after midnight is still last night), so a choice can vary by day yet hold all evening. */
  day: number;
}

/** The night runs on until five in the morning: the late hours belong to the day before. */
const NIGHT_ENDS = 5;

/** Time zones south of the equator, where the seasons run the other way. */
const SOUTH = [
  "Australia/",
  "Antarctica/",
  "Pacific/Auckland",
  "Pacific/Chatham",
  "Pacific/Fiji",
  "America/Argentina/",
  "America/Buenos_Aires",
  "America/Santiago",
  "America/Punta_Arenas",
  "America/Montevideo",
  "America/Asuncion",
  "America/Sao_Paulo",
  "America/La_Paz",
  "America/Lima",
  "Africa/Johannesburg",
  "Africa/Maputo",
  "Africa/Harare",
  "Africa/Windhoek",
  "Indian/Mauritius",
  "Atlantic/Stanley",
];

const NORTH_SEASONS: Season[] = ["winter", "winter", "spring", "spring", "spring", "summer", "summer", "summer", "autumn", "autumn", "autumn", "winter"];
const FLIP: Record<Season, Season> = { spring: "autumn", summer: "winter", autumn: "spring", winter: "summer" };

export function southern(timeZone: string): boolean {
  return SOUTH.some((z) => (z.endsWith("/") ? timeZone.startsWith(z) : timeZone === z));
}

export function daypartOf(hour: number): Daypart {
  if (hour < NIGHT_ENDS) return "late";
  if (hour < 12) return "morning";
  if (hour < 18) return "afternoon";
  return "evening";
}

/** The moment at `now`, in the window's own time zone unless one is given. */
export function momentOf(now: Date, timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone): Moment {
  const daypart = daypartOf(now.getHours());
  // The calendar day the evening started on.
  const evening = new Date(now.getFullYear(), now.getMonth(), now.getDate() - (daypart === "late" ? 1 : 0));
  const weekday = evening.getDay();
  const night = daypart === "evening" || daypart === "late";
  const weekend = weekday === 6 || weekday === 0 || (weekday === 5 && night);
  const north = NORTH_SEASONS[evening.getMonth()]!;
  const day = Math.floor(Date.UTC(evening.getFullYear(), evening.getMonth(), evening.getDate()) / 86_400_000);
  return { daypart, weekend, season: southern(timeZone) ? FLIP[north] : north, day };
}

/** Sets the lamp's colour for the part of the day now, and keeps it current. Call once. */
export function followDaypart(): void {
  const set = () => {
    document.documentElement.dataset.daypart = daypartOf(new Date().getHours());
  };
  set();
  setInterval(set, 60_000);
}
