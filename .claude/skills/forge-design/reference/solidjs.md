# Forge in SolidJS — Setup, Components, Patterns

How to wire the Forge design system into a SolidJS project and build screens with it.

## Project setup

1. Copy the three asset files into the project (e.g. into `src/forge/`):
   ```
   cp ${CLAUDE_SKILL_DIR}/assets/colors_and_type.css <project>/src/forge/
   cp ${CLAUDE_SKILL_DIR}/assets/console.css        <project>/src/forge/
   cp ${CLAUDE_SKILL_DIR}/assets/ui.jsx             <project>/src/forge/
   ```
   Further assets are **optional** — copy only what the project needs; graph/charts/code import
   nothing from `ui.jsx` but require this version's `console.css`:
   - `assets/graph.jsx` — `NodeGraph` editor + `Flowchart` (auto-layout DAG). No deps.
   - `assets/charts.jsx` — Pie/Line/Bar/Gantt/Sparkline SVG charts. No deps.
   - `assets/code.jsx` — CodeMirror 6 editor/diff. Needs npm packages (see "Code editor").
   - `assets/chat.jsx` + `assets/chat.css` — chat kit + `Markdown` control. **The exception:
     chat.jsx imports `./ui.jsx`** (Avatar, Button, form controls), so copy it next to ui.jsx,
     and import `chat.css` after `console.css` (chat CSS is skill-owned, not in the mirror).
2. The three files version together — `ui.jsx` components emit classes defined in `console.css`
   (e.g. `Modal` needs the `.fmodal` block), so always copy/update them as a set, never one file.
3. Import the CSS once, in the app entry, tokens first. Find the entry by reading the
   project's `index.html` — the `<script type="module" src="...">` points at it (Vite
   defaults: `src/index.tsx` or `src/main.tsx`):
   ```jsx
   import './forge/colors_and_type.css';
   import './forge/console.css';
   ```
   Fonts (IBM Plex Sans + JetBrains Mono) load via the `@import` inside the token CSS — no extra step.
4. Install the icon set: `npm i lucide-solid` (or `pnpm add` / `bun add` — match the project's lockfile).
   If the project cannot take the dependency, inline SVGs with `stroke="currentColor" stroke-width="1.5"`.
   (`ui.jsx` itself works without lucide — its hamburger and modal-close icons are inline SVGs.)
5. Do **not** add other runtime dependencies for styling. Tailwind is optional; if the project already
   uses it, expose the tokens in `theme.extend` (e.g. `colors: { 'bg-1': 'var(--bg-1)', accent: 'var(--accent)' }`)
   rather than duplicating values.

## Where new code goes

- New components: the project's existing component directory; if it has none, default to
  `src/components/<Name>.jsx`.
- New component CSS: a project-owned stylesheet `src/forge/custom.css`, imported at the app
  entry **after** `console.css`. Create it on first need.
- Never append to the copied `colors_and_type.css`, `console.css`, or `ui.jsx` — keeping them
  pristine lets them be re-synced from the design project later.

## Solid-specific rules (the JSX is plain, but these bite)

- Use `class`, not `className`; use `classList={{ 'is-error': !!props.error }}` for conditional classes.
- **Never destructure props** in a component signature — it breaks reactivity. Use `props.x`,
  or `splitProps` / `mergeProps` as in `assets/ui.jsx`.
- Use `<Show when={...}>` for conditionals and `<For each={...}>` for lists (not `.map`).
- Event handlers are `onClick`, `onInput` (Solid inputs fire `onInput`, not `onChange`, per keystroke).
- lucide-solid icons are components: `import { Terminal } from 'lucide-solid'` then
  `<Terminal size={16} strokeWidth={1.5} />`, or pass through the `Icon` wrapper: `<Icon of={Terminal} />`.

## The primitives (`assets/ui.jsx`)

| Component | Props | Notes |
|---|---|---|
| `Button` | `variant` (`primary`\|`secondary`\|`danger`\|`ghost`), `size` (`sm`\|`md`\|`lg`), `icon` (lucide component), native button props | `secondary`/`md` is the default. One `primary` per view. |
| `Input` | `label`, `help`, `error` (bool), `icon`, native input props | Wraps label + field + help text. |
| `Badge` | `tone` (`neutral`\|`success`\|`warning`\|`danger`\|`info`\|`accent`), `dot` (bool) | Status chips in tables and headers. |
| `Card` | `title`, `action` (JSX for the header right side), `padded` (default true) | Set `padded={false}` when the body is a table. |
| `Stat` | `label`, `value`, `delta`, `tone` (`success`\|`danger`\|`neutral`) | Metric tile — eyebrow label + big tabular number. |
| `Toast` | `tone`, `icon` | Inline notification strip. |
| `StatusDot` | `tone` | 6px solid dot; pair with text, don't use alone. |
| `Kbd` | children | Keyboard shortcut chip. |
| `Icon` | `of` (lucide component), `size` | Forge defaults: 16px, 1.5px stroke, `currentColor`. |

Shell & navigation (classes shown so raw-class markup stays discoverable):

| Component | Props | Notes |
|---|---|---|
| `AppShell` | `topbar` (JSX), `sidebar` (JSX), children → main | The whole `.app-shell` grid. **Owns the mobile drawer**: hamburger, scrim and close-on-nav-tap are built in — zero wiring. |
| `NavSection` | children | `.fsidebar-section` group label. |
| `NavLink` | `href`, `icon`, `active`, `count`, native `<a>` props | Sidebar link; `.is-active` rail + right-aligned `.count`. |
| `Crumbs` | `items` (array) | `.ftopbar-crumbs` with `/` separators. |
| `IconButton` | `icon`, `label` (aria-label + title), native button props | `.ftopbar-icon-btn` for bell/theme/user. |

Page & data:

| Component | Props | Notes |
|---|---|---|
| `PageHead` | `title`, `sub`, `actions` (JSX) | `.page-head` header row. |
| `Grid` | children, native div props | `.fgrid` auto-fit tile grid (stat rows, card grids). |
| `Table` | children (`<thead>`/`<tbody>`), native table props | `.ftable-wrap > .ftable` — mobile scroll built in. Markup-only; render rows with `<For>`. |
| `Logs` / `LogLine` | native div props / `time`, `level` (`info\|warn\|error\|debug`), children | `.flogs` container of `.flog-line` rows. |
| `SettingsLayout` | `nav` (JSX links), children | `.settings-layout` with sticky `.settings-nav`. |
| `SettingsSection` | `title`, `sub`, children | `.settings-section` card. |
| `SettingsRow` | children | `.settings-row` two-column field grid. |
| `Empty` | `title`, `action` (JSX), children (one sentence) | `.empty` dashed empty state. |
| `Eyebrow` | children | `.eyebrow` micro-label. |
| `Modal` | `open`, `onClose`, `title`, `footer` (JSX), children | Portal + `.fmodal`; closes on Escape, backdrop click, head X. Controlled by a signal. **Requires the `.fmodal` block in console.css — copy both files together.** |

Form controls (all controlled; Checkbox/Toggle/Radio keep a hidden native input for a11y):

| Component | Props | Notes |
|---|---|---|
| `Checkbox` | `checked`, `onChange(bool)`, `indeterminate`, `disabled`, children = label | `.fcheck`; indeterminate renders an accent dash. |
| `Toggle` | `checked`, `onChange(bool)`, `disabled`, children = label | `.ftoggle` switch — pill track is a sanctioned radius exception. |
| `Radio` / `RadioGroup` | group: `options [{value,label,disabled?}]`, `value`, `onChange`, `label?`, `row?` | `.fradio`; group name auto-generated. |
| `Select` | `options`, `value`, `onChange`, `placeholder`, `label`, `help`, `error`, `disabled` | Custom `.fselect-pop` popover (z 60 — works inside modals); Arrow/Enter/Escape/Home/End keys. Caveat: the popover is positioned in-flow, so inside a *scrolling* `.fmodal-body` it can clip — keep modal selects near the top or size the body. |
| `ListBox` | `options` + single `value`/`onChange`, or `multiple` + `values[]`/`onChange(values)`, `label?` | `.flistbox` scrollable option list; multi rows get check indicators; keyboard nav. |
| `Progress` | `value` 0–100, `tone` (`accent`\|`success`\|`warning`\|`danger`), `label?`, `showValue?`, `indeterminate?` | 4px `.fprogress` bar — the default Forge loading treatment. |
| `Spinner` | `size` (16), `label` | Inline arc spinner, `currentColor` — for inline/button waits only. |
| `Combobox` | `options`, `value`, `onChange`, `placeholder`, `label`, `help`, `error`, `emptyText?` | Searchable select — typing filters (`.fcombo` + the Select popover classes). |
| `Slider` | `value`, `onChange(n)`, `min` 0, `max` 100, `step` 1, `label?`, `showValue?` | Restyled native range (`.fslider`) — keyboard/a11y free. |
| `Textarea` | `label`, `help`, `error`, native textarea props | `.ffield-area` — mirrors Input. |
| `ToggleGroup` | `options [{value,label,icon?,disabled?}]`, `value`, `onChange` | `.fseg` segmented control. |
| `Calendar` | `value` (`YYYY-MM-DD`\|null), `onChange(iso)`, `min?`, `max?` | Monday-start month grid; today outlined, ISO strings throughout. Ranges = future work. |
| `DatePicker` | Calendar props + `label`, `placeholder`, `help`, `disabled` | Trigger button + Calendar in a `.fpop`. |

Overlays & menus:

| Component | Props | Notes |
|---|---|---|
| `Tooltip` | `label`, `side` (`top`\|`bottom`\|`left`\|`right`), children | CSS-only (`.ftip`), shows on hover **and** keyboard focus. Text-only — rich content wants `Popover`. |
| `Popover` | `label`, `icon?`, `variant`, `size`, `align` (`start`\|`end`), `width?`, children = panel | Renders its own trigger `Button` + `.fpop` panel. |
| `DropdownMenu` | trigger props + `items` | Items: `{label, icon?, kbd?, danger?, disabled?, onSelect?}` or `{separator: true}`. Full keyboard nav. |
| `ContextMenu` | `items` (same shape), children = surface | Right-click opens at cursor inside the wrapped surface. |
| `Command` | `open`, `onClose`, `items [{group?, label, icon?, kbd?, onSelect}]`, `placeholder?` | ⌘K palette (`.fcmd`, modal layer). Substring filter, arrows wrap, Enter selects. Bind the hotkey consumer-side. |
| `Sheet` | `open`, `onClose`, `title`, `side` (`right`\|`left`), `footer?`, children | Slide-in panel at z 40 — above the drawer, below modals. Full-width ≤768px. |
| `Toaster` / `toast(msg, {tone, icon, duration})` / `dismissToast(id)` | mount `<Toaster/>` once at the app root | Stacked corner toasts at z 70; `duration: 0` = sticky; default 4 s. |

Navigation & structure:

| Component | Props | Notes |
|---|---|---|
| `Tabs` | `tabs [{id, label, count?, disabled?}]`, `active`, `onChange` | Renders the bar only — content is your `Show`/`Switch`. |
| `Accordion` | `items [{id, title, content}]`, `defaultOpen?` (id) | Single-open. `Collapsible` (`title`, `defaultOpen?`) is the standalone disclosure. |
| `Pagination` | `page`, `pages`, `onChange` | Windowed `1 … p−1 [p] p+1 … N`. |
| `Separator` | `vertical?` | `.fsep` hairline. |
| `Skeleton` | `width?`, `height?` | Loading placeholder; shimmer only under `prefers-reduced-motion: no-preference`. |
| `Avatar` | `name`, `src?`, `size` (`sm`\|`md`\|`lg`), `status?` (tone) | Initials fallback, optional status dot. |
| `Alert` | `tone`, `title?`, `icon?`, children | Block callout via the tone triple. |
| `SplitPane` | `first`, `second` (JSX), `initial` 280, `min` 160, `vertical?`, `onResize?` | Draggable divider (pointer + arrow keys). Uncontrolled. |

Popover-family caveat: all anchored popovers (Select, Combobox, DatePicker, DropdownMenu,
Popover, ContextMenu) position in-place with `position: absolute` — inside a scrolling or
`overflow: hidden` container they clip. Keep them out of scrolling modal bodies, or size
the container. Don't open `Command` and `Modal` at once (both live at z 50).

```jsx
// DropdownMenu
<DropdownMenu label="Actions" align="end" items={[
  { label: 'Rename', icon: Pencil, kbd: 'R', onSelect: rename },
  { separator: true },
  { label: 'Delete', icon: Trash2, danger: true, onSelect: del },
]} />

// Command — bind ⌘K yourself (one-liner):
createEffect(() => {
  const onKey = (e) => { if ((e.metaKey || e.ctrlKey) && e.key === 'k') { e.preventDefault(); setCmdOpen(true); } };
  document.addEventListener('keydown', onKey);
  onCleanup(() => document.removeEventListener('keydown', onKey));
});

// Toaster — mount once, call from anywhere:
<Toaster />
toast('Deploy started');
toast('Disk almost full', { tone: 'warning', duration: 0 });  // sticky
```

## Node graphs (`assets/graph.jsx`, optional)

`NodeGraph` is a **controlled** node editor: nodes/edges/selection live in the consumer's
store; the component reports interactions via callbacks. Elbow (orthogonal) edge routing,
typed connection ports, drag-to-move, drag-to-connect, click-to-select, Delete-to-remove.

```jsx
import { NodeGraph } from './forge/graph';

<NodeGraph
  nodes={[{ id: 'fetch', x: 40, y: 60, title: 'HTTP request',
            inputs:  [{ id: 'run',  type: 'trigger' }],
            outputs: [{ id: 'body', type: 'object' }, { id: 'status', type: 'number' }] }]}
  edges={[{ id: 'e1', from: { node: 'fetch', port: 'body' },
            to: { node: 'parse', port: 'raw' }, state: 'active' }]}  // 'active' | 'broken' | default
  selected={sel()}                          // null | {kind: 'node'|'edge', id}
  onNodeMove={(id, x, y) => …}              // fires per pointermove — write to your store
  onConnect={({ from, to }) => …}           // append the edge (types already validated)
  onSelect={setSel}                         // null on background click
  onDelete={(sel) => …}                     // Delete/Backspace with a selection
  style={{ height: '460px' }}               // consumer sizes the canvas
/>
```

- Edge `state`: `active` = marching-ants animation (flow direction out → in); `broken` =
  flashing danger; default = solid in the source port's type colour. Under reduced motion
  both fall back to static, legible styles.
- Port types → colours (`PORT_COLORS` export): trigger `--fg-0`, string `--success`,
  number `--info`, boolean `--danger`, object `--accent`, array `--warning`, any `--fg-3`.
  `any` connects to everything; otherwise types must match.
- Nodes are fixed-width (`--fgraph-node-w`, 180px; per-node `w` override) so edge anchors
  are computed without DOM measurement. Optional per-node body: pass a render-prop child
  `{(node) => JSX}`.
- Future work (not yet built): pan/zoom viewport, auto-width nodes.

### Flowchart (display DAG, auto-layout)

`<Flowchart nodes={[{id, label, tone?}]} edges={[{from, to, label?, state?}]} onNodeClick? />`
— no x/y needed: longest-path layering + one barycenter pass lays the graph out left→right.
Edge `state` reuses the NodeGraph classes (`active` ants, `broken` flash). Cycles are
tolerated (back-edges skipped for layering, still drawn via the backward detour). Renders
inside a `.fchart-scroll`, so it scrolls horizontally on narrow screens.

## Charts (`assets/charts.jsx`, optional, zero-dep)

Static SVG (hover/tooltip layer is future work). Categorical series colours come from the
**validated ramp** `CHART_SERIES` (see "Chart colours" in tokens.md) — fixed order, never
cycled; >5 series fold into "Other" (`--fg-2`). Semantic data passes `tone:` instead —
status hues never impersonate a series. Charts measure their container (ResizeObserver)
and re-render at true pixel width.

| Component | Props |
|---|---|
| `PieChart` | `data [{label, value, tone?}]`, `size` 180, `donut?`, `legend` true, `showValues` true |
| `LineChart` | `series [{label, points [{x,y}], tone?}]`, `height` 220, `area?`, `xLabels?`, `yTicks?` |
| `BarChart` | `data` or `series [{label, data, tone?}]`, `stacked?`, `height` 220 — vertical only (horizontal = future) |
| `GanttChart` | `tasks [{id, label, start, end, tone?, progress?}]` (ISO dates), `today?` (`false` hides) — dashed accent today line; dependency arrows = future |
| `Sparkline` | `points [n]`, `width` 96, `height` 28, `tone?` — for Stat cards |

Also exported: `CHART_SERIES`, `CHART_SERIES_BG`, `seriesColor(i, tone?)`, `niceTicks`.

## Chat (`assets/chat.jsx` + `assets/chat.css`, optional)

Chat kit: 1:1 and room transcripts, AI tool-call boxes, interactive question prompts,
media/file blocks, link cards, typing indicator, composer, plus a standalone `Markdown`
control. In the monorepo this is `@forge/chat` (`import '@forge/chat/styles.css'` after
the ui styles). As a copy-in it needs `ui.jsx` beside it and `chat.css` imported after
`console.css`.

| Component | Props | Notes |
|---|---|---|
| `ChatView` | `items`, `participants [{id,name,avatar?,status?}]`, `self` (id), `variant` (`direct`\|`room`), `typing?` (ids), `unreadAfter?` (item id), `groupWindow?` 5, `dayDividers?` true, `showTimes?` true, `resolveLink?`, `onReachTop?`, `markdown?` true, `style`, children = composer slot | Data-driven transcript. Owns grouping, day dividers ("Today"/"Yesterday"), unread marker, stick-to-bottom scrolling, the "N new messages" jump pill, and scroll compensation when `onReachTop` prepends history. Consumer sizes it (`style={{height}}`). |
| `ChatMessage` | `message`, `participant?`, `self?`, `showTime?`, `markdown?`, `resolveLink?` | Standalone message renderer (ChatView calls it per message). |
| `ChatToolCall` | `tool {name, status: running\|success\|error, summary?, args?, result?, defaultOpen?, children?}` | Collapsible tool-call box; string args/result render as code blocks; `children` nest recursively. |
| `ChatPrompt` | `prompt {id, question, control, answer?, onAnswer, submitLabel?}` — control: `{type: buttons\|radio\|checkbox\|select, options, placeholder?}` | Interactive question. `answer` present ⇒ disabled + chosen highlighted + "Answered" footer. Checkbox answers with `string[]`. |
| `LinkCard` | `url`, `meta? {url,title?,description?,image?,icon?,domain?}`, `resolve?` | **No client-side metadata fetching (CORS)** — pass `meta` or a server-backed async `resolve(url)`. Loading shows skeletons; failure degrades to a plain anchor. Results cached per URL. |
| `ChatComposer` | `onSend(text)`, `value?`/`onChange?`, `placeholder?`, `disabled?`, `actions?` (left slot), `accessories?` (row above), `maxRows?` 8, `autofocus?` | Auto-growing textarea; Enter sends (IME-safe), Shift+Enter breaks; send never fires empty. |
| `Markdown` | `text`, `linkTarget?` (`_blank`) | Standalone rendered-markdown control (`.fmd`). |
| `ChatTyping` / `ChatDivider` | `names []` / `label` | Exported for standalone use. |

Message shape: `{id, author, at?, text?}` or `{id, author, at?, blocks: [...]}` — blocks:
`{kind:'text', text, markdown?}`, `{kind:'image', src, alt?, width?, height?, href?}`
(pass width/height so space is reserved before load), `{kind:'video', src, poster?, …}`,
`{kind:'file', name, size?, href?, icon?}`, `{kind:'link', url, meta?}`, `{kind:'tool', tool}`,
`{kind:'prompt', prompt}`, `{kind:'custom', render: () => JSX}`. Plus `pending?` (dimmed)
and `error?` (danger border + caption) on the message. Non-message items:
`{type:'event', id, text, at?}` and `{type:'divider', id, label}`.

Markdown subset (exact): headings `#`–`####`, paragraphs (blank-line separated, `\n` = break),
fenced code with lang label, ul/ol + task lists (`- [x]`), `>` blockquote, `---` hr, pipe
tables, images `![alt](url)`, `**bold**`, `*em*`, `~~strike~~`, `` `code` ``, `[label](url)`,
bare-URL autolinks. Raw HTML always renders as literal text; only http/https/mailto URLs
become links (`javascript:` degrades to plain text). No syntax highlighting — that's code.jsx.
Also exported: `parseMarkdown`, `safeUrl`, `formatTime`, `formatDay`, `formatBytes`.

Caveats: a `select` prompt near the bottom of the transcript opens its popover in-flow —
the scroll container scrolls to reveal it (same family as the Select-in-modal caveat).
No virtualization: comfortable to ~1–2k items.

```jsx
import { ChatView, ChatComposer } from './forge/chat';

const [items, setItems] = createSignal([
  { id: 'm1', author: 'ana', at: new Date(), text: 'Ready to **deploy**?' },
  { id: 'm2', author: 'bot', at: new Date(), blocks: [
    { kind: 'tool', tool: { name: 'run_tests', status: 'success', result: '128 passed' } },
    { kind: 'prompt', prompt: { id: 'q1', question: 'Proceed?', answer: answers().q1,
        control: { type: 'buttons', options: [{ value: 'yes', label: 'Deploy' }] },
        onAnswer: (v) => setAnswers({ ...answers(), q1: v }) } },
  ] },
]);

<ChatView style={{ height: '480px' }} variant="room" self="me"
          participants={people} items={items()}>
  <ChatComposer onSend={(t) => setItems([...items(), { id: uid(), author: 'me', at: new Date(), text: t }])} />
</ChatView>
```

## Code editor (`assets/code.jsx`, optional, CodeMirror 6)

Install when you copy the file (the one sanctioned dependency set beyond lucide-solid):

```
npm i @codemirror/state @codemirror/view @codemirror/language @codemirror/commands \
      @codemirror/lint @codemirror/search @codemirror/merge @codemirror/lang-javascript \
      @codemirror/lang-python @codemirror/lang-json @codemirror/lang-css \
      @codemirror/lang-html @codemirror/legacy-modes @lezer/highlight
```

| Component | Props |
|---|---|
| `CodeEditor` | `value`, `onChange` (absent → read-only), `readOnly`, `language` (`js`\|`jsx`\|`ts`\|`tsx`\|`python`\|`json`\|`css`\|`html`\|`shell`), `annotations`, `contextMenuItems`, `lineNumbers` true, `wrap?`, `height` '240px', `placeholder?` |
| `DiffEditor` | `original`, `modified`, `onChange?` (editable right side), `language`, `unified?`, `annotations?` (modified side), `lineNumbers`, `height` '280px' |

Also exported: `LANGUAGES`, `forgeTheme` (theme + highlight extensions for raw CM use).

- **Annotations** are LSP/tree-sitter-style diagnostics: `{from: {line, col}, to?, severity:
  'error'|'warning'|'info'|'hint', message, source?}` — **1-based line, 0-based col**.
  Rendered as squiggles + gutter dots + hover message.
- **Context menu** items use the DropdownMenu shape; `onSelect(view)` receives the
  EditorView. Native menu is suppressed.
- Controlled-value pattern is loop-safe by compare-before-dispatch — echoing `onChange`
  back into `value` is fine. The editor is themed entirely with `var(--token)`, so it
  follows `data-theme` live.

Note: if the copying project's bundler can't resolve bare imports from the forge dir
(out-of-root), alias `@codemirror` and `@lezer` to its `node_modules` (see
`preview/vite.config.js` for the pattern).

<examples>
<example name="dashboard-card">
```jsx
import { For } from 'solid-js';
import { RefreshCw, GitBranch } from 'lucide-solid';
import { Button, Card, Badge, Stat } from './forge/ui';

function DeploysCard(props) {
  return (
    <Card title="Recent deploys" padded={false}
          action={<Button variant="ghost" size="sm" icon={RefreshCw}>Refresh</Button>}>
      <table class="ftable">
        <thead>
          <tr><th>Service</th><th>Commit</th><th>Status</th><th>When</th></tr>
        </thead>
        <tbody>
          <For each={props.deploys}>
            {(d) => (
              <tr>
                <td>{d.service}</td>
                <td class="col-mono">{d.sha.slice(0, 7)}</td>
                <td><Badge tone={d.ok ? 'success' : 'danger'} dot>{d.ok ? 'deployed' : 'failed'}</Badge></td>
                <td class="col-mono">{d.ago}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </Card>
  );
}
```
</example>
</examples>

## Layout & screen patterns (classes from `console.css`)

- **App shell**: `<AppShell topbar={…} sidebar={…}>` (drawer built in) — or raw
  `<div class="app-shell">` containing `.ftopbar`/`.fsidebar`/`.app-main`. Topbar 48px,
  sidebar 240px, both sticky; at ≤1024px the sidebar becomes an off-canvas drawer
  (see "Responsive patterns").
- **Sidebar nav**: `.fsidebar-section` uppercase group labels; links get `.is-active` for the current
  route (accent inset rail comes free); `<span class="count">` right-aligns a mono counter.
- **Page header**: `.page-head` with `<h1>` + `.sub` caption on the left, `.page-actions` buttons right.
- **Topbar**: `.ftopbar-brand`, `.ftopbar-crumbs` (separator `<span class="sep">/</span>`),
  `.ftopbar-search`, `.ftopbar-icon-btn` for bell/theme/user.
- **Tables**: `<Card padded={false}><Table><thead>…` — sticky uppercase headers, 32px hoverable
  rows, `.col-mono` for IDs/times. `Table` renders `.ftable-wrap > .ftable`, which scrolls the
  table horizontally at ≤768px so it never forces page-level scroll.
- **Logs**: `.flogs` container of `.flog-line` grids (`.flog-time` / `.flog-level info|warn|error|debug` /
  `.flog-msg`). Mono, dense, on `--bg-0`.
- **Settings**: `<SettingsLayout nav={…}><SettingsSection title …><SettingsRow>` — sticky nav +
  section cards + two-column field grids.
- **Empty states**: `<Empty title="…" action={…}>one sentence</Empty>`. Never a paragraph.
- **Stat rows**: `<Grid>` of `<Card><Stat …/></Card>` tiles — auto-fit columns, collapses to one
  column on narrow screens with no media query.
- **Modals**: `<Modal open={sig()} onClose={…} title footer={…}>` — Portal-rendered `.fmodal`:
  `--bg-1` panel, `--r-lg`, `border-strong`, dimmed blurred backdrop, enter over `--dur-3`;
  closes on Escape / backdrop / X.

## Responsive patterns

The component CSS is responsive out of the box (breakpoints in `reference/tokens.md`), and
`AppShell` owns the mobile drawer — hamburger, scrim, and close-on-nav-tap need zero wiring:

```jsx
import { AppShell, NavSection, NavLink, Crumbs, IconButton } from './forge/ui';
import { LayoutGrid, Bell } from 'lucide-solid';

<AppShell
  topbar={<>
    <div class="ftopbar-brand"><strong>console</strong></div>
    <Crumbs items={['fleet', 'nodes']} />
    <div style={{ flex: 1 }} />
    <IconButton icon={Bell} label="Notifications" />
  </>}
  sidebar={<>
    <NavSection>Fleet</NavSection>
    <NavLink href="/" icon={LayoutGrid} active count={12}>Overview</NavLink>
  </>}
>
  {/* page content */}
</AppShell>
```

- Fallback (raw-class shells that can't use `ui.jsx`): add a `createSignal(false)`, toggle
  `is-sidebar-open` on `.app-shell` via `classList`, render a
  `button.ftopbar-icon-btn.fsidebar-toggle` first in the topbar and a `div.fscrim` after the
  nav, both clicking the signal closed. The toggle and scrim are `display: none` on desktop,
  so the markup is inert above 1024px.
- Don't hide content to make a screen fit — reflow it. The only sanctioned removal is the
  breadcrumbs at ≤768px (the page `<h1>` carries location).
- Test widths: **1280** (desktop truth), **1024** and **768** (breakpoint edges), **375** (phone).

## Copy rules (UI strings you generate)

- Sentence case everywhere. Buttons are 1–3 word verbs ("Deploy", "Roll back").
- Errors: what failed → why → what to do, one line each. Calm, no exclamation marks, no emoji.
- Numbers always carry units with a space (`12 ms`, `4.2 GB`); relative time in lists ("2m ago"),
  absolute UTC on hover/detail.

## Verifying a screen

After building, run the app: read the `scripts` in `package.json` and use `dev` (or `start`),
with the package manager the lockfile implies. If no runnable script exists, skip the live
render, do a static pass of the tokens.md checklist instead, and say so in your summary.
Then check: dark theme renders
by default; toggling `data-theme="light"` on `<html>` keeps everything legible; keyboard-Tab shows
2px accent focus rings; at 375px wide there is no page-level horizontal scroll and the drawer
opens/closes from the hamburger. If a component looks off, diff its CSS usage against the
checklist in `${CLAUDE_SKILL_DIR}/reference/tokens.md` before inventing new styles.
