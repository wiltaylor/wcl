/* The audit view: what a git range did to the wskill, and what is wrong
   with what it did.

   ONE view, two representations, health as a header strip. The changelog IS
   the surface — the metrics that got WORSE render in the header beside the
   counts, so the triage number and the named units are on one screen, and
   there is no separate health pane (a health report names no unit).

   The graph is the UNION graph — before ∪ after — with removals ghosted in
   red rather than absent. That is the one thing the live graph structurally
   cannot do, and it is why this is a distinct view over a distinct model
   rather than a mode on GraphView: the live graph draws what exists, and
   half of an audit is what stopped existing.

   Findings ride the changed rows as tags, scoped to the diff, so the view
   answers "what changed, and what is wrong with what changed" in one pass.
   The three severities keep their distinction in the styling: a CANDIDATE
   is a nomination, not a defect, and must never render as an error.

   Everything that isn't rendering — sections, link churn, the header
   numbers, the drawn slice of the union graph — is preview/audit.js. */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { RefreshCw } from 'lucide-solid';
import { Button, Input, Spinner, ToggleGroup, toast } from '@forge/ui';

import { api } from '../../api';
import { openFile } from '../../state/buffers';
import { revealSpan } from '../../state/views';
import { activeEntry, selected } from '../../state/sites';
import {
  designTab,
  designTabOptions,
  exitDesign,
  pendingAuditRange,
  setDesignTab,
  setPendingAuditRange,
} from '../../state/design';
import {
  countsText,
  graphModel,
  healthTally,
  newsRows,
  openTarget,
  rangeLabel,
  severityTally,
  worseMetrics,
} from '../../preview/audit';

/** How the range reads when nobody has typed one: the previous commit
    against the working tree, the library's own default. An authoring
    session's output is usually the last commit, and often not committed at
    all yet. */
const DEFAULT_RANGE = 'HEAD~1';

export default function AuditView() {
  // A curator result is a one-shot navigation request: consume its exact
  // before..after range on mount, then let later manual visits use the
  // audit's ordinary default again.
  const requestedRange = pendingAuditRange();
  const [range, setRange] = createSignal(requestedRange ?? DEFAULT_RANGE);
  if (requestedRange) setPendingAuditRange(null);
  const [data, setData] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [loading, setLoading] = createSignal(false);
  const [pane, setPane] = createSignal('log'); // 'log' | 'graph'

  /** The wskill itself, never a projection entry: an audit is per-wskill,
      and the registry path is the one name that means the same thing at
      both ends of the range. */
  const target = () => selected()?.registry ?? activeEntry();

  const run = async (spec) => {
    const entry = target();
    if (!entry) return;
    setLoading(true);
    const res = await api.audit(entry, spec ?? range());
    setLoading(false);
    if (!res.ok) {
      setError(res.error);
      setData(null);
      return;
    }
    setError(null);
    setData(res);
  };

  // Only a wskill has an audit; picking a plain site drops back to the
  // canvas rather than leaving another document's audit on screen.
  let loadedKey = null;
  createEffect(() => {
    if (selected() && !selected().wskill) {
      setDesignTab('canvas');
      return;
    }
    const key = target();
    if (!key || key === loadedKey) return;
    loadedKey = key;
    setData(null);
    setError(null);
    run();
  });

  /* Follow a row to its source — as far as the range honestly allows. A
     span only addresses the working tree when the after end IS the working
     tree; against a commit the file is opened without a selection rather
     than pointing confidently at the wrong bytes. */
  const openCode = async (n) => {
    const target = openTarget(data(), n);
    if (target === 'none') return;
    exitDesign();
    const res = await openFile(n.file);
    if (!res.ok) {
      toast(res.error, { tone: 'danger', duration: 6000 });
      return;
    }
    if (target === 'span') revealSpan(n.file, n.span.start, n.span.end);
  };

  return (
    <div class="ed-audit">
      <div class="ed-design-note">
        <ToggleGroup
          options={designTabOptions()}
          value={designTab()}
          onChange={(t) => setDesignTab(t)}
        />
        <span class="ed-design-page">wskill audit</span>
        <Input
          class="ed-audit-range"
          value={range()}
          placeholder="HEAD~1 · a..b · main..."
          onInput={(e) => setRange(e.currentTarget.value)}
          onKeyDown={(e) => e.key === 'Enter' && run()}
        />
        <Button size="sm" disabled={loading()} onClick={() => run()}>
          <RefreshCw size={13} /> Audit
        </Button>
        <Show when={loading()}>
          <Spinner size={12} label="Reading both revisions" />
        </Show>
        <span class="spacer" />
        <ToggleGroup
          options={[
            { value: 'log', label: 'Changed' },
            { value: 'graph', label: 'Graph' },
          ]}
          value={pane()}
          onChange={setPane}
        />
      </div>

      <Show when={error()}>
        <div class="ed-audit-error">{error()}</div>
      </Show>

      <Show when={data()} fallback={<Show when={!error()}><div class="ed-empty">Reading the range…</div></Show>}>
        <HeaderStrip data={data()} />
        <div class="ed-audit-body">
          <Show when={pane() === 'log'}>
            <Changelog data={data()} onOpen={openCode} />
          </Show>
          <Show when={pane() === 'graph'}>
            <UnionGraph data={data()} />
          </Show>
        </div>
      </Show>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Header strip — the counts, the severities, and the metrics that got worse
// ---------------------------------------------------------------------------

function HeaderStrip(props) {
  const tally = () => severityTally(props.data);
  const health = () => healthTally(props.data);
  return (
    <div class="ed-audit-head">
      <span class="ed-audit-range-label" title="The two resolved ends of the range">
        {rangeLabel(props.data)}
      </span>
      <span class="ed-audit-counts">
        <b>{countsText(props.data.summary?.units)}</b> units
      </span>
      <span class="ed-audit-counts">
        <b>{countsText(props.data.summary?.indexes)}</b> indexes
      </span>
      <span class="ed-audit-counts">
        <b>
          {countsText({
            added: props.data.summary?.edges?.added,
            removed: props.data.summary?.edges?.removed,
            modified: 0,
          })}
        </b>{' '}
        links
      </span>
      <Show when={tally().error > 0}>
        <span class="ed-audit-sev is-error">{tally().error} errors</span>
      </Show>
      <Show when={tally().warn > 0}>
        <span class="ed-audit-sev is-warn">{tally().warn} warnings</span>
      </Show>
      {/* A nomination, never a defect — counted apart and styled apart. */}
      <Show when={tally().candidate > 0}>
        <span class="ed-audit-sev is-candidate">{tally().candidate} candidates</span>
      </Show>
      {/* Health has no pane of its own: the metrics that moved the wrong
          way ride here, beside the counts, and the named units stay the
          surface below. */}
      <span class="ed-audit-health">
        <b classList={{ 'is-worse': health().worse > 0 }}>{health().worse}</b>
        <span class="ed-audit-dim">of {health().total} metrics worse</span>
        <For each={worseMetrics(props.data)}>
          {(m) => (
            <span class="ed-audit-metric">
              {m.label}{' '}
              <b>
                {m.before} → {m.after}
              </b>
            </span>
          )}
        </For>
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// A · the changelog
// ---------------------------------------------------------------------------

/** The marker a row wears. The view's own glyphs, not the model's — one of
    the four sections (`broken`) is not a `Change` at all, and a rendered
    row uses the typographic minus the CLI's plain-text one cannot. */
const MARKER = { added: '+', removed: '−', modified: '~', broken: '!' };

/** What following a row does, so the row can say so before it is clicked
    rather than after ({@link openTarget}). */
const OPEN_HINT = {
  none: 'Removed in this range — there is no current file to open',
  file: 'Open the file (the span addresses the compared commit, not the working tree)',
  span: 'Open the file at this block',
};

function Changelog(props) {
  const sections = () => newsRows(props.data);
  return (
    <div class="ed-audit-log">
      <Show when={sections().length === 0}>
        <div class="ed-empty">Nothing changed in this range.</div>
      </Show>
      <For each={sections()}>
        {(s) => (
          <section class="ed-audit-section">
            <h2>
              {s.label} — {s.rows.length}
            </h2>
            <For each={s.rows}>
              {(row) => (
                <Row
                  row={row}
                  section={s.key}
                  target={openTarget(props.data, row.node)}
                  onOpen={props.onOpen}
                />
              )}
            </For>
          </section>
        )}
      </For>
    </div>
  );
}

function Row(props) {
  const n = () => props.row.node;
  return (
    <div class="ed-audit-rowgroup">
      <div
        class="ed-audit-row"
        classList={{ [`is-${props.section}`]: true, 'is-inert': props.target === 'none' }}
        onClick={() => props.onOpen(n())}
        title={`${n().file}\n${OPEN_HINT[props.target]}`}
      >
        <span class="ed-audit-marker">{MARKER[props.section]}</span>
        <span class="ed-audit-kind">{n().kind}</span>
        <span class="ed-audit-id">{n().id}</span>
        <span class="ed-audit-title">{n().title}</span>
        <For each={n().changed ?? []}>
          {(aspect) => <span class="ed-audit-tag">{aspect}</span>}
        </For>
        {/* Severity is the whole point of the tag styling: a candidate is a
            nomination and must not wear an error's badge. */}
        <For each={n().findings ?? []}>
          {(f) => (
            <span class={`ed-audit-tag is-${f.severity}`} title={`${f.rule}: ${f.message}`}>
              {f.rule}
            </span>
          )}
        </For>
      </div>
      <For each={props.row.edges}>
        {(e) => (
          <div class="ed-audit-edge" classList={{ [`is-${e.change}`]: true }}>
            <span class="ed-audit-marker">{MARKER[e.change]}</span>
            <span class="ed-audit-kind">{e.kind}</span>
            <span class="ed-audit-id">→ {e.to}</span>
          </div>
        )}
      </For>
    </div>
  );
}

// ---------------------------------------------------------------------------
// B · the union graph — before ∪ after, removals ghosted
// ---------------------------------------------------------------------------

function UnionGraph(props) {
  const [onlyChanged, setOnlyChanged] = createSignal(true);
  const [viewBox, setViewBox] = createSignal(null);
  const model = () => graphModel(props.data, { onlyChanged: onlyChanged() });

  // The box follows whichever slice is shown, until the reader pans or
  // zooms — then it is theirs.
  let touched = false;
  createEffect(() => {
    const box = model().box;
    if (!touched) setViewBox(box);
  });

  let svg;
  let drag = null;
  const onPointerDown = (e) => {
    drag = { x: e.clientX, y: e.clientY, vb: { ...viewBox() } };
    svg.setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e) => {
    if (!drag) return;
    touched = true;
    const scale = viewBox().w / svg.clientWidth;
    setViewBox({
      ...drag.vb,
      x: drag.vb.x - (e.clientX - drag.x) * scale,
      y: drag.vb.y - (e.clientY - drag.y) * scale,
    });
  };
  const onPointerUp = () => {
    drag = null;
  };
  const onWheel = (e) => {
    e.preventDefault();
    touched = true;
    const vb = viewBox();
    const factor = e.deltaY > 0 ? 1.15 : 1 / 1.15;
    const mx = vb.x + (e.offsetX / svg.clientWidth) * vb.w;
    const my = vb.y + (e.offsetY / svg.clientHeight) * vb.h;
    setViewBox({
      x: mx - (mx - vb.x) * factor,
      y: my - (my - vb.y) * factor,
      w: vb.w * factor,
      h: vb.h * factor,
    });
  };

  const at = (key) => {
    const n = model().byKey.get(key);
    return n ? { x: n.x + n.w / 2, y: n.y + n.h / 2 } : null;
  };
  const path = (e) => {
    const a = at(e.from);
    const b = at(e.to);
    return a && b ? `M ${a.x} ${a.y} L ${b.x} ${b.y}` : '';
  };

  return (
    <div class="ed-audit-graph">
      <div class="ed-audit-ctl">
        <label>
          <input
            type="checkbox"
            checked={onlyChanged()}
            onChange={(e) => setOnlyChanged(e.currentTarget.checked)}
          />{' '}
          only what changed
        </label>
        <span class="ed-audit-legend">
          <span>
            <i class="ed-audit-sw is-added" /> added
          </span>
          <span>
            <i class="ed-audit-sw is-modified" /> modified
          </span>
          <span>
            <i class="ed-audit-sw is-unchanged" /> unchanged
          </span>
          <span>
            <i class="ed-audit-sw is-removed" /> removed
          </span>
        </span>
        {/* Say what is drawn, or the header's link counts (which include
            pins) read as a promise this pane doesn't keep. */}
        <span class="ed-audit-dim">
          before ∪ after, unit links only — a removal is ghosted, never absent; index
          pins are changelog rows
        </span>
      </div>
      <Show when={viewBox()}>
        <svg
          ref={svg}
          class="ed-audit-svg"
          viewBox={`${viewBox().x} ${viewBox().y} ${viewBox().w} ${viewBox().h}`}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onWheel={onWheel}
        >
          <For each={model().edges}>
            {(e) => <path d={path(e)} class={`ed-audit-gedge is-${e.change}`} />}
          </For>
          <For each={model().nodes}>
            {(n) => (
              <g class={`ed-audit-gnode is-${n.change}`} transform={`translate(${n.x}, ${n.y})`}>
                <rect width={n.w} height={n.h} rx="8" />
                <text x={n.w / 2} y="20">
                  {n.title}
                </text>
                <text x={n.w / 2} y="33" class="ed-audit-gkind">
                  {n.kind}
                </text>
                <title>
                  {n.id} — {n.kind} ({n.change})
                  {(n.findings ?? []).map((f) => `\n${f.severity}: ${f.message}`).join('')}
                </title>
              </g>
            )}
          </For>
        </svg>
      </Show>
    </div>
  );
}
