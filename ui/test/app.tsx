// The app's screens as the owner meets them, for screen tests: the navigation stack on a route,
// with the item menu, the playing layer and the toasts mounted as App mounts them.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render } from "@testing-library/react";
import { ItemMenuHost } from "@/components/ItemMenu";
import { PlaybackHost } from "@/components/Playing";
import { Toaster } from "@/components/ui/toast";
import { Screens } from "@/nav/Screens";
import { useNav, type Route } from "@/nav/stack";

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
export const settle = (ms = 80) => act(() => sleep(ms));
export const activeKey = () => (document.activeElement as HTMLElement | null)?.dataset.focusKey;

export const press = (key: string) =>
  act(async () => {
    (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    await sleep(30);
  });

/** Selects what has focus, as the pad's A does. */
export const select = () =>
  act(async () => {
    (document.activeElement as HTMLElement).click();
    await sleep(40);
  });

export function screens(route: Route) {
  useNav.getState().replaceRoot(route);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <Screens />
      <ItemMenuHost />
      <PlaybackHost />
      <Toaster />
    </QueryClientProvider>,
  );
}
