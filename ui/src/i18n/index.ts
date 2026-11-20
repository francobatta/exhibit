// The window's languages. Every word on screen comes from a catalogue in `./locales`:
// `en-US.json` holds every key and is the source; `es.json` holds the Spanish both regions share,
// and `es-ES.json` and `es-MX.json` only what Spain and Latin America say differently. A key a
// regional file lacks falls back to `es`, then to English.
//
// The language and the date format are the library's settings (the core keeps them, since TMDB's
// text follows the language too). The window starts in them before its first frame, from what
// the app gives the page (`window.__exhibitLocale`), and follows them when they change.

import i18next from "i18next";
import { initReactI18next, useTranslation } from "react-i18next";
import enUS from "./locales/en-US.json";
import es from "./locales/es.json";
import esES from "./locales/es-ES.json";
import esMX from "./locales/es-MX.json";

export const LANGUAGES = ["en-US", "es-ES", "es-MX"] as const;
export type Language = (typeof LANGUAGES)[number];

/** Each language named in itself, so the owner finds theirs whatever is showing. */
export const LANGUAGE_NAMES: Record<Language, string> = {
  "en-US": "English",
  "es-ES": "Español (España)",
  "es-MX": "Español (Latinoamérica)",
};

export const DATE_FORMATS = ["auto", "day", "month", "numeric"] as const;
export type DateFormat = (typeof DATE_FORMATS)[number];

declare global {
  interface Window {
    /** Set by the app before the page loads. */
    __exhibitLocale?: { language?: string; dateFormat?: string };
  }
}

declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "translation";
    resources: { translation: typeof enUS };
  }
}

const isLanguage = (l: unknown): l is Language => LANGUAGES.includes(l as Language);
const isDateFormat = (f: unknown): f is DateFormat => DATE_FORMATS.includes(f as DateFormat);

export const resources = {
  "en-US": { translation: enUS },
  es: { translation: es },
  "es-ES": { translation: esES },
  "es-MX": { translation: esMX },
};

const given = typeof window === "undefined" ? undefined : window.__exhibitLocale;

let dateFormat: DateFormat = isDateFormat(given?.dateFormat) ? given.dateFormat : "auto";

void i18next.use(initReactI18next).init({
  resources,
  lng: isLanguage(given?.language) ? given.language : "en-US",
  fallbackLng: { "es-ES": ["es", "en-US"], "es-MX": ["es", "en-US"], default: ["en-US"] },
  supportedLngs: [...LANGUAGES, "es"],
  // Catalogues are bundled: ready before the first render.
  initAsync: false,
  interpolation: { escapeValue: false },
  returnNull: false,
});

i18next.on("languageChanged", (lng) => {
  document.documentElement.lang = lng;
});
document.documentElement.lang = i18next.language;

export const i18n = i18next;

/** Translates outside a component (a label built in a helper). Components use `useT`. */
export const t = i18next.t.bind(i18next);

/**
 * A key known only at run time (an error's code), or null when no catalogue has it. Keys written
 * in the code go through `t`, whose types check them.
 */
export function translated(key: string, params: Record<string, string>): string | null {
  if (!i18next.exists(key)) return null;
  return (i18next.t as unknown as (k: string, o: Record<string, string>) => string)(key, params);
}

/** The window's translator; a component that uses it shows the new words when the language changes. */
export function useT() {
  return useTranslation().t;
}

/** The language in effect, in a component: it re-renders when the language changes. */
export function useLanguage(): Language {
  const { i18n: instance } = useTranslation();
  return isLanguage(instance.language) ? instance.language : "en-US";
}

/** The language in effect. */
export function language(): Language {
  return isLanguage(i18next.language) ? i18next.language : "en-US";
}

export function currentDateFormat(): DateFormat {
  return dateFormat;
}

/**
 * Speaks `lang` and writes dates as `format`, at once. Unknown values are ignored. A change of
 * date format alone re-renders the window as a change of language does.
 */
export function setLocale(lang: string | null | undefined, format: string | null | undefined): void {
  const nextLang = isLanguage(lang) ? lang : language();
  const nextFormat = isDateFormat(format) ? format : dateFormat;
  if (nextLang === i18next.language && nextFormat === dateFormat) return;
  dateFormat = nextFormat;
  void i18next.changeLanguage(nextLang);
}
