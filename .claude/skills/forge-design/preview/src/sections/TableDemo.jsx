import { For } from 'solid-js';
import { RefreshCw } from 'lucide-solid';
import { PageHead, Card, Table, Badge, Button } from '@forge/ui.jsx';

const DEPLOYS = [
  { service: 'vllm-server', sha: 'ab12f34', ok: 'success', label: 'deployed', node: 'dgx', latency: '12 ms', ago: '2m ago' },
  { service: 'bench-runner', sha: 'cd56e78', ok: 'danger', label: 'failed', node: 'severus', latency: '—', ago: '18m ago' },
  { service: 'hermes-agent', sha: 'ef90a12', ok: 'warning', label: 'pending', node: 'severus', latency: '44 ms', ago: '1h ago' },
  { service: 'zot-registry', sha: '0a1b2c3', ok: 'success', label: 'deployed', node: 'nas', latency: '8 ms', ago: '3h ago' },
];

export default function TableDemo() {
  return (
    <>
      <PageHead title="Tables" sub="Table wraps .ftable-wrap > .ftable — scrolls horizontally on mobile" />
      <Card title="Recent deploys" padded={false}
            action={<Button variant="ghost" size="sm" icon={RefreshCw}>Refresh</Button>}>
        <Table>
          <thead>
            <tr><th>Service</th><th>Commit</th><th>Status</th><th>Node</th><th>Latency</th><th>When</th></tr>
          </thead>
          <tbody>
            <For each={DEPLOYS}>
              {(d) => (
                <tr>
                  <td>{d.service}</td>
                  <td class="col-mono">{d.sha}</td>
                  <td><Badge tone={d.ok} dot>{d.label}</Badge></td>
                  <td class="col-mono">{d.node}</td>
                  <td class="col-mono">{d.latency}</td>
                  <td class="col-mono">{d.ago}</td>
                </tr>
              )}
            </For>
          </tbody>
        </Table>
      </Card>
    </>
  );
}
