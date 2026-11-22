// Onboarding the owner's AI agent, as a hosted service does with a setup link: one line to paste
// to the agent, which sends it to instructions the core writes for this exe and this library
// (`agent_prompt`). The agent registers Exhibit's MCP server for good and checks it works.

import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useState } from "react";
import { toast } from "@/components/ui/toast";
import { language, useT, type Language } from "@/i18n";
import * as api from "@/lib/api";
import { plain } from "@/lib/api";

/** The language the agent answers in, named in English for it; none for English. */
const REPLY_IN: Record<Language, string | null> = {
  "en-US": null,
  "es-ES": "Spanish (Spain)",
  "es-MX": "Spanish (Mexico)",
};

/** `copy` writes the instructions afresh and puts the setup prompt on the clipboard; `copied` once it has. */
export function useAgentSetup() {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const copy = () => {
    api
      .agentPrompt(REPLY_IN[language()])
      .then(writeText)
      .then(
        () => {
          setCopied(true);
          toast(t("agentSetup.copied"));
        },
        (e: unknown) => toast(t("agentSetup.copyFailed", { reason: plain(e) })),
      );
  };
  return { copied, copy };
}
