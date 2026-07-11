/* Forge chat — optional copy-in asset (chat UI: conversation view, tool-call
   boxes, interactive prompts, link cards, markdown, composer).

   UNLIKE the other extras this file IMPORTS ./ui.jsx (Avatar, Button, Icon,
   Spinner, Skeleton, Checkbox, Radio, Select) and needs chat.css from this
   skill — chat CSS is NOT part of the console.css mirror. Copy in together:
   colors_and_type.css + console.css + ui.jsx + chat.css + chat.jsx.

   Source of truth: packages/chat in the forge repo (this is the skill-owned
   JSX port — keep in sync when the package changes). */

import {
  For, Match, Show, Switch, createEffect, createMemo, createResource,
  createSignal, createUniqueId, mergeProps, onCleanup, onMount,
} from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { Avatar, Button, Checkbox, Icon, Radio, Select, Skeleton, Spinner } from './ui.jsx';

/* ================= markdown parser (zero-dep, XSS-safe) ======================= */
/* Raw HTML is never interpreted (stays literal text); link/image URLs pass
   safeUrl() or degrade to plain text. */

export function safeUrl(raw) {
  try {
    const u = new URL(raw, 'https://relative.invalid');
    return u.protocol === 'http:' || u.protocol === 'https:' || u.protocol === 'mailto:' ? raw : null;
  } catch {
    return null;
  }
}

const EM_DEPTH_MAX = 4;

function earliestMatch(s, depth) {
  let best = null;
  /* First consider() at a given index wins — call order is priority order. */
  const consider = (idx, len, nodes) => {
    if (best === null || idx < best.idx) best = { idx, len, nodes };
  };
  let m;

  if ((m = /`([^`]+)`/.exec(s)))
    consider(m.index, m[0].length, [{ t: 'code', text: m[1] }]);
  if ((m = /!\[([^\]]*)\]\(((?:[^()\s]|\([^()\s]*\))+)\)/.exec(s))) {
    const src = safeUrl(m[2]);
    consider(m.index, m[0].length,
      src ? [{ t: 'image', src, alt: m[1] }] : [{ t: 'text', text: m[1] }]);
  }
  if ((m = /\[([^\]]+)\]\(((?:[^()\s]|\([^()\s]*\))+)\)/.exec(s))) {
    const href = safeUrl(m[2]);
    const children = parseInline(m[1], depth + 1);
    consider(m.index, m[0].length, href ? [{ t: 'link', href, children }] : children);
  }
  if ((m = /\*\*(.+?)\*\*/.exec(s)))
    consider(m.index, m[0].length, [{ t: 'strong', children: parseInline(m[1], depth + 1) }]);
  if ((m = /~~(.+?)~~/.exec(s)))
    consider(m.index, m[0].length, [{ t: 'strike', children: parseInline(m[1], depth + 1) }]);
  if ((m = /\*([^*\s](?:[^*]*[^*\s])?)\*|\b_([^_\s](?:[^_]*[^_\s])?)_\b/.exec(s)))
    consider(m.index, m[0].length, [{ t: 'em', children: parseInline(m[1] ?? m[2], depth + 1) }]);
  if ((m = /https?:\/\/[^\s<>]+/.exec(s))) {
    const url = m[0].replace(/[.,;:!?)\]'"]+$/, '');
    if (url.length > 'https://'.length)
      consider(m.index, url.length, [{ t: 'link', href: url, children: [{ t: 'text', text: url }] }]);
  }
  return best;
}

export function parseInline(src, depth = 0) {
  if (!src) return [];
  if (depth >= EM_DEPTH_MAX) return [{ t: 'text', text: src }];
  const out = [];
  let rest = src;
  while (rest.length) {
    const m = earliestMatch(rest, depth);
    if (!m) {
      out.push({ t: 'text', text: rest });
      break;
    }
    if (m.idx > 0) out.push({ t: 'text', text: rest.slice(0, m.idx) });
    out.push(...m.nodes);
    rest = rest.slice(m.idx + m.len);
  }
  return out;
}

const LIST_RE = /^\s*([-*]|\d+[.)])\s+(.*)$/;

function splitRow(line) {
  let s = line.trim();
  if (s.startsWith('|')) s = s.slice(1);
  if (s.endsWith('|')) s = s.slice(0, -1);
  return s.split('|').map((c) => c.trim());
}

function inlineLines(lines) {
  const out = [];
  lines.forEach((l, i) => {
    if (i) out.push({ t: 'br' });
    out.push(...parseInline(l.trim()));
  });
  return out;
}

function parseBlocks(lines) {
  const blocks = [];
  let para = [];
  const flush = () => {
    if (para.length) {
      blocks.push({ t: 'p', children: inlineLines(para) });
      para = [];
    }
  };

  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    const fence = /^```(.*)$/.exec(line);
    if (fence) {
      flush();
      const buf = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) buf.push(lines[i++]);
      i++; // closing fence (unclosed: rest of input is code)
      blocks.push({ t: 'code', lang: fence[1].trim(), text: buf.join('\n') });
      continue;
    }
    if (!line.trim()) {
      flush();
      i++;
      continue;
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    if (heading) {
      flush();
      blocks.push({ t: 'heading', level: heading[1].length, children: parseInline(heading[2].trim()) });
      i++;
      continue;
    }
    if (/^(?:-{3,}|\*{3,}|_{3,})\s*$/.test(line)) {
      flush();
      blocks.push({ t: 'hr' });
      i++;
      continue;
    }
    if (/^>\s?/.test(line)) {
      flush();
      const buf = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) buf.push(lines[i++].replace(/^>\s?/, ''));
      blocks.push({ t: 'quote', children: parseBlocks(buf) });
      continue;
    }
    const li = LIST_RE.exec(line);
    if (li) {
      flush();
      const ordered = /\d/.test(li[1].charAt(0));
      const items = [];
      while (i < lines.length) {
        const m = LIST_RE.exec(lines[i]);
        if (!m || /\d/.test(m[1].charAt(0)) !== ordered) break;
        const task = !ordered && /^\[([ xX])\]\s+(.*)$/.exec(m[2]);
        if (task) items.push({ task: true, checked: task[1] !== ' ', children: parseInline(task[2]) });
        else items.push({ children: parseInline(m[2]) });
        i++;
      }
      blocks.push({ t: 'list', ordered, items });
      continue;
    }
    if (line.includes('|') && i + 1 < lines.length) {
      const sep = splitRow(lines[i + 1]);
      if (sep.length > 1 && sep.every((c) => /^:?-+:?$/.test(c))) {
        flush();
        const head = splitRow(line).map((c) => parseInline(c));
        i += 2;
        const rows = [];
        while (i < lines.length && lines[i].includes('|'))
          rows.push(splitRow(lines[i++]).map((c) => parseInline(c)));
        blocks.push({ t: 'table', head, rows });
        continue;
      }
    }
    para.push(line);
    i++;
  }
  flush();
  return blocks;
}

export function parseMarkdown(src) {
  return parseBlocks((src ?? '').replace(/\r\n?/g, '\n').split('\n'));
}

/* ================= Markdown (standalone rendered-markdown control) ============ */
/* Subset: headings #–####, paragraphs, fenced code (+lang label), ul/ol + task
   lists, blockquote, hr, pipe tables, images, **bold**, *em*, ~~strike~~,
   `code`, [links](url), autolinks. No syntax highlighting — use code.jsx. */
export function Markdown(props) {
  const merged = mergeProps({ linkTarget: '_blank' }, props);
  const blocks = createMemo(() => parseMarkdown(merged.text));
  return (
    <div class={`fmd ${merged.class ?? ''}`}>
      <For each={blocks()}>{(b) => renderBlock(b, merged.linkTarget)}</For>
    </div>
  );
}

function renderBlock(b, target) {
  switch (b.t) {
    case 'p':
      return <p>{renderInlines(b.children, target)}</p>;
    case 'heading':
      return <Dynamic component={`h${b.level}`}>{renderInlines(b.children, target)}</Dynamic>;
    case 'code':
      return <pre class="fmd-code" data-lang={b.lang || undefined}><code>{b.text}</code></pre>;
    case 'list':
      return (
        <Dynamic component={b.ordered ? 'ol' : 'ul'}>
          <For each={b.items}>
            {(item) => (
              <li classList={{ 'fmd-task': !!item.task }}>
                <Show when={item.task}>
                  <input type="checkbox" checked={item.checked} disabled aria-hidden="true" />
                </Show>
                {renderInlines(item.children, target)}
              </li>
            )}
          </For>
        </Dynamic>
      );
    case 'quote':
      return <blockquote class="fmd-quote"><For each={b.children}>{(c) => renderBlock(c, target)}</For></blockquote>;
    case 'hr':
      return <hr class="fmd-hr" />;
    case 'table':
      return (
        <div class="fmd-table-wrap">
          <table class="fmd-table">
            <thead>
              <tr><For each={b.head}>{(cell) => <th>{renderInlines(cell, target)}</th>}</For></tr>
            </thead>
            <tbody>
              <For each={b.rows}>
                {(row) => <tr><For each={row}>{(cell) => <td>{renderInlines(cell, target)}</td>}</For></tr>}
              </For>
            </tbody>
          </table>
        </div>
      );
  }
}

function renderInlines(nodes, target) {
  return <For each={nodes}>{(n) => renderInline(n, target)}</For>;
}

function renderInline(n, target) {
  switch (n.t) {
    case 'text': return n.text;
    case 'br': return <br />;
    case 'code': return <code class="fmd-icode">{n.text}</code>;
    case 'strong': return <strong>{renderInlines(n.children, target)}</strong>;
    case 'em': return <em>{renderInlines(n.children, target)}</em>;
    case 'strike': return <s>{renderInlines(n.children, target)}</s>;
    case 'link': return <a href={n.href} target={target} rel="noopener noreferrer">{renderInlines(n.children, target)}</a>;
    case 'image': return <img class="fmd-img" src={n.src} alt={n.alt} loading="lazy" />;
  }
}

/* ================= time helpers ================================================ */
export function formatTime(at) {
  return new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(new Date(at));
}

function dayKey(at) {
  const d = new Date(at);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

export function formatDay(at) {
  const now = new Date();
  const key = dayKey(at);
  if (key === dayKey(now)) return 'Today';
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (key === dayKey(yesterday)) return 'Yesterday';
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(at));
}

function isoTime(at) {
  return new Date(at).toISOString();
}

/** "4.2 MB" — mono captions want the space before the unit. */
export function formatBytes(size) {
  const units = ['B', 'kB', 'MB', 'GB', 'TB'];
  let v = size;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return `${i === 0 ? v : v.toFixed(1)} ${units[i]}`;
}

/* ================= private inline SVGs ========================================= */
const ChevronRightSvg = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M9 6l6 6-6 6" />
  </svg>
);

const ArrowDownSvg = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <line x1="12" y1="4" x2="12" y2="20" /><path d="M6 14l6 6 6-6" />
  </svg>
);

const SendSvg = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M21 3L10 14" /><path d="M21 3l-7 18-4-7-7-4 18-7z" />
  </svg>
);

const FileSvg = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
    <path d="M14 3v5h5" />
  </svg>
);

const GlobeSvg = () => (
  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" aria-hidden="true">
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18" /><path d="M12 3a13.5 13.5 0 0 1 0 18 13.5 13.5 0 0 1 0-18z" />
  </svg>
);

/* ================= LinkCard ====================================================== */
/* Metadata can't be fetched client-side (CORS) — pass `meta`, or a
   server-backed `resolve`. Unresolvable URLs degrade to a plain anchor. */
const metaCache = new Map();

export function LinkCard(props) {
  const [fetched] = createResource(
    () => (!props.meta && props.resolve ? props.url : null),
    (url) => {
      let p = metaCache.get(url);
      if (!p) {
        p = props.resolve(url).catch(() => null);
        metaCache.set(url, p);
      }
      return p;
    },
  );
  const meta = () => props.meta ?? fetched();
  const loading = () => !props.meta && !!props.resolve && fetched.loading;
  const domain = () => {
    const m = meta();
    if (m?.domain) return m.domain;
    try {
      return new URL(props.url).hostname;
    } catch {
      return props.url;
    }
  };

  return (
    <Show
      when={meta() || loading()}
      fallback={
        <a class="fchat-linkplain" href={props.url} target="_blank" rel="noopener noreferrer">
          {props.url}
        </a>
      }
    >
      <a class="fchat-linkcard" href={props.url} target="_blank" rel="noopener noreferrer">
        <Show
          when={!loading()}
          fallback={
            <span class="fchat-linkcard-text" aria-hidden="true">
              <Skeleton width="90px" height="10px" />
              <Skeleton width="180px" height="12px" />
              <Skeleton width="220px" height="10px" />
            </span>
          }
        >
          <span class="fchat-linkcard-text">
            <span class="fchat-linkcard-domain">
              <Show when={meta()?.icon} fallback={<GlobeSvg />}>
                <img src={meta().icon} alt="" />
              </Show>
              {domain()}
            </span>
            <Show when={meta()?.title}>
              <span class="fchat-linkcard-title">{meta().title}</span>
            </Show>
            <Show when={meta()?.description}>
              <span class="fchat-linkcard-desc">{meta().description}</span>
            </Show>
          </span>
          <Show when={meta()?.image}>
            <img class="fchat-linkcard-thumb" src={meta().image} alt="" loading="lazy" />
          </Show>
        </Show>
      </a>
    </Show>
  );
}

/* ================= ChatToolCall ==================================================== */
/* Collapsible tool-call box; nested `children` recurse behind a left rail. */
export function ChatToolCall(props) {
  const [open, setOpen] = createSignal(!!props.tool.defaultOpen);
  const hasBody = () =>
    props.tool.args !== undefined || props.tool.result !== undefined || !!props.tool.children?.length;

  return (
    <div class="fchat-tool" classList={{ 'is-open': open() }}>
      <button type="button" class="fchat-tool-head" aria-expanded={open()} disabled={!hasBody()}
              onClick={() => setOpen((o) => !o)}>
        <ChevronRightSvg />
        <span class="fchat-tool-name">{props.tool.name}</span>
        <Show when={props.tool.summary}>
          <span class="fchat-tool-summary">{props.tool.summary}</span>
        </Show>
        <span class={`fchat-tool-status is-${props.tool.status}`}>
          <Show when={props.tool.status === 'running'} fallback={<span class="fchat-tool-dot" />}>
            <Spinner size={12} label="Running" />
          </Show>
          {props.tool.status}
        </span>
      </button>
      <Show when={open() && hasBody()}>
        <div class="fchat-tool-body">
          <Show when={props.tool.args !== undefined}>
            <div class="eyebrow">Arguments</div>
            <ToolPayload value={props.tool.args} />
          </Show>
          <Show when={props.tool.result !== undefined}>
            <div class="eyebrow">Result</div>
            <ToolPayload value={props.tool.result} />
          </Show>
          <Show when={props.tool.children?.length}>
            <div class="fchat-tool-children">
              <For each={props.tool.children}>{(child) => <ChatToolCall tool={child} />}</For>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
}

function ToolPayload(props) {
  return (
    <Show when={typeof props.value === 'string'} fallback={props.value}>
      <pre class="fmd-code"><code>{props.value}</code></pre>
    </Show>
  );
}

/* ================= ChatPrompt ======================================================= */
/* Interactive question box. `answer` present ⇒ answered: controls disable
   (native fieldset disable) and the chosen option is highlighted. */
export function ChatPrompt(props) {
  const [choice, setChoice] = createSignal(undefined);
  const [choices, setChoices] = createSignal([]);
  const name = createUniqueId();

  const answered = () => props.prompt.answer !== undefined;
  const answerArr = () => {
    const a = props.prompt.answer;
    return a === undefined ? [] : Array.isArray(a) ? a : [a];
  };
  const isChosen = (v) => answerArr().includes(v);
  const control = () => props.prompt.control;
  const options = () => control().options;

  return (
    <fieldset class="fchat-prompt" classList={{ 'is-answered': answered() }} disabled={answered()}>
      <legend class="fchat-prompt-q">{props.prompt.question}</legend>
      <Switch>
        <Match when={control().type === 'buttons'}>
          <div class="fchat-prompt-row">
            <For each={options()}>
              {(opt) => (
                <Button size="sm" variant={answered() && isChosen(opt.value) ? 'primary' : 'secondary'}
                        disabled={opt.disabled}
                        onClick={() => props.prompt.onAnswer?.(opt.value)}>
                  {opt.label}
                </Button>
              )}
            </For>
          </div>
        </Match>
        <Match when={control().type === 'radio'}>
          <div class="fchat-prompt-opts" role="radiogroup">
            <For each={options()}>
              {(opt) => (
                <Radio name={name} value={opt.value} disabled={opt.disabled}
                       checked={answered() ? isChosen(opt.value) : choice() === opt.value}
                       onChange={(v) => setChoice(v)}>
                  {opt.label}
                </Radio>
              )}
            </For>
          </div>
          <SubmitRow prompt={props.prompt} value={choice()} answered={answered()} />
        </Match>
        <Match when={control().type === 'checkbox'}>
          <div class="fchat-prompt-opts">
            <For each={options()}>
              {(opt) => (
                <Checkbox disabled={opt.disabled}
                          checked={answered() ? isChosen(opt.value) : choices().includes(opt.value)}
                          onChange={(on) =>
                            setChoices((cur) => (on ? [...cur, opt.value] : cur.filter((v) => v !== opt.value)))}>
                  {opt.label}
                </Checkbox>
              )}
            </For>
          </div>
          <SubmitRow prompt={props.prompt}
                     value={choices().length ? choices() : undefined} answered={answered()} />
        </Match>
        <Match when={control().type === 'select'}>
          <Select options={options()} placeholder={control().placeholder}
                  value={answered() ? answerArr()[0] : choice()}
                  onChange={(v) => setChoice(v)} />
          <SubmitRow prompt={props.prompt} value={choice()} answered={answered()} />
        </Match>
      </Switch>
      <Show when={answered()}>
        <div class="fchat-prompt-done">Answered</div>
      </Show>
    </fieldset>
  );
}

function SubmitRow(props) {
  return (
    <Show when={!props.answered}>
      <div class="fchat-prompt-row">
        <Button size="sm" variant="primary" disabled={props.value === undefined}
                onClick={() => props.value !== undefined && props.prompt.onAnswer?.(props.value)}>
          {props.prompt.submitLabel ?? 'Submit'}
        </Button>
      </div>
    </Show>
  );
}

/* ================= ChatMessage ======================================================== */
/* One message: text blocks render as bubbles, everything else (media, files,
   link cards, tool calls, prompts) as standalone rows in the message column. */
export function ChatMessage(props) {
  const blocks = () =>
    props.message.blocks ??
    (props.message.text !== undefined ? [{ kind: 'text', text: props.message.text }] : []);
  const label = () => {
    const who = props.participant?.name ?? props.message.author;
    return props.message.at ? `${who}, ${formatTime(props.message.at)}` : who;
  };

  return (
    <article class="fchat-msg"
             classList={{ 'is-pending': !!props.message.pending, 'is-error': !!props.message.error }}
             aria-label={label()}>
      <div class="fchat-msg-blocks">
        <For each={blocks()}>{(block) => <MessageBlock block={block} {...props} />}</For>
        <Show when={props.message.error}>
          <div class="fchat-msg-fail">{props.message.error}</div>
        </Show>
      </div>
      <Show when={props.showTime !== false && props.message.at}>
        <time class="fchat-msg-time" datetime={isoTime(props.message.at)}>
          {formatTime(props.message.at)}
        </time>
      </Show>
    </article>
  );
}

function MessageBlock(props) {
  const b = () => props.block;
  return (
    <Switch>
      <Match when={b().kind === 'text' && b()}>
        {(block) => (
          <div class="fchat-bubble">
            <Show when={(block().markdown ?? props.markdown) !== false}
                  fallback={<p class="fchat-plain">{block().text}</p>}>
              <Markdown text={block().text} />
            </Show>
          </div>
        )}
      </Match>
      <Match when={b().kind === 'image' && b()}>
        {(block) => {
          const media = (
            <span class="fchat-media" style={mediaStyle(block().width, block().height)}>
              <img src={block().src} alt={block().alt ?? ''} loading="lazy" />
            </span>
          );
          return (
            <Show when={block().href} fallback={media}>
              <a href={block().href} target="_blank" rel="noopener noreferrer">{media}</a>
            </Show>
          );
        }}
      </Match>
      <Match when={b().kind === 'video' && b()}>
        {(block) => (
          <span class="fchat-media is-video" style={mediaStyle(block().width, block().height)}>
            <video src={block().src} poster={block().poster} controls preload="metadata" />
          </span>
        )}
      </Match>
      <Match when={b().kind === 'file' && b()}>
        {(block) => {
          const row = (
            <>
              <Show when={block().icon} fallback={<FileSvg />}>
                <Icon of={block().icon} size={15} />
              </Show>
              <span class="fchat-file-name">{block().name}</span>
              <Show when={block().size !== undefined}>
                <span class="fchat-file-size">{formatBytes(block().size)}</span>
              </Show>
            </>
          );
          return (
            <Show when={block().href} fallback={<span class="fchat-file">{row}</span>}>
              <a class="fchat-file" href={block().href} download={block().name}>{row}</a>
            </Show>
          );
        }}
      </Match>
      <Match when={b().kind === 'link' && b()}>
        {(block) => <LinkCard url={block().url} meta={block().meta} resolve={props.resolveLink} />}
      </Match>
      <Match when={b().kind === 'tool' && b()}>
        {(block) => <ChatToolCall tool={block().tool} />}
      </Match>
      <Match when={b().kind === 'prompt' && b()}>
        {(block) => <ChatPrompt prompt={block().prompt} />}
      </Match>
      <Match when={b().kind === 'custom' && b()}>
        {(block) => block().render()}
      </Match>
    </Switch>
  );
}

function mediaStyle(width, height) {
  const style = {};
  if (width && height) style['aspect-ratio'] = `${width} / ${height}`;
  if (width) style.width = `min(${width}px, 100%)`;
  return style;
}

/* ================= ChatView =========================================================== */
/* Data-driven transcript. Owns message grouping, day dividers, the unread
   marker, typing row, and scroll behavior: pinned-to-bottom while the user is
   at the bottom, a "N new messages" jump pill when they've scrolled back, and
   scroll compensation when history is prepended via onReachTop. */
const NEAR_BOTTOM = 48;
const NEAR_TOP = 64;

export function ChatView(props) {
  const merged = mergeProps(
    { variant: 'direct', groupWindow: 5, dayDividers: true, showTimes: true, markdown: true },
    props,
  );
  const byId = createMemo(() => new Map(merged.participants.map((p) => [p.id, p])));

  const entries = createMemo(() => {
    const out = [];
    let group = null;
    let lastDay = null;
    let lastAt;
    const close = () => {
      if (group) out.push(group);
      group = null;
    };
    for (const item of merged.items) {
      const at = item.type === 'divider' ? undefined : item.at;
      if (merged.dayDividers && at !== undefined) {
        const day = dayKey(at);
        if (day !== lastDay) {
          close();
          out.push({ k: 'day', id: `fchat-day-${day}`, label: formatDay(at) });
          lastDay = day;
        }
      }
      if (item.type === 'event') {
        close();
        out.push({ k: 'event', id: item.id, item });
      } else if (item.type === 'divider') {
        close();
        out.push({ k: 'divider', id: item.id, item });
      } else {
        const gapOk =
          lastAt === undefined || item.at === undefined ||
          +new Date(item.at) - +new Date(lastAt) < merged.groupWindow * 60_000;
        if (!group || group.author !== item.author || !gapOk) {
          close();
          group = { k: 'group', id: item.id, author: item.author, at: item.at, messages: [] };
        }
        group.messages.push(item);
        lastAt = item.at;
      }
      if (merged.unreadAfter !== undefined && item.id === merged.unreadAfter) {
        close();
        out.push({ k: 'unread', id: 'fchat-unread' });
      }
    }
    close();
    return out;
  });

  const typers = createMemo(() =>
    (merged.typing ?? []).map((id) => byId().get(id)?.name ?? id).filter(Boolean),
  );

  /* ------ scroll behavior ------ */
  let scroller;
  let list;
  const [pinned, setPinned] = createSignal(true);
  const [newCount, setNewCount] = createSignal(0);
  let topLatched = false;
  let prevFirst;
  let prevLast;
  let prevHeight = 0;

  const toBottom = () => {
    scroller.scrollTop = scroller.scrollHeight;
  };
  const jump = () => {
    toBottom();
    setPinned(true);
    setNewCount(0);
  };
  const onScroll = () => {
    const nb = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < NEAR_BOTTOM;
    setPinned(nb);
    if (nb) setNewCount(0);
    if (scroller.scrollTop < NEAR_TOP) {
      if (!topLatched) {
        topLatched = true;
        merged.onReachTop?.();
      }
    } else {
      topLatched = false;
    }
  };

  onMount(() => {
    toBottom();
    prevHeight = scroller.scrollHeight;
    /* Re-stick as async content (images, link cards) grows the list. */
    const ro = new ResizeObserver(() => {
      if (pinned()) toBottom();
    });
    ro.observe(list);
    onCleanup(() => ro.disconnect());
  });

  createEffect(() => {
    const items = merged.items;
    const first = items[0]?.id;
    const last = items[items.length - 1]?.id;
    if (prevLast !== undefined && last !== prevLast) {
      if (pinned()) {
        toBottom();
      } else {
        const idx = items.findIndex((i) => i.id === prevLast);
        setNewCount((c) => c + (idx >= 0 ? items.length - 1 - idx : 1));
      }
    }
    if (prevFirst !== undefined && first !== prevFirst && last === prevLast && !pinned()) {
      /* History prepended — keep the viewport anchored to the old content. */
      scroller.scrollTop += scroller.scrollHeight - prevHeight;
      topLatched = false;
    }
    prevFirst = first;
    prevLast = last;
    prevHeight = scroller.scrollHeight;
  });

  const showAvatar = (author) => merged.variant === 'room' || author !== merged.self;
  const showName = () => merged.variant === 'room';

  return (
    <div class={`fchat ${merged.class ?? ''}`} style={merged.style}>
      <div class="fchat-scrollwrap">
        <div class="fchat-scroll" ref={scroller} role="log" aria-label="Conversation" onScroll={onScroll}>
          <div class="fchat-list" ref={list}>
            <For each={entries()}>
              {(entry) => (
                <Switch>
                  <Match when={entry.k === 'day' && entry}>
                    {(e) => <ChatDivider label={e().label} />}
                  </Match>
                  <Match when={entry.k === 'unread'}>
                    <div class="fchat-divider is-unread"><span>New</span></div>
                  </Match>
                  <Match when={entry.k === 'divider' && entry}>
                    {(e) => <ChatDivider label={e().item.label} />}
                  </Match>
                  <Match when={entry.k === 'event' && entry}>
                    {(e) => (
                      <div class="fchat-event">
                        {e().item.text}
                        <Show when={e().item.at}>
                          <time datetime={isoTime(e().item.at)}>{formatTime(e().item.at)}</time>
                        </Show>
                      </div>
                    )}
                  </Match>
                  <Match when={entry.k === 'group' && entry}>
                    {(e) => {
                      const self = () => e().author === merged.self;
                      const who = () => byId().get(e().author);
                      return (
                        <div class="fchat-group" classList={{ 'is-self': self() }}>
                          <div class="fchat-group-gutter">
                            <Show when={showAvatar(e().author)}>
                              <Avatar size="sm" name={who()?.name ?? e().author} src={who()?.avatar}
                                      status={who()?.status} />
                            </Show>
                          </div>
                          <div class="fchat-group-body">
                            <Show when={showName() || (merged.showTimes && e().at)}>
                              <div class="fchat-meta">
                                <Show when={showName()}>
                                  <span class="fchat-meta-name">{who()?.name ?? e().author}</span>
                                </Show>
                                <Show when={merged.showTimes && e().at}>
                                  <time datetime={isoTime(e().at)}>{formatTime(e().at)}</time>
                                </Show>
                              </div>
                            </Show>
                            <For each={e().messages}>
                              {(m) => (
                                <ChatMessage message={m} participant={who()} self={self()}
                                             showTime={merged.showTimes} markdown={merged.markdown}
                                             resolveLink={merged.resolveLink} />
                              )}
                            </For>
                          </div>
                        </div>
                      );
                    }}
                  </Match>
                </Switch>
              )}
            </For>
            <Show when={typers().length}>
              <ChatTyping names={typers()} />
            </Show>
          </div>
        </div>
        <Show when={!pinned() && newCount() > 0}>
          <button type="button" class="fchat-jump" onClick={jump}>
            <ArrowDownSvg />
            {newCount()} new message{newCount() === 1 ? '' : 's'}
          </button>
        </Show>
      </div>
      {merged.children}
    </div>
  );
}

/* ================= ChatDivider / ChatTyping =========================================== */
export function ChatDivider(props) {
  return (
    <div class="fchat-divider">
      <span>{props.label}</span>
    </div>
  );
}

export function ChatTyping(props) {
  const text = () => {
    const n = props.names;
    if (n.length === 1) return `${n[0]} is typing`;
    if (n.length === 2) return `${n[0]} and ${n[1]} are typing`;
    return 'Several people are typing';
  };
  return (
    <div class="fchat-typing" aria-live="polite">
      <span class="fchat-typing-dots" aria-hidden="true">
        <span /><span /><span />
      </span>
      {text()}
    </div>
  );
}

/* ================= ChatComposer ======================================================== */
/* Auto-growing message input. Enter sends (IME-safe), Shift+Enter breaks. */
export function ChatComposer(props) {
  const merged = mergeProps({ placeholder: 'Message', sendLabel: 'Send', maxRows: 8 }, props);
  const [inner, setInner] = createSignal('');
  const value = () => merged.value ?? inner();
  let area;

  const setValue = (v) => {
    setInner(v);
    merged.onChange?.(v);
  };
  const autogrow = () => {
    area.style.height = 'auto';
    const line = parseFloat(getComputedStyle(area).lineHeight) || 20;
    const max = line * merged.maxRows;
    area.style.height = `${Math.min(area.scrollHeight, max)}px`;
    area.style.overflowY = area.scrollHeight > max ? 'auto' : 'hidden';
  };
  const send = () => {
    const text = value().trim();
    if (!text || merged.disabled) return;
    merged.onSend(text);
    setValue('');
  };
  const onKeyDown = (e) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send();
    }
  };

  onMount(() => {
    if (merged.autofocus) area.focus();
  });
  createEffect(() => {
    value();
    autogrow();
  });

  return (
    <div class="fchat-composer">
      <Show when={merged.accessories}>
        <div class="fchat-composer-accessories">{merged.accessories}</div>
      </Show>
      <div class="fchat-composer-row">
        <Show when={merged.actions}>
          <div class="fchat-composer-actions">{merged.actions}</div>
        </Show>
        <div class="fchat-composer-field">
          <textarea ref={area} rows="1" value={value()} placeholder={merged.placeholder}
                    disabled={merged.disabled}
                    onInput={(e) => setValue(e.currentTarget.value)}
                    onKeyDown={onKeyDown} />
        </div>
        <button type="button" class="fchat-composer-send" aria-label={merged.sendLabel}
                title={merged.sendLabel} disabled={merged.disabled || !value().trim()}
                onClick={send}>
          <SendSvg />
        </button>
      </div>
    </div>
  );
}
