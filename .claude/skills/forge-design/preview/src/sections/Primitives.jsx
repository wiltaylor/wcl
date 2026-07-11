import { RefreshCw, Rocket, Search, Trash2 } from 'lucide-solid';
import {
  PageHead, Card, Button, Input, Badge, Toast, Kbd, StatusDot, Eyebrow, Grid, Stat,
} from '@forge/ui.jsx';

export default function Primitives() {
  return (
    <>
      <PageHead title="Primitives" sub="Button, Input, Badge, Toast, Kbd, StatusDot, Stat"
                actions={<Button variant="primary" icon={Rocket}>Deploy</Button>} />

      <Grid style={{ 'margin-bottom': '16px' }}>
        <Card><Stat label="Uptime" value="99.98 %" delta="+0.01 %" tone="success" /></Card>
        <Card><Stat label="Requests" value="1.2 M" delta="flat" tone="neutral" /></Card>
        <Card><Stat label="P95 latency" value="142 ms" delta="+12 ms" tone="danger" /></Card>
        <Card><Stat label="GPU util" value="87 %" delta="-3 %" tone="success" /></Card>
      </Grid>

      <Card title="Buttons" action={<Badge tone="neutral">.fbtn</Badge>}>
        <div style={{ display: 'flex', gap: '8px', 'flex-wrap': 'wrap', 'align-items': 'center' }}>
          <Button variant="primary">Primary</Button>
          <Button>Secondary</Button>
          <Button variant="ghost">Ghost</Button>
          <Button variant="danger" icon={Trash2}>Delete</Button>
          <Button size="sm" icon={RefreshCw}>Small</Button>
          <Button size="lg">Large</Button>
          <Button disabled>Disabled</Button>
        </div>
      </Card>

      <div style={{ height: '16px' }} />

      <Card title="Inputs">
        <div style={{ display: 'grid', gap: '16px', 'grid-template-columns': 'repeat(auto-fit, minmax(220px, 1fr))' }}>
          <Input label="Node name" placeholder="gx10-eae0" help="Lowercase, hyphens allowed." />
          <Input label="Search" icon={Search} placeholder="Filter services…" />
          <Input label="API token" error help="Token is expired — rotate it in settings." value="frg_9f2…" />
        </div>
      </Card>

      <div style={{ height: '16px' }} />

      <Card title="Badges, toasts & friends">
        <div style={{ display: 'flex', gap: '10px', 'flex-wrap': 'wrap', 'align-items': 'center' }}>
          <Badge tone="success" dot>deployed</Badge>
          <Badge tone="warning" dot>pending</Badge>
          <Badge tone="danger" dot>failed</Badge>
          <Badge tone="info">info</Badge>
          <Badge tone="accent">accent</Badge>
          <Badge>neutral</Badge>
          <span><StatusDot tone="success" /> healthy</span>
          <span><StatusDot tone="danger" /> down</span>
          <Kbd>⌘K</Kbd>
          <Eyebrow>Eyebrow label</Eyebrow>
        </div>
        <div style={{ display: 'flex', gap: '10px', 'flex-wrap': 'wrap', 'margin-top': '14px' }}>
          <Toast tone="success">Deploy finished in 41 s</Toast>
          <Toast tone="warning">KV cache at 91 % capacity</Toast>
          <Toast tone="danger">Node severus unreachable</Toast>
          <Toast tone="info">3 updates available</Toast>
        </div>
      </Card>
    </>
  );
}
