# pen.dev redesign notes (v0.7.0)

## What we tried
1. Upgraded Node to v22.20.0 (≥22.19 required).
2. Installed `@pen.dev/cli` 0.3.14.
3. Logged in as `deltrue@mail.grokbot.com` via email OTP (session Active).
4. Agent run `pen --agent claude --out design/slim-tokens.pen …` failed:
   `Not logged in · Please run /login` / `authentication_failed`
   (pen.dev account OK; Claude Agent / Codex session for the AI agent is separate and was not available.)
5. Interactive `get_style` / `read_skill` worked (schema + style catalog).

## Blocker
Claude Agent auth for pen CLI agent mode is missing on this box. No `PEN_CLI_KEY` /
`ANTHROPIC_API_KEY` / Claude Code login was available. Per brief: rebuild with strict
design tokens from pen philosophy when agent redesign is blocked.

## pen philosophy applied
- Variables once → CSS custom properties (colors, space, radius, sizes).
- Components inherit → shared `.btn-*`, `.list-row`, `.sm-badge`, `.head` / `.head-actions`.
- No ad-hoc padding per screen; one spacing scale (`--space-1…6`).
- One button height/padding; one list-row; one badge chip; one page-header layout.

## Co-visible pane rule (v0.7.0 steering)
Floating/sibling panes on the SAME screen (playlist list + track list) share
`--panel-bg` / `--panel-border` / `--panel-radius` — identical chrome.
Hierarchy comes from typography, selection fill, and a hairline divider
(`--panel-divider`), NOT a second surface color or raised shadow on one side.
Refs: Apple Music / Finder master-detail, YesPlayMusic playlist split.
