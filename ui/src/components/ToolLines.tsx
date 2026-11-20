// A media tool as Settings and first run show it: where it came from and which version, then
// where it is; or what is missing without it. And setting its path by hand through the folder
// browser.

import { useQueryClient } from "@tanstack/react-query";
import { t, useT } from "@/i18n";
import * as api from "@/lib/api";
import { plain, type ToolStatus } from "@/lib/api";
import { keys } from "@/lib/queries";
import { consequence, toolFound, toolLine } from "@/lib/tools";
import { useBrowse } from "./FolderBrowser";
import { toast } from "./ui/toast";

/** A quiet line of fact under a name. */
export const line = "font-display text-body text-label-dim [overflow-wrap:anywhere]";

/** A tool's lines: where it came from and its version, then its path; or what is missing without it. */
export function ToolLines({ tool, extra }: { tool: ToolStatus; extra?: string }) {
  useT();
  const found = toolFound(tool);
  return (
    <>
      <p className={line}>{toolLine(tool)}</p>
      {found ? (
        <p className="font-display text-caption text-label-faint [overflow-wrap:anywhere]">{tool.path}</p>
      ) : (
        <p className="font-reading text-body leading-reading text-label-dim">
          {consequence(tool.tool)}
          {extra && ` ${extra}`}
        </p>
      )}
    </>
  );
}

/** Opens the browser on `.exe` files for `tool`, and sets the path picked. */
export function useSetToolPath() {
  const client = useQueryClient();
  const { browser, browse } = useBrowse();
  const set = (tool: api.Tool, path: string | null) =>
    api.setToolPath(tool, path).then(
      () => client.invalidateQueries({ queryKey: keys.tools }),
      (e: unknown) => toast(plain(e)),
    );
  const choose = (tool: api.Tool) =>
    browse({
      mode: "exe",
      title: t("tools.whereIs", { tool }),
      placeholder: t("tools.typePath", { tool }),
      onPick: (path) => void set(tool, path),
    });
  return { browser, choose, set };
}
