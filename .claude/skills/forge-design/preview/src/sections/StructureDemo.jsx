import { createSignal, Show } from 'solid-js';
import { AlertTriangle } from 'lucide-solid';
import {
  PageHead, Card, Tabs, Accordion, Collapsible, Pagination, Separator, Avatar,
  Alert, Skeleton, Badge, Button, SplitPane, Logs, LogLine,
} from '@forge/ui.jsx';

export default function StructureDemo() {
  const [tab, setTab] = createSignal('overview');
  const [page, setPage] = createSignal(6);

  return (
    <>
      <PageHead title="Navigation & structure" sub="Tabs, Accordion, Pagination, Separator, Avatar, Alert, Skeleton, SplitPane" />

      <Card title="Tabs (bar only — content is your Show/Switch)">
        <Tabs active={tab()} onChange={setTab}
              tabs={[
                { id: 'overview', label: 'Overview' },
                { id: 'logs', label: 'Logs', count: 128 },
                { id: 'alerts', label: 'Alerts', count: 2 },
                { id: 'billing', label: 'Billing', disabled: true },
              ]} />
        <div style={{ padding: '14px 2px', 'font-size': '13px', color: 'var(--fg-1)' }}>
          <Show when={tab() === 'overview'}>Fleet is healthy — 6 nodes, 14 deploys today.</Show>
          <Show when={tab() === 'logs'}>128 log lines in the last hour.</Show>
          <Show when={tab() === 'alerts'}>2 active alerts: disk on nas, KV cache on dgx.</Show>
        </div>
      </Card>
      <div style={{ height: '16px' }} />

      <div style={{ display: 'grid', gap: '16px', 'grid-template-columns': 'repeat(auto-fit, minmax(300px, 1fr))', 'align-items': 'start' }}>
        <Card title="Accordion & collapsible">
          <Accordion defaultOpen="a"
                     items={[
                       { id: 'a', title: 'What runs on the DGX Spark?', content: 'vLLM serving Ornith-1.0-35B-FP8, or the ds4 engine — they swap; only one fits in unified memory.' },
                       { id: 'b', title: 'Where do models live?', content: 'The NAS AI-Models share, NFS-mounted at /mnt/ai-models, mirrored into zot as KitOps ModelKits.' },
                       { id: 'c', title: 'How are benches run?', content: 'llm-bench with seeded sampling and pinned revisions, so runs compare across time.' },
                     ]} />
          <div style={{ height: '12px' }} />
          <Collapsible title="Advanced options" defaultOpen={false}>
            Verbose logging, experimental schedulers, and other footguns live here.
          </Collapsible>
        </Card>

        <Card title="Alerts & avatars">
          <div style={{ display: 'grid', gap: '10px' }}>
            <Alert tone="info" title="Sync scheduled">Model mirror sync runs nightly at 03:00.</Alert>
            <Alert tone="warning" icon={AlertTriangle}>KV cache at 91 % capacity.</Alert>
            <Alert tone="danger" title="Node unreachable">severus has missed 3 health checks.</Alert>
            <Alert tone="success">All 14 deploys completed.</Alert>
          </div>
          <Separator />
          <div style={{ display: 'flex', gap: '12px', 'align-items': 'center' }}>
            <Avatar name="Wil Taylor" size="lg" status="success" />
            <Avatar name="Meta Agent" status="warning" />
            <Avatar name="Hermes" size="sm" />
            <Avatar name="Ci Runner" size="md" status="danger" />
            <span style={{ display: 'inline-flex', 'align-items': 'center' }} class="fsep is-vertical" />
            <Badge tone="neutral">initials, sizes, status dots</Badge>
          </div>
        </Card>
      </div>
      <div style={{ height: '16px' }} />

      <div style={{ display: 'grid', gap: '16px', 'grid-template-columns': 'repeat(auto-fit, minmax(300px, 1fr))', 'align-items': 'start' }}>
        <Card title="Skeleton (loading placeholder)">
          <div style={{ display: 'flex', gap: '12px', 'align-items': 'center', 'margin-bottom': '12px' }}>
            <Skeleton width="32px" height="32px" style={{ 'border-radius': 'var(--r-pill)' }} />
            <div style={{ flex: 1, display: 'grid', gap: '8px' }}>
              <Skeleton width="40%" />
              <Skeleton width="70%" height="10px" />
            </div>
          </div>
          <Skeleton height="80px" />
        </Card>

        <Card title="Pagination">
          <Pagination page={page()} pages={20} onChange={setPage} />
          <div style={{ 'margin-top': '10px', 'font-size': '12px', color: 'var(--fg-2)' }}>
            Page <span style={{ 'font-family': 'var(--font-mono)' }}>{page()}</span> of 20
          </div>
        </Card>
      </div>
      <div style={{ height: '16px' }} />

      <Card title="SplitPane (drag the divider; arrow keys work too)" padded={false}>
        <SplitPane initial={260} min={160} style={{ height: '220px' }}
          first={
            <Logs style={{ height: '100%', border: '0', 'border-radius': '0' }}>
              <LogLine time="21:04:12" level="info">model loaded in 41.2 s</LogLine>
              <LogLine time="21:04:13" level="warn">kv cache at 91 %</LogLine>
              <LogLine time="21:04:15" level="error">request 8f2c timed out</LogLine>
            </Logs>
          }
          second={
            <div style={{ padding: '16px', 'font-size': '13px', color: 'var(--fg-1)' }}>
              <strong style={{ color: 'var(--fg-0)' }}>Detail pane.</strong> Select a log line to inspect it here.
              <div style={{ 'margin-top': '8px' }}><Button size="sm">Copy request id</Button></div>
            </div>
          } />
      </Card>
    </>
  );
}
