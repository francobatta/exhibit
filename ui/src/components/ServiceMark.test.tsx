import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, expect, test } from "bun:test";
import { service } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { ServiceMark } from "./ServiceMark";

afterEach(cleanup);

function show(id: string, size: "panel" | "hero" | "line" = "panel") {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <ServiceMark service={id} size={size} />
    </QueryClientProvider>,
  );
}

test("the mark is named by the service and drawn from its svg as a mask", async () => {
  tauri.handle("services", () => [service()]);
  const view = show("letterboxd");
  const mark = await waitFor(() => view.getByRole("img", { name: "Letterboxd" }));
  const art = mark.style.getPropertyValue("--art");
  expect(art).toContain("data:image/svg+xml,");
  expect(art).toContain(encodeURIComponent("<circle"));
  expect(mark.dataset.service).toBe("letterboxd");
});

test("a service the app has not listed draws nothing", async () => {
  tauri.handle("services", () => [service()]);
  const view = show("trakt");
  await new Promise((r) => setTimeout(r, 30));
  expect(view.container.innerHTML).toBe("");
});
