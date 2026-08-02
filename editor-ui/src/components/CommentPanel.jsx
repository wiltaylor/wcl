/* The preview pane's comment surface: a menu button (with a per-page count
   badge) that expands to page-comment / block-pick / list actions, the
   compose modal, the comments list modal, and the review-handshake banner.
   Element picking and pin placement happen inside the same-origin iframe
   via preview/frame.js; all chrome here is Forge components. */

import { For, Show, createSignal } from 'solid-js';
import { Badge, Button, DropdownMenu, Modal, Textarea, toast } from '@forge/ui';

import { selected } from '../state/sites';
import { mainPreview } from '../state/preview';
import {
  compose,
  editComment,
  focusId,
  listOpen,
  commentPage,
  pageComments,
  picking,
  resolveComment,
  reviewWaiting,
  sendToAgent,
  setCompose,
  setFocusId,
  setListOpen,
  setPicking,
  submitComment,
} from '../state/comments';
import { pageInfo } from '../preview/anchors';
import { beginPick, descOf, elByLoc, jumpTo, locOf, selectionQuote } from '../preview/frame';

/** The expandable trigger + hint, rendered in the preview header strip. */
export function CommentMenu(props) {
  const doc = () => props.iframe()?.contentDocument;
  let cancelPick = null;

  const startPick = () => {
    const d = doc();
    if (!d || picking()) return;
    setPicking(true);
    cancelPick = beginPick(d, {
      onPick(el) {
        setPicking(false);
        cancelPick = null;
        const p = pageInfo(d);
        if (!p) return;
        setCompose({
          loc: locOf(p.el, el),
          target: descOf(el),
          quote: selectionQuote(props.iframe()?.contentWindow),
        });
      },
      onCancel() {
        setPicking(false);
        cancelPick = null;
      },
    });
  };

  // In-iframe navigation mid-pick drops the old document (and its
  // listeners) on the floor; PreviewPane resets the picking signal, and
  // calling a stale cancel later is a harmless no-op on a dead document.
  const stopPick = () => {
    cancelPick?.();
    cancelPick = null;
    setPicking(false);
  };

  const count = () => pageComments().length;

  return (
    <div class="ed-comment-menu">
      <Show
        when={!picking()}
        fallback={
          <>
            <span class="ed-pick-hint">Click a block to comment — Esc cancels</span>
            <Button size="sm" onClick={stopPick}>
              Cancel
            </Button>
          </>
        }
      >
        <DropdownMenu
          size="sm"
          align="end"
          label={
            <>
              Comments
              <Show when={count() > 0}>
                {' '}
                <Badge tone="warning">{count()}</Badge>
              </Show>
            </>
          }
          items={[
            {
              label: 'Comment on this page',
              disabled: !commentPage(),
              onSelect: () => setCompose({ page: true }),
            },
            {
              label: 'Comment on a block',
              disabled: !commentPage(),
              onSelect: startPick,
            },
            { separator: true },
            {
              label: `Show comments (${count()})`,
              disabled: !commentPage(),
              onSelect: () => setListOpen(true),
            },
          ]}
        />
      </Show>
    </div>
  );
}

/** Compose + list modals and the review banner — pane-level overlays. */
export function CommentOverlays(props) {
  const doc = () => props.iframe()?.contentDocument;
  const [draft, setDraft] = createSignal('');
  const [editingId, setEditingId] = createSignal(null);
  const [editDraft, setEditDraft] = createSignal('');
  const [sending, setSending] = createSignal(false);

  const submit = async () => {
    const text = draft().trim();
    if (!text) return;
    const res = await submitComment(text);
    if (res.ok) {
      setDraft('');
      toast('Comment added', { tone: 'success', duration: 1500 });
    } else {
      toast(res.error ?? 'could not add the comment', { tone: 'danger', duration: 6000 });
    }
  };

  const saveEdit = async (id) => {
    const text = editDraft().trim();
    if (!text) return;
    const res = await editComment(id, text);
    if (res.ok) setEditingId(null);
    else toast(res.error ?? 'could not edit the comment', { tone: 'danger', duration: 6000 });
  };

  const resolve = async (id) => {
    const res = await resolveComment(id);
    if (!res.ok) toast(res.error ?? 'could not resolve', { tone: 'danger', duration: 6000 });
  };

  const jump = (c) => {
    setListOpen(false);
    if (!jumpTo(doc(), c)) toast('This comment no longer matches a block', { tone: 'warning' });
  };

  const canJump = (c) => {
    const d = doc();
    const p = d && pageInfo(d);
    return !!(c.loc && p && elByLoc(p.el, c.loc));
  };

  const send = async () => {
    setSending(true);
    const res = await sendToAgent();
    setSending(false);
    if (res.ok) toast('Sent to the agent', { tone: 'success', duration: 2000 });
    else toast(res.error ?? 'could not signal the agent', { tone: 'danger', duration: 6000 });
  };

  return (
    <>
      <Show when={reviewWaiting()}>
        <div class="ed-review-banner">
          <span>
            An AI agent is ready for your review — rebuild to see its latest changes, leave
            comments, then send.
          </span>
          <Button size="sm" onClick={() => mainPreview.build()} disabled={!selected() || mainPreview.building()}>
            Rebuild
          </Button>
          <Button size="sm" variant="primary" onClick={send} disabled={sending()}>
            Send to agent
          </Button>
        </div>
      </Show>

      <Modal
        open={compose() !== null}
        onClose={() => setCompose(null)}
        title={compose()?.page ? 'Comment on this page' : 'Comment on a block'}
        footer={
          <>
            <Button onClick={() => setCompose(null)}>Cancel</Button>
            <Button variant="primary" onClick={submit} disabled={!draft().trim()}>
              Comment
            </Button>
          </>
        }
      >
        <Show when={compose() && !compose().page}>
          <p class="ed-comment-target">{compose().target}</p>
        </Show>
        <Show when={compose()?.quote}>
          <blockquote class="ed-comment-quote">{compose().quote}</blockquote>
        </Show>
        <Textarea
          rows={4}
          placeholder="Leave a comment…"
          value={draft()}
          onInput={(e) => setDraft(e.currentTarget.value)}
        />
      </Modal>

      <Modal
        open={listOpen()}
        onClose={() => {
          setListOpen(false);
          setFocusId(null);
          setEditingId(null);
        }}
        title={`Comments on ${commentPage()?.name ?? 'this page'}`}
      >
        <Show when={pageComments().length > 0} fallback={<p>No comments on this page.</p>}>
          <div class="ed-comment-list">
            <For each={pageComments()}>
              {(c) => (
                <div class="ed-comment-item" classList={{ 'is-focus': focusId() === c.id }}>
                  <div class="meta">
                    <span>{c.loc ? (c.target ?? 'block') : 'Whole page'}</span>
                    <Show when={c.author}>
                      <span>· {c.author}</span>
                    </Show>
                  </div>
                  <Show when={c.quote}>
                    <blockquote class="ed-comment-quote">{c.quote}</blockquote>
                  </Show>
                  <Show
                    when={editingId() === c.id}
                    fallback={<p class="body">{c.body}</p>}
                  >
                    <Textarea
                      rows={3}
                      value={editDraft()}
                      onInput={(e) => setEditDraft(e.currentTarget.value)}
                    />
                  </Show>
                  <div class="actions">
                    <Show
                      when={editingId() === c.id}
                      fallback={
                        <>
                          <Show when={canJump(c)}>
                            <Button size="sm" onClick={() => jump(c)}>
                              Jump to
                            </Button>
                          </Show>
                          <Button
                            size="sm"
                            onClick={() => {
                              setEditingId(c.id);
                              setEditDraft(c.body);
                            }}
                          >
                            Edit
                          </Button>
                          <Button size="sm" variant="danger" onClick={() => resolve(c.id)}>
                            Resolve
                          </Button>
                        </>
                      }
                    >
                      <Button size="sm" onClick={() => setEditingId(null)}>
                        Cancel
                      </Button>
                      <Button
                        size="sm"
                        variant="primary"
                        onClick={() => saveEdit(c.id)}
                        disabled={!editDraft().trim()}
                      >
                        Save
                      </Button>
                    </Show>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Modal>
    </>
  );
}
