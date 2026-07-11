---
name: forge-design
description: Builds SolidJS UIs with the Forge design system — a dark-default, dense, technical-tools aesthetic for dashboards, consoles, observability and admin panels. Ships the full colour scheme/design tokens (CSS variables), component CSS, and SolidJS components for everything — shell (AppShell with built-in mobile drawer, nav, Crumbs, PageHead, Tabs, Pagination, SplitPane, Sheet), forms (Input, Textarea, Checkbox, Toggle, Radio, Select, Combobox, ListBox, Slider, ToggleGroup, Calendar, DatePicker), feedback (Badge, Toast + Toaster, Alert, Progress, Spinner, Skeleton, Tooltip, Modal, Command palette, menus — Dropdown and ContextMenu), data (Table, Logs, Stat, Avatar, Accordion), plus optional assets: a NodeGraph editor + auto-layout Flowchart (typed ports, elbow connections, animated/broken edges), zero-dep SVG charts (pie, line, bar, gantt, sparkline) on a CVD-validated ramp, a CodeMirror-6 code editor/diff viewer with LSP-style annotations and Forge context menus, and a chat kit — 1:1 and room transcripts, AI tool-call boxes, interactive question prompts (buttons/radio/checkbox/select), link cards, inline media, typing indicator, composer, and a zero-dep XSS-safe Markdown control. Responsive out of the box; includes a runnable preview gallery of every control. Use when building or styling SolidJS components or pages, choosing colours for a new component, wiring design tokens into a project, or when the user says "use the design system", "Forge style", "match the console look", "make it work on mobile", "preview the design system", "show me the controls", "build a node editor", "show a diff", "add a chart", "build a dashboard with charts", "build a chat", "chat UI", "conversation view", "assistant transcript", "render markdown".
user-invocable: true
argument-hint: [what to build or style]
---

<overview>
Applies the **Forge design system** (synced from the claude.ai design project "Tech Tools
Design System") to SolidJS work. The system now ships two ways:

1. **npm packages** (preferred): `@forge/tokens`, `@forge/ui`, `@forge/charts`,
   `@forge/graph`, `@forge/code`, `@forge/chat` under `packages/` in the forge repo
   (github:wiltaylor/forge) — TypeScript, typed props, plus `@forge/client`
   (REST/SSE/WS/JWT API client) and `@forge/remote` (component federation).
   See the repo README for git-dependency install and CSS import order.
2. **Copy-in assets** (fallback for projects that can't take the packages):
   the token CSS, component CSS (`.fbtn`, `.fcard`, …) and `ui.jsx` in this
   skill's `assets/`.

Reference docs cover the colour scheme and the rules for designing new components. Output
is production SolidJS code that renders dark by default and honors `prefers-color-scheme`.
</overview>

<variables>
- `${CLAUDE_SKILL_DIR}`: Path to this skill's directory.
- `$ARGUMENTS`: What the user wants built or styled (may be empty — then ask).
</variables>

<workflow>
<step order="1">
**Preview mode**: if the user asks to preview the design system / see the controls: inside
the forge repo, run `pnpm install` then `pnpm dev` from the repo root **in the background**
— the gallery (apps/gallery, every component + theming demos) is at `http://localhost:5173`.
Elsewhere, use this skill's legacy preview: `cd ${CLAUDE_SKILL_DIR}/preview`, `npm install`
if needed, `npm run dev` in the background → `http://localhost:4890`. Otherwise continue
with step 2.
</step>

<step order="2">
If `$ARGUMENTS` is empty and no UI task is in context, ask the user what they want to
build or style before doing anything else.
</step>

<step order="3">
Read `${CLAUDE_SKILL_DIR}/reference/solidjs.md` (setup, Solid gotchas, primitives, screen
patterns). If the task involves picking colours, creating a **new** component, or anything
not covered by the existing primitives, also read `${CLAUDE_SKILL_DIR}/reference/tokens.md`
and follow its new-component checklist.
</step>

<step order="4">
Wire the design system into the target project. **Prefer the packages**: inside the forge
monorepo add `"@forge/ui": "workspace:^"` etc.; outside it use git dependencies
(`github:wiltaylor/forge#main&path:packages/ui`, pnpm). Import CSS at the app entry in
order: `@forge/tokens/tokens.css`, `@forge/tokens/base.css` (optionally `fonts.css` first),
then `@forge/ui/styles.css` (+ `@forge/charts|graph|code|chat/styles.css` as used).
**Copy-in fallback** (per `reference/solidjs.md`): copy `assets/colors_and_type.css`,
`assets/console.css`, and `assets/ui.jsx` in, import the two CSS files at the app entry
(tokens first). Optional extras: `assets/graph.jsx`, `assets/charts.jsx`, `assets/code.jsx`
(needs the CodeMirror packages listed in `reference/solidjs.md`), and `assets/chat.jsx` +
`assets/chat.css` (chat.jsx imports `./ui.jsx`; import chat.css after console.css). Copy —
never symlink into this skill directory.
For each asset that already exists in the project, run `diff -u <project copy> <skill copy>`:
identical → leave it and continue; different → show the diff and ask the user whether to
update the project copy. Never overwrite an existing project copy without approval.
</step>

<step order="5">
Build the UI. Reuse the primitives and `console.css` classes before writing new CSS; when
new CSS is unavoidable, every colour/size/duration must be a `var(--token)` reference —
no hardcoded hex, oklch, px-durations or shadows. Put new files where the "Where new code
goes" section of `reference/solidjs.md` says — never append to the copied Forge assets.
</step>

<step order="6">
Validate: render the app in both themes (toggle `data-theme` on `<html>`) and at 375px and
768px viewport widths — the drawer opens/closes from the hamburger and nothing forces
page-level horizontal scroll. Run any new component through the checklist at the end of
`reference/tokens.md`. Fix violations before presenting the result.
</step>
</workflow>

<boundaries>
<always>
- Default to dark theme and honor `prefers-color-scheme` — never light-only
- Use CSS variables from `colors_and_type.css` for every colour, size and duration
- Use SolidJS idioms: `class`/`classList`, `splitProps`/`mergeProps`, `Show`/`For` — never destructure props
- Use Lucide icons (`lucide-solid`) at 1.5px stroke, `currentColor`
- Keep density: 32px controls/rows, 14px body, sentence case, tabular numerals with units
- Keep desktop density — touch sizing applies only under `pointer: coarse`, never by viewport width
</always>

<ask>
- What to build, if invoked with no arguments and no UI task in context
- Before adding any runtime dependency beyond `lucide-solid`
- Before overwriting Forge asset files that already exist in the target project
</ask>

<never>
- Hardcode colours that exist as tokens, or invent new colours outside the palette rules in `reference/tokens.md`
- Use drop shadows, gradients, frosted-glass cards, emoji, or unicode-as-icon (`→`, `✓`)
- Put the accent colour on large background fills or pill-shaped buttons
- Hide content on narrow screens beyond the documented reflows (breadcrumbs ≤768px) — reflow, don't remove
- Edit the files in `${CLAUDE_SKILL_DIR}/assets/` — `colors_and_type.css` and `console.css` mirror
  the design project (id `019dc74c-a1ff-74d0-8504-0ad85b5589fe`; re-sync via the DesignSync tool),
  and `ui.jsx` is the skill-owned SolidJS port (the remote `ui.jsx` is the React original — never
  push it from here). `chat.jsx`/`chat.css` (like `charts.jsx`/`graph.jsx`/`code.jsx`) are
  skill-owned ports whose source of truth is the forge repo's packages — sync from there, not by
  hand-invention. `${CLAUDE_SKILL_DIR}/preview/` is skill-owned tooling, not part of the mirror —
  editing it is fine.
</never>
</boundaries>
