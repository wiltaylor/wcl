import { PageHead, Logs, LogLine } from '@forge/ui.jsx';

export default function LogsDemo() {
  return (
    <>
      <PageHead title="Logs" sub="Logs + LogLine — mono, dense, on --bg-0" />
      <Logs style={{ height: '180px' }}>
        <LogLine time="21:04:12" level="info">model loaded in 41.2 s</LogLine>
        <LogLine time="21:04:13" level="debug">kv cache block size 16, 2048 blocks</LogLine>
        <LogLine time="21:04:13" level="warn">kv cache at 91 % capacity</LogLine>
        <LogLine time="21:04:15" level="error">request 8f2c timed out after 30 s</LogLine>
        <LogLine time="21:04:16" level="info">retrying request 8f2c (attempt 2/3)</LogLine>
        <LogLine time="21:04:18" level="info">request 8f2c completed in 1.9 s</LogLine>
      </Logs>
    </>
  );
}
