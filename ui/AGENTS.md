# Agents working in ui/

This project uses React and the Vercel UI SDK (`@ui-sdk/react`, `@ui-sdk/core`, beta). Before using
UI SDK, follow the `ui-sdk` skill if it is installed (`.agents/skills/ui-sdk`, linked into
`.claude/skills`), and read `node_modules/@ui-sdk/react/docs/llms.txt`. Do not write its API from
memory.

- Each component comes from its own entry (`@ui-sdk/react/button`); there is no root import.
- For a component's API, read its page under `node_modules/@ui-sdk/react/docs/components/`.
- To build a new component or behaviour, search `node_modules/@ui-sdk/react/docs/core.md` by
  heading (it is over 300 KB; do not read it whole), and read `rendering.md` next to it.

Bun only: `bun test`, `bunx --bun tsc --noEmit`, `bun run build`. Node.js is not installed.
Tokens live only in `src/styles/tokens.css`; owned primitives in `src/components/ui/`.
