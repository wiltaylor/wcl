import { createSignal } from 'solid-js';
import { PageHead, Card, Combobox, Slider, Textarea, ToggleGroup, Calendar, DatePicker } from '@forge/ui.jsx';

const MODELS = [
  { value: 'ornith', label: 'Ornith-1.0-35B-FP8' },
  { value: 'ds4', label: 'DeepSeek V4 Flash q2' },
  { value: 'glm', label: 'GLM 5.2 iq2xxs', disabled: true },
  { value: 'qwen', label: 'Qwen3 9B' },
  { value: 'haiku', label: 'Haiku 4.5' },
];

export default function Forms2Demo() {
  const [model, setModel] = createSignal('ornith');
  const [temp, setTemp] = createSignal(70);
  const [topP, setTopP] = createSignal(95);
  const [precision, setPrecision] = createSignal('fp8');
  const [calDate, setCalDate] = createSignal(null);
  const [runDate, setRunDate] = createSignal(null);

  return (
    <>
      <PageHead title="Forms 2" sub="Combobox, Slider, Textarea, ToggleGroup, Calendar, DatePicker" />

      <Card title="Combobox & segmented">
        <div style={{ display: 'grid', gap: '16px', 'grid-template-columns': 'repeat(auto-fit, minmax(240px, 1fr))', 'align-items': 'start' }}>
          <Combobox label="Model (type to filter)" options={MODELS} value={model()} onChange={setModel}
                    placeholder="Search models…" help="GLM is Metal-only — disabled." />
          <div class="ffield">
            <span class="ffield-label">Precision</span>
            <div>
              <ToggleGroup value={precision()} onChange={setPrecision}
                           options={[
                             { value: 'fp8', label: 'FP8' },
                             { value: 'int4', label: 'INT4' },
                             { value: 'bf16', label: 'BF16', disabled: true },
                           ]} />
            </div>
          </div>
        </div>
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Sliders & textarea">
        <div style={{ display: 'grid', gap: '16px', 'grid-template-columns': 'repeat(auto-fit, minmax(240px, 1fr))' }}>
          <div style={{ display: 'grid', gap: '16px' }}>
            <Slider label="Temperature (×100)" value={temp()} onChange={setTemp} showValue />
            <Slider label="Top-p (×100)" value={topP()} onChange={setTopP} showValue />
            <Slider label="Locked" value={30} disabled />
          </div>
          <Textarea label="System prompt" rows="4" placeholder="You are a terse infrastructure copilot…"
                    help="Plain text; ~2k tokens max." />
        </div>
      </Card>
      <div style={{ height: '16px' }} />

      <Card title="Calendar & date picker">
        <div style={{ display: 'flex', gap: '32px', 'flex-wrap': 'wrap', 'align-items': 'start' }}>
          <Calendar value={calDate()} onChange={setCalDate} />
          <div style={{ 'min-width': '240px' }}>
            <DatePicker label="Run after" value={runDate()} onChange={setRunDate}
                        min="2026-07-01" max="2026-09-30"
                        placeholder="Pick a date…" help="July–September only." />
          </div>
        </div>
      </Card>
    </>
  );
}
