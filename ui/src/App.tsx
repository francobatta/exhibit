import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MotionConfig } from "motion/react";
import { lazy, Suspense, useEffect, useState } from "react";
import { Backdrop } from "@/components/Backdrop";
import { ConfirmHost } from "@/components/Dialog";
import { Hints } from "@/components/Hints";
import { ItemMenuHost } from "@/components/ItemMenu";
import { MatchPickerHost } from "@/components/MatchPicker";
import { PlaybackHost } from "@/components/Playing";
import { ErrorState, Loading } from "@/components/states";
import { TopBar } from "@/components/TopBar";
import { Toaster } from "@/components/ui/toast";
import { FocusRoot } from "@/focus/react";
import { plain } from "@/lib/api";
import { LibrarySync, useBoot } from "@/lib/queries";
import { cycleSection, openSettings, Screens } from "@/nav/Screens";
import { openAsk, useNav } from "@/nav/stack";
import { AskHost } from "@/screens/Ask";

// Development only: every work component on one wall (screens/Gallery.tsx). Not in production builds.
const gallery =
  import.meta.env.DEV &&
  (new URLSearchParams(location.search).has("gallery") || import.meta.env.VITE_EXHIBIT_GALLERY === "1");
const Gallery = gallery ? lazy(() => import("@/screens/Gallery").then((m) => ({ default: m.Gallery }))) : null;

function Shell() {
  const boot = useBoot();
  const firstRun = boot.data?.firstRun;

  useEffect(() => {
    if (firstRun === true) useNav.getState().replaceRoot({ screen: "firstRun" });
  }, [firstRun]);

  if (boot.isPending) return <Loading label="Opening the library" />;
  if (boot.isError) {
    return (
      <div className="pt-bar">
        <ErrorState
          message={plain(boot.error)}
          hint="Exhibit could not open its library. Check that its data folder is reachable, then try again."
          action={{ label: "Try again", onAction: () => boot.refetch() }}
        />
      </div>
    );
  }
  return (
    <>
      {Gallery ? (
        <Suspense fallback={<Loading />}>
          <Gallery />
        </Suspense>
      ) : (
        <Screens />
      )}
      <TopBar />
    </>
  );
}

export default function App() {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: 1, staleTime: 30_000 } } }),
  );
  return (
    <QueryClientProvider client={client}>
      <MotionConfig reducedMotion="user">
        <FocusRoot
          onAsk={() => openAsk()}
          onSection={cycleSection}
          onSettings={openSettings}
        />
        <LibrarySync />
        <Backdrop />
        <Shell />
        <AskHost />
        <ItemMenuHost />
        <MatchPickerHost />
        <ConfirmHost />
        <PlaybackHost />
        <Hints />
        <Toaster />
      </MotionConfig>
    </QueryClientProvider>
  );
}
