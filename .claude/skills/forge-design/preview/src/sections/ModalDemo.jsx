import { createSignal } from 'solid-js';
import { PageHead, Card, Button, Modal } from '@forge/ui.jsx';

export default function ModalDemo() {
  const [open, setOpen] = createSignal(false);
  return (
    <>
      <PageHead title="Modal" sub="Controlled — closes on Escape, backdrop click, or the X" />
      <Card title="Confirm dialog">
        <Button variant="danger" onClick={() => setOpen(true)}>Delete run…</Button>
      </Card>
      <Modal open={open()} onClose={() => setOpen(false)} title="Delete run"
             footer={
               <>
                 <Button onClick={() => setOpen(false)}>Cancel</Button>
                 <Button variant="danger" onClick={() => setOpen(false)}>Delete</Button>
               </>
             }>
        This permanently removes run 7f3a1c and its artifacts. This cannot be undone.
      </Modal>
    </>
  );
}
