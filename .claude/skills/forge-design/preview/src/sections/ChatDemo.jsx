import { createSignal } from 'solid-js';
import { PageHead, Card, Badge, IconButton } from '@forge/ui.jsx';
import { ChatView, ChatComposer, Markdown } from '@forge/chat.jsx';
import { Paperclip, Plus } from 'lucide-solid';

const now = Date.now();
const ago = (min) => new Date(now - min * 60_000);

const svgUri = (label, bg) =>
  `data:image/svg+xml,${encodeURIComponent(
    `<svg xmlns='http://www.w3.org/2000/svg' width='360' height='240'>` +
    `<rect width='100%' height='100%' fill='${bg}'/>` +
    `<text x='50%' y='52%' fill='#ECEEF2' font-family='sans-serif' font-size='18' text-anchor='middle'>${label}</text></svg>`,
  )}`;

/* ---------------- Direct (1:1) ------------------------------------------------ */
function DirectDemo() {
  let nextId = 1;
  const [items, setItems] = createSignal([
    { id: 'd1', author: 'sam', at: ago(32), text: 'Hey — did the **staging deploy** go out? The release notes still say `v0.8.1`.' },
    { id: 'd2', author: 'me', at: ago(30), text: 'Going out now. Changes:\n- new ingest pipeline\n- retry budget fix\n- *smaller* docker image' },
    { id: 'd3', author: 'me', at: ago(29), blocks: [
      { kind: 'image', src: svgUri('deploy graph', '#1E232C'), alt: 'Deploy timing graph', width: 360, height: 240 },
    ] },
    { id: 'd4', author: 'sam', at: ago(20), blocks: [
      { kind: 'text', text: 'Nice. Docs for the pipeline are here:' },
      { kind: 'link', url: 'https://docs.example.dev/ingest', meta: {
        url: 'https://docs.example.dev/ingest',
        title: 'Ingest pipeline — Example docs',
        description: 'Architecture, backpressure rules and the retry budget for the streaming ingest pipeline.',
        domain: 'docs.example.dev',
        image: svgUri('docs', '#252B36'),
      } },
    ] },
    { id: 'd5', author: 'sam', at: ago(19), blocks: [
      { kind: 'file', name: 'ingest-rollout.pdf', size: 4_200_000 },
    ] },
    { id: 'd6', author: 'me', at: ago(2), text: 'Sending the summary to the team now.', pending: true },
    { id: 'd7', author: 'me', at: ago(1), text: 'And the metrics dashboard link.', error: 'Not delivered — retry' },
  ]);
  const [typing, setTyping] = createSignal([]);

  const send = (text) => {
    setItems((cur) => [...cur, { id: `dm-${nextId++}`, author: 'me', at: new Date(), text }]);
    setTyping(['sam']);
    setTimeout(() => {
      setTyping([]);
      setItems((cur) => [...cur, {
        id: `dm-${nextId++}`, author: 'sam', at: new Date(),
        text: 'Got it — *thanks*! I will take a look after standup.',
      }]);
    }, 1400);
  };

  return (
    <ChatView
      style={{ height: '100%' }}
      variant="direct"
      self="me"
      participants={[
        { id: 'me', name: 'Wil Taylor', status: 'success' },
        { id: 'sam', name: 'Sam Chen', status: 'success' },
      ]}
      items={items()}
      typing={typing()}
    >
      <ChatComposer
        onSend={send}
        placeholder="Message Sam"
        actions={<IconButton icon={Paperclip} label="Attach" />}
      />
    </ChatView>
  );
}

/* ---------------- Room -------------------------------------------------------- */
function RoomDemo() {
  let nextId = 1;
  const yesterday = (min) => new Date(now - 24 * 60 * 60_000 - min * 60_000);
  const [items, setItems] = createSignal([
    { id: 'r1', author: 'ana', at: yesterday(200), text: 'Rolling restart of the ingest workers done. Error rate back under **0.1%**.' },
    { id: 'r2', author: 'ben', at: yesterday(190), text: 'Confirmed from the dashboard side.' },
    { id: 'r0', type: 'event', at: yesterday(60), text: 'Priya joined #ops' },
    { id: 'r3', author: 'priya', at: ago(95), text: 'Morning! Picking up the pager today.' },
    { id: 'r4', author: 'priya', at: ago(94), text: 'First up: the `disk-pressure` alert on node 7.' },
    { id: 'r5', author: 'priya', at: ago(93), text: 'Looks like log rotation stalled — clearing it now.' },
    { id: 'r6', author: 'ana', at: ago(80), text: 'Thanks. Watch the compaction queue too, it spiked overnight.' },
    { id: 'r7', author: 'ben', at: ago(12), text: 'Deploy window opens at 14:00 — anything blocking?' },
    { id: 'r8', author: 'me', at: ago(3), text: 'Nothing from my side. Ship it.' },
  ]);
  const [typing] = createSignal(['ana', 'priya']);

  const send = (text) =>
    setItems((cur) => [...cur, { id: `rm-${nextId++}`, author: 'me', at: new Date(), text }]);

  return (
    <ChatView
      style={{ height: '100%' }}
      variant="room"
      self="me"
      unreadAfter="r6"
      participants={[
        { id: 'me', name: 'Wil Taylor', status: 'success' },
        { id: 'ana', name: 'Ana Ortiz', status: 'success' },
        { id: 'ben', name: 'Ben Adeyemi', status: 'warning' },
        { id: 'priya', name: 'Priya Nair', status: 'neutral' },
      ]}
      items={items()}
      typing={typing()}
    >
      <ChatComposer onSend={send} placeholder="Message #ops" />
    </ChatView>
  );
}

/* ---------------- Assistant transcript ----------------------------------------- */
function AssistantDemo() {
  let nextId = 1;
  const [answers, setAnswers] = createSignal({});
  const [extra, setExtra] = createSignal([]);
  const [older, setOlder] = createSignal([]);
  const answer = (id) => (value) => setAnswers((cur) => ({ ...cur, [id]: value }));

  const resolveLink = (url) =>
    new Promise((resolve) =>
      setTimeout(() => resolve({
        url,
        title: 'forge — dark-default design system',
        description: 'SolidJS components, tokens, charts, node graph and a chat kit for dense technical tools.',
        domain: 'github.com',
        image: svgUri('repo', '#1E232C'),
      }), 1500),
    );

  const prompt = (id, question, control) => ({
    kind: 'prompt',
    prompt: { id, question, control, answer: answers()[id], onAnswer: answer(id) },
  });

  const items = () => [
    ...older(),
    { id: 'a1', author: 'me', at: ago(14), text: 'Find the flaky test in CI and fix it.' },
    { id: 'a2', author: 'bot', at: ago(13), blocks: [
      { kind: 'text', text: 'Scanning the last 20 CI runs for retried tests.' },
      { kind: 'tool', tool: {
        name: 'search_ci_logs', status: 'success', summary: '20 runs',
        args: '{ "query": "retry", "runs": 20 }',
        result: '3 hits: net/socket.test.ts, auth/token.test.ts (2×)',
      } },
      { kind: 'tool', tool: {
        name: 'run_tests', status: 'error', summary: 'auth/token.test.ts',
        args: '{ "file": "auth/token.test.ts", "repeat": 50 }',
        result: 'Failed 7/50 — TokenExpiry off by timezone offset',
      } },
      { kind: 'tool', tool: {
        name: 'fix_and_verify', status: 'running', summary: 'patching test clock',
        defaultOpen: true,
        args: '{ "strategy": "freeze_clock" }',
        children: [
          { name: 'edit_file', status: 'success', summary: 'auth/token.test.ts', result: '+4 −2' },
          { name: 'run_tests', status: 'running', summary: 'repeat 50' },
        ],
      } },
    ] },
    { id: 'a3', author: 'bot', at: ago(11), blocks: [
      { kind: 'text', text: 'While that runs — the fix touches the public repo:' },
      { kind: 'link', url: 'https://github.com/wiltaylor/forge' },
      prompt('p1', 'Open a PR when the tests pass?', {
        type: 'buttons',
        options: [{ value: 'yes', label: 'Open PR' }, { value: 'draft', label: 'Draft PR' }, { value: 'no', label: 'Not yet' }],
      }),
    ] },
    { id: 'a4', author: 'bot', at: ago(9), blocks: [
      prompt('p2', 'Which suites should run in the verification pass?', {
        type: 'checkbox',
        options: [
          { value: 'unit', label: 'Unit' }, { value: 'integration', label: 'Integration' },
          { value: 'e2e', label: 'End-to-end' },
        ],
      }),
      prompt('p3', 'Pick a merge strategy', {
        type: 'radio',
        options: [
          { value: 'squash', label: 'Squash and merge' }, { value: 'rebase', label: 'Rebase' },
          { value: 'merge', label: 'Merge commit' },
        ],
      }),
      prompt('p4', 'Notify which channel when done?', {
        type: 'select', placeholder: 'Choose a channel…',
        options: [
          { value: 'ops', label: '#ops' }, { value: 'eng', label: '#engineering' }, { value: 'none', label: 'No one' },
        ],
      }),
    ] },
    ...extra(),
  ];

  const burst = () =>
    setExtra((cur) => [
      ...cur,
      ...Array.from({ length: 10 }, (_, i) => ({
        id: `burst-${nextId}-${i}`, author: 'bot', at: new Date(),
        text: `Verification batch ${nextId}: run ${i + 1}/10 passed in ${(Math.random() * 3 + 1).toFixed(1)} s.`,
      })),
    ]) || nextId++;

  const loadOlder = () => {
    if (older().length >= 40) return;
    const base = older().length;
    setOlder((cur) => [
      ...Array.from({ length: 20 }, (_, i) => ({
        id: `old-${base + i}`, author: i % 2 ? 'me' : 'bot', at: ago(300 - base - i),
        text: `Earlier context #${base + i + 1} — scrollback loaded on demand.`,
      })),
      ...cur,
    ]);
  };

  const send = (text) =>
    setExtra((cur) => [...cur, { id: `ask-${nextId++}`, author: 'me', at: new Date(), text }]);

  return (
    <ChatView
      style={{ height: '100%' }}
      variant="direct"
      self="me"
      participants={[
        { id: 'me', name: 'Wil Taylor' },
        { id: 'bot', name: 'Forge Assistant', status: 'info' },
      ]}
      items={items()}
      resolveLink={resolveLink}
      onReachTop={loadOlder}
    >
      <ChatComposer
        onSend={send}
        placeholder="Ask the assistant"
        actions={<IconButton icon={Plus} label="Simulate burst" onClick={burst} />}
      />
    </ChatView>
  );
}

/* ---------------- Markdown control ---------------------------------------------- */
const MD_SAMPLE = `# Release notes

Ship **v0.9** with the *new* chat kit — details in [the docs](https://docs.example.dev).

## Checklist
- [x] markdown control
- [ ] screenshots
- regular bullet with \`inline code\`

| Package | Size |
|---------|------|
| @forge/ui | 51 kB |
| @forge/chat | 18 kB |

> Zero dependencies — raw HTML like <script>alert(1)</script> stays literal text.

\`\`\`ts
const blocks = parseMarkdown(text);
\`\`\`

Autolinks work too: https://forge.example.dev — and unsafe ones don't: [nope](javascript:alert(1))`;

function MarkdownDemo() {
  const [text, setText] = createSignal(MD_SAMPLE);
  return (
    <div style={{ display: 'grid', 'grid-template-columns': 'repeat(auto-fit, minmax(320px, 1fr))', gap: '12px', padding: '12px' }}>
      <textarea
        value={text()}
        onInput={(e) => setText(e.currentTarget.value)}
        style={{
          'min-height': '360px', resize: 'vertical', padding: '12px',
          background: 'var(--bg-0)', color: 'var(--fg-1)', border: '1px solid var(--border)',
          'border-radius': 'var(--r-md)', 'font-family': 'var(--font-mono)', 'font-size': '12px',
        }}
      />
      <div style={{ padding: '4px 8px', 'min-width': 0 }}>
        <Markdown text={text()} />
      </div>
    </div>
  );
}

/* ---------------- Section ---------------------------------------------------------- */
export default function ChatDemo() {
  return (
    <>
      <PageHead
        title="Chat"
        sub="Conversation view for 1:1 and rooms — message grouping, tool-call boxes, interactive prompts, media, link cards and a markdown control"
      />
      <div style={{ display: 'grid', 'grid-template-columns': 'repeat(auto-fit, minmax(340px, 1fr))', gap: '16px' }}>
        <Card padded={false} title="Direct (1:1)" action={<Badge tone="accent">composer round-trip</Badge>}>
          <div style={{ height: '480px' }}><DirectDemo /></div>
        </Card>
        <Card padded={false} title="Room" action={<Badge>day dividers · unread · events</Badge>}>
          <div style={{ height: '480px' }}><RoomDemo /></div>
        </Card>
      </div>
      <Card padded={false} title="Assistant transcript"
            action={<Badge tone="accent">tool calls · prompts · resolver</Badge>}
            class="fchat-demo-tall">
        <div style={{ height: '560px' }}><AssistantDemo /></div>
      </Card>
      <Card padded={false} title="Markdown" action={<Badge>zero-dep · XSS-safe</Badge>}>
        <MarkdownDemo />
      </Card>
    </>
  );
}
