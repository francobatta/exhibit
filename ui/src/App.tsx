import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MotionConfig } from "motion/react";
import { useEffect, useState } from "react";
import { Backdrop } from "@/components/Backdrop";
import { Hints } from "@/components/Hints";
import { ErrorState, Loading } from "@/components/states";
import { TopBar } from "@/components/TopBar";
import { Toaster } from "@/components/ui/toast";
import { focusItem } from "@/focus/bridge";
import { FocusRoot } from "@/focus/react";
import { plain } from "@/lib/api";
import { LibrarySync, useBoot } from "@/lib/queries";
import { cycleSection, openSettings, Screens } from "@/nav/Screens";
import { useNav } from "@/nav/stack";

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
      <Screens />
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
          onAsk={() => focusItem("ask")}
          onSection={cycleSection}
          onSettings={openSettings}
        />
        <LibrarySync />
        <Backdrop />
        <Shell />
        <Hints />
        <Toaster />
      </MotionConfig>
    </QueryClientProvider>
  );
}
