import { createStore } from 'solid-js/store';
import { createSignal } from 'solid-js';
import { PageHead, Card, Badge } from '@forge/ui.jsx';
import { NodeGraph } from '@forge/graph.jsx';

let nextId = 1;

export default function GraphDemo() {
  const [graph, setGraph] = createStore({
    nodes: [
      { id: 'trigger', x: 30, y: 40, title: 'Webhook trigger',
        inputs: [], outputs: [{ id: 'event', type: 'trigger', label: 'event' }] },
      { id: 'fetch', x: 280, y: 120, title: 'HTTP request',
        inputs: [{ id: 'run', type: 'trigger', label: 'run' }],
        outputs: [{ id: 'body', type: 'object', label: 'body' }, { id: 'status', type: 'number', label: 'status' }] },
      { id: 'parse', x: 540, y: 60, title: 'JSON parse',
        inputs: [{ id: 'raw', type: 'object', label: 'raw' }],
        outputs: [{ id: 'items', type: 'array', label: 'items' }] },
      { id: 'alert', x: 540, y: 260, title: 'Alert (unreachable)',
        inputs: [{ id: 'code', type: 'number', label: 'code' }],
        outputs: [{ id: 'sent', type: 'boolean', label: 'sent' }] },
    ],
    edges: [
      { id: 'e1', from: { node: 'trigger', port: 'event' }, to: { node: 'fetch', port: 'run' }, state: 'active' },
      { id: 'e2', from: { node: 'fetch', port: 'body' }, to: { node: 'parse', port: 'raw' } },
      { id: 'e3', from: { node: 'fetch', port: 'status' }, to: { node: 'alert', port: 'code' }, state: 'broken' },
    ],
  });
  const [selected, setSelected] = createSignal(null);

  const moveNode = (id, x, y) => {
    const idx = graph.nodes.findIndex((n) => n.id === id);
    if (idx >= 0) setGraph('nodes', idx, { x: Math.max(0, x), y: Math.max(0, y) });
  };
  const connect = ({ from, to }) => {
    const dup = graph.edges.some((e) =>
      e.from.node === from.node && e.from.port === from.port &&
      e.to.node === to.node && e.to.port === to.port);
    if (!dup) setGraph('edges', (e) => [...e, { id: `new-${nextId++}`, from, to }]);
  };
  const remove = (sel) => {
    if (sel?.kind === 'edge') setGraph('edges', (e) => e.filter((x) => x.id !== sel.id));
    if (sel?.kind === 'node') {
      setGraph('edges', (e) => e.filter((x) => x.from.node !== sel.id && x.to.node !== sel.id));
      setGraph('nodes', (n) => n.filter((x) => x.id !== sel.id));
    }
    setSelected(null);
  };

  return (
    <>
      <PageHead title="Node graph" sub="Drag nodes by their title bar; drag from an output port to a compatible input to connect; click an edge and press Delete to remove it" />
      <Card padded={false} title="Pipeline editor"
            action={<>
              <Badge tone="accent">active = marching ants</Badge>{' '}
              <Badge tone="danger">broken = flashing</Badge>
            </>}>
        <NodeGraph
          nodes={graph.nodes}
          edges={graph.edges}
          selected={selected()}
          onNodeMove={moveNode}
          onConnect={connect}
          onSelect={setSelected}
          onDelete={remove}
          style={{ height: '460px' }}
        />
      </Card>
    </>
  );
}
