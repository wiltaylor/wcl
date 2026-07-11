/* Forge charts — optional zero-dep copy-in asset (needs console.css's
   "Charts" section). Static SVG, tokens only. Imports only solid-js.

   Categorical series colours use CHART_SERIES — a fixed, validated order
   (see "Chart colours" in reference/tokens.md; never cycle past it, fold
   extras into "Other" = --fg-2). Semantic data should pass tone: props
   instead; status hues never impersonate a series.

   Static this round: no hover/tooltip layer (future work). */

import { Show, For, Index, createSignal, createMemo, onMount, onCleanup, mergeProps } from 'solid-js';

/* Fixed categorical order — validated with the dataviz palette checker:
   min adjacent CVD ΔE 17.8 (dark) / 16.9 (light). Never reorder or cycle. */
export const CHART_SERIES = [
  'var(--accent)', 'var(--danger)', 'var(--success)', 'var(--warning)', 'var(--info)',
];
export const CHART_SERIES_BG = [
  'var(--accent-bg)', 'var(--danger-bg)', 'var(--success-bg)', 'var(--warning-bg)', 'var(--info-bg)',
];
export const CHART_OTHER = 'var(--fg-2)';  /* the "Other" fold — not a series slot */

export function seriesColor(i, tone) {
  if (tone) return `var(--${tone})`;
  return i < CHART_SERIES.length ? CHART_SERIES[i] : CHART_OTHER;
}
export function seriesBg(i, tone) {
  if (tone) return `var(--${tone}-bg)`;
  return i < CHART_SERIES_BG.length ? CHART_SERIES_BG[i] : 'color-mix(in oklab, var(--fg-2) 14%, transparent)';
}

/* 1/2/5-step "nice" ticks from 0 (charts here are zero-based magnitudes). */
export function niceTicks(max, n = 4) {
  if (!(max > 0)) return [0, 1];
  const raw = max / n;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((s) => s * mag).find((s) => s >= raw);
  const ticks = [];
  for (let v = 0; v <= max + step * 0.001; v += step) ticks.push(v);
  if (ticks[ticks.length - 1] < max) ticks.push(ticks[ticks.length - 1] + step);
  return ticks;
}

const fmt = (n) => {
  if (Math.abs(n) >= 1e6) return `${+(n / 1e6).toFixed(1)} M`;
  if (Math.abs(n) >= 1e3) return `${+(n / 1e3).toFixed(1)} k`;
  return `${+n.toFixed(2)}`;
};

/* Width-measuring wrapper: charts re-render at true pixel width (crisp text). */
function useMeasure() {
  const [w, setW] = createSignal(0);
  let el;
  onMount(() => {
    const ro = new ResizeObserver(([e]) => setW(e.contentRect.width));
    ro.observe(el);
    setW(el.clientWidth);
    onCleanup(() => ro.disconnect());
  });
  return [(node) => (el = node), w];
}

function Legend(props) {
  return (
    <Show when={(props.items?.length ?? 0) > 1}>
      <div class="fchart-legend">
        <For each={props.items}>
          {(it) => (
            <span style={{ display: 'inline-flex', 'align-items': 'center', gap: '6px' }}>
              <span class="fchart-swatch" style={{ background: it.color }} />
              {it.label}
              <Show when={it.pct != null}>
                <span class="fchart-pct">{it.pct} %</span>
              </Show>
            </span>
          )}
        </For>
      </div>
    </Show>
  );
}

/* ---------------- PieChart --------------------------------------------------- */
export function PieChart(props) {
  const merged = mergeProps({ size: 180, legend: true, showValues: true }, props);
  const total = () => merged.data.reduce((s, d) => s + d.value, 0);
  const slices = () => {
    const r = merged.size / 2;
    const inner = merged.donut ? r * 0.62 : 0;
    let angle = -Math.PI / 2;
    return merged.data.map((d, i) => {
      const frac = total() ? d.value / total() : 0;
      const a0 = angle, a1 = (angle += frac * 2 * Math.PI);
      const large = a1 - a0 > Math.PI ? 1 : 0;
      const p = (a, rad) => `${r + rad * Math.cos(a)} ${r + rad * Math.sin(a)}`;
      const d_ = inner
        ? `M ${p(a0, r)} A ${r} ${r} 0 ${large} 1 ${p(a1, r)} L ${p(a1, inner)} A ${inner} ${inner} 0 ${large} 0 ${p(a0, inner)} Z`
        : `M ${r} ${r} L ${p(a0, r)} A ${r} ${r} 0 ${large} 1 ${p(a1, r)} Z`;
      return { d: d_, color: seriesColor(i, d.tone), label: d.label, pct: Math.round(frac * 100) };
    });
  };
  return (
    <div class="fchart">
      <div style={{ display: 'flex', gap: '20px', 'align-items': 'center', 'flex-wrap': 'wrap' }}>
        <svg width={merged.size} height={merged.size} role="img" aria-label="Pie chart">
          <For each={slices()}>
            {(s) => <path class="fslice" d={s.d} fill={s.color} stroke="var(--bg-1)" stroke-width="2" />}
          </For>
          <Show when={merged.donut}>
            <text class="fchart-value" x={merged.size / 2} y={merged.size / 2} text-anchor="middle" dominant-baseline="central"
                  style={{ 'font-size': '14px', 'font-weight': '600', fill: 'var(--fg-0)' }}>
              {fmt(total())}
            </text>
          </Show>
        </svg>
        <Show when={merged.legend}>
          <div class="fchart-legend" style={{ 'flex-direction': 'column', 'align-items': 'flex-start', gap: '6px', 'padding-top': '0' }}>
            <For each={slices()}>
              {(s) => (
                <span style={{ display: 'inline-flex', 'align-items': 'center', gap: '6px' }}>
                  <span class="fchart-swatch" style={{ background: s.color }} />
                  {s.label}
                  <Show when={merged.showValues}>
                    <span class="fchart-pct">{s.pct} %</span>
                  </Show>
                </span>
              )}
            </For>
          </div>
        </Show>
      </div>
    </div>
  );
}

/* ---------------- LineChart -------------------------------------------------- */
const PAD = { l: 44, r: 12, t: 8, b: 22 };

export function LineChart(props) {
  const merged = mergeProps({ height: 220, legend: true, yTicks: 4 }, props);
  const [ref, w] = useMeasure();

  const geom = createMemo(() => {
    const width = Math.max(w(), 120);
    const xs = merged.series.flatMap((s) => s.points.map((p) => p.x));
    const ys = merged.series.flatMap((s) => s.points.map((p) => p.y));
    const xMin = Math.min(...xs), xMax = Math.max(...xs);
    const ticks = niceTicks(Math.max(...ys), merged.yTicks);
    const yMax = ticks[ticks.length - 1];
    const X = (x) => PAD.l + ((x - xMin) / Math.max(1e-9, xMax - xMin)) * (width - PAD.l - PAD.r);
    const Y = (y) => merged.height - PAD.b - (y / yMax) * (merged.height - PAD.t - PAD.b);
    return { width, ticks, X, Y, xMin, xMax };
  });

  return (
    <div class="fchart" ref={ref}>
      <svg width={geom().width} height={merged.height} role="img" aria-label="Line chart">
        <For each={geom().ticks}>
          {(t) => (
            <>
              <line class={t === 0 ? 'fchart-baseline' : 'fchart-grid'}
                    x1={PAD.l} x2={geom().width - PAD.r} y1={geom().Y(t)} y2={geom().Y(t)} />
              <text class="fchart-axis" x={PAD.l - 6} y={geom().Y(t)} text-anchor="end" dominant-baseline="central">{fmt(t)}</text>
            </>
          )}
        </For>
        <Show when={merged.xLabels}>
          <Index each={merged.xLabels}>
            {(lbl, i) => (
              <text class="fchart-axis" x={geom().X(i)} y={merged.height - 6} text-anchor="middle">{lbl()}</text>
            )}
          </Index>
        </Show>
        <For each={merged.series}>
          {(s, i) => {
            const color = () => seriesColor(i(), s.tone);
            const pts = () => s.points.map((p) => `${geom().X(p.x)},${geom().Y(p.y)}`).join(' ');
            return (
              <>
                <Show when={merged.area}>
                  <polygon fill={seriesBg(i(), s.tone)} stroke="none"
                           points={`${geom().X(s.points[0].x)},${geom().Y(0)} ${pts()} ${geom().X(s.points[s.points.length - 1].x)},${geom().Y(0)}`} />
                </Show>
                <polyline fill="none" stroke={color()} stroke-width="2"
                          stroke-linejoin="round" stroke-linecap="round" points={pts()} />
                <For each={s.points}>
                  {(p) => <circle cx={geom().X(p.x)} cy={geom().Y(p.y)} r="3"
                                  fill={color()} stroke="var(--bg-1)" stroke-width="2" />}
                </For>
              </>
            );
          }}
        </For>
      </svg>
      <Legend items={merged.legend ? merged.series.map((s, i) => ({ label: s.label, color: seriesColor(i, s.tone) })) : []} />
    </div>
  );
}

/* ---------------- BarChart --------------------------------------------------- */
export function BarChart(props) {
  const merged = mergeProps({ height: 220, yTicks: 4 }, props);
  const [ref, w] = useMeasure();
  const series = () => merged.series ?? [{ label: '', data: merged.data, tone: undefined }];
  const cats = () => (merged.series ? merged.series[0].data : merged.data).map((d) => d.label);

  const geom = createMemo(() => {
    const width = Math.max(w(), 120);
    const totals = cats().map((_, ci) =>
      merged.stacked
        ? series().reduce((s, sr) => s + (sr.data[ci]?.value ?? 0), 0)
        : Math.max(...series().map((sr) => sr.data[ci]?.value ?? 0)));
    const ticks = niceTicks(Math.max(...totals), merged.yTicks);
    const yMax = ticks[ticks.length - 1];
    const innerW = width - PAD.l - PAD.r;
    const band = innerW / Math.max(1, cats().length);
    const groupN = merged.stacked ? 1 : series().length;
    const barW = Math.min(24, (band * 0.7) / groupN);
    const Y = (y) => merged.height - PAD.b - (y / yMax) * (merged.height - PAD.t - PAD.b);
    return { width, ticks, band, barW, groupN, Y };
  });

  const barPath = (x, y0, y1, w_, r = 4) => {
    const h = y0 - y1;
    const rr = Math.min(r, h, w_ / 2);
    return `M ${x} ${y0} v ${-(h - rr)} q 0 ${-rr} ${rr} ${-rr} h ${w_ - 2 * rr} q ${rr} 0 ${rr} ${rr} v ${h - rr} z`;
  };

  return (
    <div class="fchart" ref={ref}>
      <svg width={geom().width} height={merged.height} role="img" aria-label="Bar chart">
        <For each={geom().ticks}>
          {(t) => (
            <>
              <line class={t === 0 ? 'fchart-baseline' : 'fchart-grid'}
                    x1={PAD.l} x2={geom().width - PAD.r} y1={geom().Y(t)} y2={geom().Y(t)} />
              <text class="fchart-axis" x={PAD.l - 6} y={geom().Y(t)} text-anchor="end" dominant-baseline="central">{fmt(t)}</text>
            </>
          )}
        </For>
        <Index each={cats()}>
          {(cat, ci) => {
            const cx = () => PAD.l + geom().band * ci + geom().band / 2;
            return (
              <>
                <text class="fchart-axis" x={cx()} y={merged.height - 6} text-anchor="middle">{cat()}</text>
                <Show when={merged.stacked}
                      fallback={
                        <For each={series()}>
                          {(sr, si) => {
                            const v = () => sr.data[ci]?.value ?? 0;
                            const x = () => cx() - (geom().barW * geom().groupN) / 2 + geom().barW * si();
                            return <path class="fbar" fill={seriesColor(si(), sr.tone ?? sr.data[ci]?.tone)}
                                         d={barPath(x(), geom().Y(0), geom().Y(v()), geom().barW)} />;
                          }}
                        </For>
                      }>
                  {(() => {
                    let acc = 0;
                    return (
                      <For each={series()}>
                        {(sr, si) => {
                          const v = sr.data[ci]?.value ?? 0;
                          const y0 = geom().Y(acc);
                          acc += v;
                          const y1 = geom().Y(acc);
                          const x = cx() - geom().barW / 2;
                          return <rect class="fbar" x={x} y={y1} width={geom().barW} height={Math.max(0, y0 - y1)}
                                       fill={seriesColor(si(), sr.tone)} stroke="var(--bg-1)" stroke-width="2" />;
                        }}
                      </For>
                    );
                  })()}
                </Show>
              </>
            );
          }}
        </Index>
      </svg>
      <Legend items={merged.series ? series().map((s, i) => ({ label: s.label, color: seriesColor(i, s.tone) })) : []} />
    </div>
  );
}

/* ---------------- GanttChart ------------------------------------------------- */
/* Dates: ISO YYYY-MM-DD strings (DatePicker convention) or day-index numbers. */
const DAY = 86400000;
const toMs = (v) => (typeof v === 'number' ? v * DAY : new Date(`${v}T00:00:00`).getTime());

export function GanttChart(props) {
  const merged = mergeProps({ labelWidth: 140, rowH: 28 }, props);
  const [ref, w] = useMeasure();

  const geom = createMemo(() => {
    const width = Math.max(w(), 320);
    const t0 = Math.min(...merged.tasks.map((t) => toMs(t.start)));
    const t1 = Math.max(...merged.tasks.map((t) => toMs(t.end)));
    const span = Math.max(DAY, t1 - t0);
    const days = span / DAY;
    const stepDays = days <= 14 ? 1 : days <= 90 ? 7 : 30;
    const innerW = width - 12;
    const X = (ms) => ((ms - t0) / span) * innerW;
    const ticks = [];
    for (let ms = t0; ms <= t1; ms += stepDays * DAY) ticks.push(ms);
    return { width, t0, t1, X, ticks, innerW };
  });
  const axisH = 20;
  const height = () => merged.tasks.length * merged.rowH + axisH;
  const dateLabel = (ms) => {
    const d = new Date(ms);
    return `${String(d.getDate()).padStart(2, '0')}/${String(d.getMonth() + 1).padStart(2, '0')}`;
  };
  const todayMs = () => {
    if (merged.today === false) return null;
    const t = merged.today ? toMs(merged.today) : new Date().setHours(0, 0, 0, 0);
    return t >= geom().t0 && t <= geom().t1 ? t : null;
  };

  return (
    <div class="fchart fchart-scroll">
      <div style={{ display: 'flex', 'min-width': '480px' }}>
        <div style={{ width: `${merged.labelWidth}px`, flex: 'none', 'padding-top': `${axisH}px` }}>
          <For each={merged.tasks}>
            {(t) => (
              <div class="fgantt-label" style={{ height: `${merged.rowH}px`, display: 'flex', 'align-items': 'center' }}>
                {t.label}
              </div>
            )}
          </For>
        </div>
        <div style={{ flex: 1, 'min-width': 0 }} ref={ref}>
          <svg width={geom().width} height={height()} role="img" aria-label="Gantt chart">
            <For each={geom().ticks}>
              {(ms) => (
                <>
                  <line class="fchart-grid" x1={geom().X(ms)} x2={geom().X(ms)} y1={axisH} y2={height()} />
                  <text class="fchart-axis" x={geom().X(ms)} y={12} text-anchor="middle">{dateLabel(ms)}</text>
                </>
              )}
            </For>
            <For each={merged.tasks}>
              {(t, i) => {
                const x0 = () => geom().X(toMs(t.start));
                const x1 = () => geom().X(toMs(t.end));
                const y = () => axisH + i() * merged.rowH + (merged.rowH - 16) / 2;
                const color = () => seriesColor(0, t.tone ?? 'accent');
                return (
                  <>
                    <rect x={x0()} y={y()} width={Math.max(2, x1() - x0())} height="16" rx="4"
                          fill={seriesBg(0, t.tone ?? 'accent')} />
                    <Show when={t.progress != null}>
                      <rect x={x0()} y={y()} width={Math.max(0, (x1() - x0()) * Math.min(100, t.progress) / 100)}
                            height="16" rx="4" fill={color()} />
                    </Show>
                  </>
                );
              }}
            </For>
            <Show when={todayMs() != null}>
              <line class="fchart-today" x1={geom().X(todayMs())} x2={geom().X(todayMs())} y1={axisH} y2={height()} />
            </Show>
          </svg>
        </div>
      </div>
    </div>
  );
}

/* ---------------- Sparkline -------------------------------------------------- */
export function Sparkline(props) {
  const merged = mergeProps({ width: 96, height: 28 }, props);
  const pts = () => {
    const min = Math.min(...merged.points), max = Math.max(...merged.points);
    const span = Math.max(1e-9, max - min);
    return merged.points.map((v, i) => ({
      x: 2 + (i / Math.max(1, merged.points.length - 1)) * (merged.width - 4),
      y: merged.height - 3 - ((v - min) / span) * (merged.height - 6),
    }));
  };
  const last = () => pts()[pts().length - 1];
  return (
    <svg class="fsparkline" width={merged.width} height={merged.height} role="img" aria-label="Trend"
         style={{ display: 'inline-block', 'vertical-align': 'middle' }}>
      <polyline fill="none" stroke="var(--fg-2)" stroke-width="1.5" stroke-linejoin="round"
                points={pts().map((p) => `${p.x},${p.y}`).join(' ')} />
      <circle cx={last().x} cy={last().y} r="2.5" fill={merged.tone ? `var(--${merged.tone})` : 'var(--accent)'} />
    </svg>
  );
}
