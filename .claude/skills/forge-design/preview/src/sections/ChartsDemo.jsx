import { createSignal } from 'solid-js';
import { PageHead, Card, Grid, Stat, Toggle } from '@forge/ui.jsx';
import { PieChart, LineChart, BarChart, GanttChart, Sparkline } from '@forge/charts.jsx';
import { Flowchart } from '@forge/graph.jsx';

const REQUESTS = [
  { label: 'dgx', points: [4, 6, 5, 9, 11, 10, 14, 13, 17, 16].map((y, x) => ({ x, y })) },
  { label: 'severus', points: [2, 3, 3, 4, 6, 5, 7, 8, 7, 9].map((y, x) => ({ x, y })) },
  { label: 'helios', points: [1, 1, 2, 2, 3, 4, 3, 5, 6, 5].map((y, x) => ({ x, y })) },
];

const DEPLOYS = [
  { label: 'Mon', value: 6 }, { label: 'Tue', value: 9 }, { label: 'Wed', value: 4 },
  { label: 'Thu', value: 12 }, { label: 'Fri', value: 8 },
];
const DEPLOY_SERIES = [
  { label: 'succeeded', tone: 'success', data: DEPLOYS.map((d) => ({ label: d.label, value: d.value })) },
  { label: 'failed', tone: 'danger', data: [1, 2, 0, 3, 1].map((v, i) => ({ label: DEPLOYS[i].label, value: v })) },
];

const TASKS = [
  { id: 't1', label: 'Model download', start: '2026-07-01', end: '2026-07-04', progress: 100, tone: 'success' },
  { id: 't2', label: 'KitOps import', start: '2026-07-03', end: '2026-07-07', progress: 100 },
  { id: 't3', label: 'vLLM rollout', start: '2026-07-06', end: '2026-07-10', progress: 60 },
  { id: 't4', label: 'Bench suite', start: '2026-07-08', end: '2026-07-14', progress: 25 },
  { id: 't5', label: 'severus memtest', start: '2026-07-09', end: '2026-07-12', progress: 0, tone: 'danger' },
  { id: 't6', label: 'Report', start: '2026-07-13', end: '2026-07-16', progress: 0 },
];

const FLOW_NODES = [
  { id: 'push', label: 'git push' },
  { id: 'build', label: 'Build image' },
  { id: 'test', label: 'Test suite' },
  { id: 'scan', label: 'Vuln scan' },
  { id: 'zot', label: 'Push to zot', tone: 'accent' },
  { id: 'deploy', label: 'Deploy (dgx)', tone: 'success' },
  { id: 'alert', label: 'Alert Hermes', tone: 'danger' },
];
const FLOW_EDGES = [
  { from: 'push', to: 'build' },
  { from: 'build', to: 'test', state: 'active' },
  { from: 'build', to: 'scan' },
  { from: 'test', to: 'zot', label: 'pass' },
  { from: 'scan', to: 'zot' },
  { from: 'zot', to: 'deploy' },
  { from: 'test', to: 'alert', state: 'broken', label: 'fail' },
  { from: 'alert', to: 'build', label: 'retry' },
];

export default function ChartsDemo() {
  const [stacked, setStacked] = createSignal(false);
  return (
    <>
      <PageHead title="Charts" sub="Zero-dep SVG — pie, line, bar, gantt, sparkline, flowchart. Categorical order is the validated Forge ramp." />

      <Grid style={{ 'margin-bottom': '16px', '--fgrid-min': '280px' }}>
        <Card title="Storage by share (pie)">
          <PieChart data={[
            { label: 'AI-Models', value: 3200 },
            { label: 'Media', value: 1800 },
            { label: 'Backups', value: 900 },
            { label: 'Books', value: 220 },
            { label: 'Other', value: 140 },
          ]} />
        </Card>
        <Card title="GPU hours by node (donut)">
          <PieChart donut data={[
            { label: 'dgx', value: 410 },
            { label: 'helios', value: 260 },
            { label: 'severus', value: 120 },
          ]} />
        </Card>
      </Grid>

      <Card title="Requests per hour (line, 3 series, area fills)">
        <LineChart series={[{ ...REQUESTS[0], }, REQUESTS[1], REQUESTS[2]]} area height={220}
                   xLabels={['00', '01', '02', '03', '04', '05', '06', '07', '08', '09']} />
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Deploys per day (bar)"
            action={<Toggle checked={stacked()} onChange={setStacked}>Stacked</Toggle>}>
        <BarChart series={DEPLOY_SERIES} stacked={stacked()} height={200} />
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Fleet migration plan (gantt — dashed line = today)">
        <GanttChart tasks={TASKS} />
      </Card>
      <div style={{ height: '16px' }} />

      <Grid style={{ 'margin-bottom': '16px' }}>
        <Card>
          <Stat label="Requests / h" value="16 k" delta="+9 %" tone="success" />
          <div style={{ 'margin-top': '8px' }}>
            <Sparkline points={[4, 6, 5, 9, 11, 10, 14, 13, 17, 16]} />
          </div>
        </Card>
        <Card>
          <Stat label="P95 latency" value="142 ms" delta="+12 ms" tone="danger" />
          <div style={{ 'margin-top': '8px' }}>
            <Sparkline points={[90, 95, 110, 100, 120, 115, 138, 130, 142, 142]} tone="danger" />
          </div>
        </Card>
      </Grid>

      <Card title="CI pipeline (flowchart — auto-layout, ants on active, flash on broken, retry back-edge)">
        <Flowchart nodes={FLOW_NODES} edges={FLOW_EDGES} />
      </Card>
    </>
  );
}
