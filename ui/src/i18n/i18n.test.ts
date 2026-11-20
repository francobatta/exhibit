import { afterEach, describe, expect, test } from "bun:test";
import { language, LANGUAGES, resources, setLocale, t } from "@/i18n";
import { day, episodeLabel, listed, runtime, titleOf, writeDate } from "@/lib/format";
import { plain } from "@/lib/api";

type Tree = { [key: string]: string | Tree };

/** Every key of a catalogue, dotted, with its text. */
function flatten(tree: Tree, prefix = ""): Map<string, string> {
  const out = new Map<string, string>();
  for (const [k, v] of Object.entries(tree)) {
    const key = prefix === "" ? k : `${prefix}.${k}`;
    if (typeof v === "string") out.set(key, v);
    else for (const [kk, vv] of flatten(v, key)) out.set(kk, vv);
  }
  return out;
}

const english = flatten(resources["en-US"].translation as Tree);
const shared = flatten(resources.es.translation as Tree);
const regions = { "es-ES": flatten(resources["es-ES"].translation as Tree), "es-MX": flatten(resources["es-MX"].translation as Tree) };

const PLURAL = /_(zero|one|two|few|many|other)$/;
const baseOf = (key: string) => key.replace(PLURAL, "");
const placeholders = (text: string) => [...text.matchAll(/\{\{\s*(\w+)/g)].map((m) => m[1]).sort();

describe("the catalogues", () => {
  for (const [region, own] of Object.entries(regions)) {
    // What the region shows: its own words, else the Spanish both regions share.
    const spanish = new Map([...shared, ...own]);

    test(`${region} has every English key, in every plural form Spanish needs`, () => {
      const categories = new Intl.PluralRules(region).resolvedOptions().pluralCategories;
      const missing: string[] = [];
      for (const key of english.keys()) {
        if (PLURAL.test(key)) {
          for (const c of categories) if (!spanish.has(`${baseOf(key)}_${c}`)) missing.push(`${baseOf(key)}_${c}`);
        } else if (!spanish.has(key)) missing.push(key);
      }
      expect([...new Set(missing)]).toEqual([]);
    });

    test(`${region} names the same values as English`, () => {
      const differ: string[] = [];
      for (const [key, text] of spanish) {
        const source = english.get(key) ?? english.get(`${baseOf(key)}_other`);
        if (source === undefined) continue;
        // `count` may be left out of a word that says it ("one season" is "una temporada").
        const ours = placeholders(text).filter((p) => p !== "count");
        const theirs = placeholders(source).filter((p) => p !== "count");
        if (ours.join() !== theirs.join()) differ.push(key);
      }
      expect(differ).toEqual([]);
    });
  }

  test("no catalogue has a key English lacks", () => {
    const known = (key: string) => english.has(key) || english.has(`${baseOf(key)}_other`);
    const extra = [...shared.keys(), ...regions["es-ES"].keys(), ...regions["es-MX"].keys()].filter((k) => !known(k));
    expect(extra).toEqual([]);
  });
});

describe("in Spanish", () => {
  afterEach(() => setLocale("en-US", "auto"));

  test("each region says Settings its own way, wherever it is named", () => {
    setLocale("es-ES", null);
    expect(t("common.openSettings")).toBe("Abrir Ajustes");
    expect(t("home.continueWatching")).toBe("Seguir viendo");
    setLocale("es-MX", null);
    expect(t("common.openSettings")).toBe("Abrir Configuración");
    expect(t("home.continueWatching")).toBe("Continuar viendo");
    expect(t("actions.play")).toBe("Reproducir");
  });

  test("errors, counts, labels and lists are Spanish", () => {
    setLocale("es-MX", null);
    expect(language()).toBe("es-MX");
    expect(plain({ kind: "source_offline", code: "source.offline", params: { path: "D:\\" }, message: "" })).toBe(
      "La unidad de D:\\ no está conectada.",
    );
    expect(t("units.season", { count: 1 })).toBe("1 temporada");
    expect(t("units.season", { count: 3 })).toBe("3 temporadas");
    expect(episodeLabel(2, 4)).toBe("T2 E4");
    expect(titleOf({ kind: "season", title: "", season: 0, episode: null })).toBe("Especiales");
    expect(runtime(162)).toBe("2 h 42 min");
    expect(listed(["inglés", "francés", "neerlandés"])).toBe("inglés, francés y neerlandés");
  });
});

describe("dates", () => {
  const date = new Date(2026, 10, 20);
  afterEach(() => setLocale("en-US", "auto"));

  test("follow the date format, with month names in the language", () => {
    const written = (lang: string) =>
      (["auto", "day", "month", "numeric"] as const).map((format) => {
        setLocale(lang, null);
        return writeDate(date, { format });
      });
    expect(written("en-US")).toEqual(["November 20, 2026", "20 November 2026", "November 20, 2026", "2026-11-20"]);
    expect(written("es-ES")).toEqual(["20 de noviembre de 2026", "20 de noviembre de 2026", "noviembre 20, 2026", "2026-11-20"]);
  });

  test("the owner's format is the one used", () => {
    setLocale("en-US", "day");
    expect(day(date.getTime() / 1000)).toBe("20 November 2026");
    setLocale(null, "numeric");
    expect(day(date.getTime() / 1000)).toBe("2026-11-20");
  });

  test("an unknown language or format leaves things as they are", () => {
    setLocale("fr-FR", "julian");
    expect(language()).toBe("en-US");
    expect(LANGUAGES).toContain(language());
  });
});
