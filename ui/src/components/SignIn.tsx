// Signing in to a service that asks for its own approval: Exhibit shows a short code and a QR
// code, and the owner approves it on the site from any device (the QR code opens the page with
// the code filled in). Exhibit asks at the pace the service sets until the owner approves,
// declines or the code expires; the code itself never leaves the app. Settings and first run
// both use it.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { FocusButton } from "@/components/FocusButton";
import { line } from "@/components/ToolLines";
import { focusItem } from "@/focus/bridge";
import { FocusGroup } from "@/focus/react";
import { useT } from "@/i18n";
import * as api from "@/lib/api";
import { plain, type SignInView } from "@/lib/api";
import { keys, useStartSignIn } from "@/lib/queries";

export function SignIn({ id, name, focusKey, onDone }: { id: string; name: string; focusKey: string; onDone?: () => void }) {
  const t = useT();
  const client = useQueryClient();
  const start = useStartSignIn();
  const [code, setCode] = useState<SignInView | null>(null);
  const [slower, setSlower] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const poll = useQuery({
    queryKey: ["sign-in", id, code?.userCode],
    queryFn: () => api.pollSignIn(id),
    enabled: code !== null,
    refetchInterval: code === null ? false : (code.interval + slower) * 1000,
    staleTime: 0,
    gcTime: 0,
    retry: false,
  });
  const answer = poll.data;
  // The button that started it gives way to the code: focus goes to Cancel beside it.
  const shown = code !== null;
  useEffect(() => {
    if (shown) focusItem(`${focusKey}:cancel`);
  }, [shown, focusKey]);
  const failure = poll.error;
  useEffect(() => {
    if (answer === "slow_down") setSlower((s) => s + 5);
    if (answer === "done") {
      setCode(null);
      void client.invalidateQueries({ queryKey: keys.library });
      onDone?.();
    }
  }, [answer, client, onDone]);
  useEffect(() => {
    if (!failure) return;
    setError(plain(failure));
    setCode(null);
    focusItem(focusKey);
  }, [failure, focusKey]);

  const begin = () => {
    setError(null);
    setSlower(0);
    start.mutate(id, {
      onSuccess: setCode,
      onError: (e) => setError(plain(e)),
    });
  };
  const cancel = () => {
    setCode(null);
    focusItem(focusKey);
  };

  if (code === null) {
    return (
      <>
        {error && (
          <p role="alert" className="font-display text-body text-label">
            {error}
          </p>
        )}
        <FocusGroup id={`service-${id}-sign-in`} orientation="horizontal" className="flex">
          <FocusButton focusKey={focusKey} onSelect={begin} disabled={start.isPending}>
            {start.isPending ? t("services.checking") : t("services.signIn")}
          </FocusButton>
        </FocusGroup>
      </>
    );
  }
  const site = code.verifyUrl.replace(/^https?:\/\//, "").replace(/\/$/, "");
  return (
    <div data-sign-in className="flex items-center gap-10">
      {/* Exhibit's own QR code, drawn in the text colour. */}
      <span aria-hidden className="size-(--sign-in-qr) shrink-0 text-label [&>svg]:size-full" dangerouslySetInnerHTML={{ __html: code.qrSvg }} />
      <div className="flex min-w-0 flex-col gap-3">
        <p className={line}>{t("services.signInSteps", { url: site })}</p>
        <p data-user-code className="font-display text-display leading-none font-title tracking-[0.12em] text-label tabular-nums">
          {code.userCode}
        </p>
        <p role="status" className={line}>
          {t("services.waitingApproval", { service: name })}
        </p>
        <FocusGroup id={`service-${id}-sign-in`} orientation="horizontal" className="flex">
          <FocusButton focusKey={`${focusKey}:cancel`} variant="plain" onBack={cancel} onSelect={cancel}>
            {t("services.cancel")}
          </FocusButton>
        </FocusGroup>
      </div>
    </div>
  );
}
