import { PageHead, SettingsLayout, SettingsSection, SettingsRow, Input, Button } from '@forge/ui.jsx';

export default function SettingsDemo() {
  return (
    <>
      <PageHead title="Settings" sub="SettingsLayout / SettingsSection / SettingsRow" />
      <SettingsLayout
        nav={
          <>
            <a class="is-active" href="#settings">General</a>
            <a href="#settings">Endpoints</a>
            <a href="#settings">Tokens</a>
          </>
        }
      >
        <SettingsSection title="General" sub="Node identity and scheduling.">
          <SettingsRow>
            <Input label="Display name" value="DGX Spark" />
            <Input label="VLAN" value="server" />
          </SettingsRow>
          <SettingsRow>
            <Input label="Model store" value="/mnt/ai-models" help="NFS mount from the NAS." />
            <Input label="Max jobs" value="4" />
          </SettingsRow>
          <Button variant="primary" size="sm">Save changes</Button>
        </SettingsSection>
      </SettingsLayout>
    </>
  );
}
