import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, expect, test } from "bun:test";
import { award, awardsService } from "../../test/fixtures";
import { i18n } from "@/i18n";
import { scoreText, votesText } from "@/lib/format";
import { awardsByYear, bodyName, Laurels, pageOf, wreaths } from "./Reception";

afterEach(async () => {
  cleanup();
  await act(async () => void (await i18n.changeLanguage("en-US")));
});

const PARASITE = [
  award("academy", "Best Picture", 2020, true, 0),
  award("academy", "Best Director", 2020, true, 1),
  award("academy", "Best Original Screenplay", 2020, true, 2),
  award("academy", "Best International Feature Film", 2020, true, 2),
  award("academy", "Best Film Editing", 2020, false, 2),
  award("cannes", "Palme d'Or", 2019, true, 10),
  award("globes", "Best Foreign Language Film", 2020, true, 42),
  award("bafta", "Best Original Screenplay", 2020, true, 52),
  award("bafta", "Best Film", 2020, false, 50),
];

test("one_wreath_per_body_named_by_its_top_win_three_at_most", () => {
  const { shown, more } = wreaths(PARASITE);
  expect(shown.map((w) => [w.body, w.top.name, w.wins])).toEqual([
    ["academy", "Best Picture", 4],
    ["cannes", "Palme d'Or", 1],
    ["globes", "Best Foreign Language Film", 1],
  ]);
  // BAFTA's one win is left over; nominations never make a wreath.
  expect(more).toBe(1);
  expect(wreaths([award("academy", "Best Actor", 1977, false, 2)]).shown).toEqual([]);
});

test("laurels_stand_only_for_wins_and_say_the_rest", () => {
  const view = render(
    <QueryClientProvider client={new QueryClient()}>
      <Laurels awards={PARASITE} services={[awardsService()]} />
    </QueryClientProvider>,
  );
  const wreathsShown = [...view.container.querySelectorAll("[data-laurel]")];
  expect(wreathsShown.map((w) => w.getAttribute("data-laurel"))).toEqual(["academy", "cannes", "globes"]);
  expect(wreathsShown[0]!.textContent).toBe("Best PictureAcademy Awards 20204 wins");
  expect(wreathsShown[1]!.textContent).toBe("Palme d'OrCannes 2019");
  expect(view.container.textContent).toContain("and 1 more award");
  view.unmount();
  const none = render(<Laurels awards={[award("academy", "Best Actor", 1977, false, 2)]} services={[]} />);
  expect(none.container.innerHTML).toBe("");
});

test("bodies_are_worded_by_the_catalogue_else_by_the_adapter", async () => {
  await act(async () => void (await i18n.changeLanguage("es-MX")));
  expect(bodyName("academy", [awardsService()])).toBe("Premios Óscar");
  expect(bodyName("nowhere", [awardsService()])).toBe("Nowhere Fest");
  expect(bodyName("gone", [])).toBe("gone");
});

test("awards_run_by_year_wins_first", () => {
  const years = awardsByYear([award("a", "N", 1977, false, 2), award("a", "W", 1977, true, 5), award("b", "P", 1976, true, 0)]);
  expect(years.map((y) => [y.year, y.awards.map((a) => a.name)])).toEqual([
    [1976, ["P"]],
    [1977, ["W", "N"]],
  ]);
});

test("scores_read_as_their_origin_writes_them_in_the_owners_language", async () => {
  expect(scoreText(8.2, "decimal")).toBe("8.2");
  expect(scoreText(4, "decimal")).toBe("4.0");
  expect(scoreText(89, "percent")).toBe("89%");
  expect(scoreText(94, "badge")).toBe("94");
  expect(votesText(163)).toBe("163 votes");
  expect(votesText(1_049_339)).toBe("1M votes");
  expect(votesText(null)).toBeNull();
  expect(pageOf("https://www.rottentomatoes.com/m/taxi_driver")).toBe("rottentomatoes.com/m/taxi_driver");
  expect(pageOf("https://www.imdb.com/title/tt0075314/")).toBe("imdb.com/title/tt0075314");
  await act(async () => void (await i18n.changeLanguage("es-ES")));
  expect(scoreText(8.2, "decimal")).toBe("8,2");
  expect(votesText(1)).toBe("1 voto");
});
