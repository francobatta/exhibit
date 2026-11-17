// A media tool as Settings and first run show it: where it was found and which version, or what
// is missing without it; and setting its path by hand through the folder browser.

import { useQueryClient } from "@tanstack/react-query";
import * as api from "@/lib/api";
import { plain, type ToolStatus } from "@/lib/api";
import { keys } from "@/lib/queries";
import { CONSEQUENCE, toolFound, toolLine } from "@/lib/tools";
import { useBrowse } from "./FolderBrowser";
import { toast } from "./ui/toast";

/** A quiet line of fact under a name. */
export const line = "font-display text-body text-label-dim [overflow-wrap:anywhere]";

/** A tool's lines: where it is, and what is missing without it. */
export function ToolLines({ tool, extra }: { tool: ToolStatus; extra?: string }) {
  const found = toolFound(tool);
  return (
    <>
      <p className={line}>{toolLine(tool)}</p>
      {!found && (
        <p className="font-reading text-body leading-reading text-label-dim">
          {CONSEQUENCE[tool.tool]}.{extra && ` ${extra}`}
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
      title: `Where is ${tool}?`,
      placeholder: `Or type the path to ${tool}.exe`,
      onPick: (path) => void set(tool, path),
    });
  return { browser, choose, set };
}
