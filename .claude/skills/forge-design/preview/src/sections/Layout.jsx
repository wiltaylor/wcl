import { Bell, Plus } from 'lucide-solid';
import { PageHead, Card, Button, Crumbs, Empty, Grid, Stat, Badge } from '@forge/ui.jsx';

export default function Layout() {
  return (
    <>
      <PageHead title="Page & layout" sub="PageHead, Crumbs, Grid, Empty"
                actions={<><Button>Refresh</Button><Button variant="primary" icon={Plus}>New node</Button></>} />

      <Card title="Crumbs" action={<Badge tone="neutral">.ftopbar-crumbs</Badge>}>
        <Crumbs items={['fleet', 'nodes', 'dgx', 'gpu-0']} />
      </Card>

      <div style={{ height: '16px' }} />

      <Card title="Grid" action={<Badge tone="neutral">.fgrid</Badge>}>
        <Grid>
          <Card><Stat label="Nodes" value="6" /></Card>
          <Card><Stat label="Alerts" value="2" delta="+1" tone="danger" /></Card>
          <Card><Stat label="Deploys today" value="14" delta="+6" tone="success" /></Card>
        </Grid>
      </Card>

      <div style={{ height: '16px' }} />

      <Empty title="No alerts"
             action={<Button size="sm" icon={Bell}>Configure alerts</Button>}>
        Everything is healthy.
      </Empty>
    </>
  );
}
