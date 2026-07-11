import { createSignal } from 'solid-js';
import {
  PageHead, Card, Button, Modal, Checkbox, Toggle, RadioGroup, Select, ListBox,
  Progress, Spinner,
} from '@forge/ui.jsx';

const MODELS = [
  { value: 'ornith', label: 'Ornith-1.0-35B-FP8' },
  { value: 'ds4', label: 'DeepSeek V4 Flash q2' },
  { value: 'glm', label: 'GLM 5.2 iq2xxs', disabled: true },
  { value: 'qwen', label: 'Qwen3 9B' },
];

export default function FormsDemo() {
  const [checks, setChecks] = createSignal({ a: true, b: false });
  const [gpu, setGpu] = createSignal(true);
  const [profile, setProfile] = createSignal('balanced');
  const [model, setModel] = createSignal(null);
  const [modalModel, setModalModel] = createSignal('ornith');
  const [node, setNode] = createSignal('dgx');
  const [suites, setSuites] = createSignal(['smoke', 'coding']);
  const [modalOpen, setModalOpen] = createSignal(false);
  const [busy, setBusy] = createSignal(false);

  return (
    <>
      <PageHead title="Forms" sub="Checkbox, Toggle, Radio, Select, ListBox, Progress, Spinner" />

      <Card title="Checkbox & toggle">
        <div style={{ display: 'flex', gap: '24px', 'flex-wrap': 'wrap', 'align-items': 'center' }}>
          <Checkbox checked={checks().a} onChange={(v) => setChecks((c) => ({ ...c, a: v }))}>Enable telemetry</Checkbox>
          <Checkbox checked={checks().b} onChange={(v) => setChecks((c) => ({ ...c, b: v }))}>Auto-restart</Checkbox>
          <Checkbox indeterminate>Some selected</Checkbox>
          <Checkbox disabled>Locked option</Checkbox>
          <Toggle checked={gpu()} onChange={setGpu}>GPU offload</Toggle>
          <Toggle disabled>Managed by fleet</Toggle>
        </div>
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Radio group">
        <div style={{ display: 'grid', gap: '20px', 'grid-template-columns': 'repeat(auto-fit, minmax(220px, 1fr))' }}>
          <RadioGroup label="Sampling profile" value={profile()} onChange={setProfile}
                      options={[
                        { value: 'greedy', label: 'Greedy' },
                        { value: 'balanced', label: 'Balanced' },
                        { value: 'creative', label: 'Creative' },
                      ]} />
          <RadioGroup label="Precision (row)" row value="fp8" onChange={() => {}}
                      options={[
                        { value: 'fp8', label: 'FP8' },
                        { value: 'int4', label: 'INT4' },
                        { value: 'bf16', label: 'BF16', disabled: true },
                      ]} />
        </div>
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Select" action={<Button size="sm" onClick={() => setModalOpen(true)}>Open in modal</Button>}>
        <div style={{ display: 'grid', gap: '16px', 'grid-template-columns': 'repeat(auto-fit, minmax(220px, 1fr))' }}>
          <Select label="Model" placeholder="Pick a model…" options={MODELS}
                  value={model()} onChange={setModel}
                  help="GLM is Metal-only — disabled on the Spark." />
          <Select label="Node" options={[
                    { value: 'dgx', label: 'dgx (GB10)' },
                    { value: 'severus', label: 'severus (RX 6800 XT)' },
                    { value: 'helios', label: 'helios (2× RTX 5090)' },
                  ]}
                  value={node()} onChange={setNode} />
        </div>
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="List boxes">
        <div style={{ display: 'grid', gap: '16px', 'grid-template-columns': 'repeat(auto-fit, minmax(220px, 1fr))' }}>
          <ListBox label="Default model (single)" options={MODELS} value={model() ?? 'ornith'} onChange={setModel} />
          <ListBox label="Bench suites (multi)" multiple values={suites()} onChange={setSuites}
                   options={[
                     { value: 'smoke', label: 'smoke' },
                     { value: 'quick', label: 'quick' },
                     { value: 'coding', label: 'coding' },
                     { value: 'agentic', label: 'agentic' },
                     { value: 'full', label: 'full' },
                   ]} />
        </div>
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Progress & spinner">
        <div style={{ display: 'grid', gap: '16px', 'max-width': '480px' }}>
          <Progress label="Model download" value={72} showValue />
          <Progress label="KV cache" value={91} tone="warning" showValue />
          <Progress label="Disk" value={98} tone="danger" showValue />
          <Progress label="Health checks" value={100} tone="success" showValue />
          <Progress label="Reindexing" indeterminate />
        </div>
        <div style={{ display: 'flex', gap: '16px', 'align-items': 'center', 'margin-top': '16px' }}>
          <Spinner />
          <Spinner size={24} />
          <span style={{ color: 'var(--fg-2)', 'font-size': '13px' }}><Spinner size={14} /> Loading runs…</span>
          <Button disabled={busy()} onClick={() => { setBusy(true); setTimeout(() => setBusy(false), 2500); }}>
            {busy() ? <Spinner size={14} /> : null} {busy() ? 'Deploying…' : 'Deploy'}
          </Button>
        </div>
      </Card>

      <Modal open={modalOpen()} onClose={() => setModalOpen(false)} title="Select inside a modal"
             footer={<Button onClick={() => setModalOpen(false)}>Done</Button>}>
        <div style={{ 'min-height': '160px' }}>
          <Select label="Model" options={MODELS} value={modalModel()} onChange={setModalModel} />
        </div>
      </Modal>
    </>
  );
}
