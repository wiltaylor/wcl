/* Review-comment state: all sidecar records (/api/comments), the preview
   iframe's current page key, compose/list UI state, and the review
   handshake (an agent blocked in `wcl wdoc review` waiting for feedback). */

import { createSignal } from 'solid-js';

import { api } from '../api';

const [all, setAll] = createSignal([]);
const [page, setPage] = createSignal(null); // {el, name, file, objects} | null
const [picking, setPicking] = createSignal(false);
/** null | {page: true} | {loc, target, quote} — the open compose modal. */
const [compose, setCompose] = createSignal(null);
const [listOpen, setListOpen] = createSignal(false);
const [focusId, setFocusId] = createSignal(null);
const [reviewWaiting, setReviewWaiting] = createSignal(false);

export {
  all as allComments,
  page as commentPage,
  setPage as setCommentPage,
  picking,
  setPicking,
  compose,
  setCompose,
  listOpen,
  setListOpen,
  focusId,
  setFocusId,
  reviewWaiting,
};

/** Comments on the iframe's current page (name + source-file key). */
export function pageComments() {
  return commentsForPage(all(), page());
}

export function commentsForPage(comments, currentPage) {
  if (!currentPage) return [];
  return comments.filter((comment) => {
    const pageMatch =
      comment.page === currentPage.name &&
      (!comment.page_file || comment.page_file === currentPage.file);
    const objectMatch =
      comment.object_kind &&
      comment.object_id &&
      currentPage.objects?.some(
        (object) => object.kind === comment.object_kind && object.id === comment.object_id,
      );
    return pageMatch || objectMatch;
  });
}

export async function loadComments() {
  const res = await api.comments();
  if (res.ok) setAll(res.comments ?? []);
  return res;
}

/** Submit the open compose form. Returns the API result. */
export async function submitComment(text) {
  const p = page();
  const c = compose();
  if (!p || !c) return { ok: false, error: 'no comment target' };
  const payload = { page: p.name, page_file: p.file, body: text };
  if (!c.page) {
    payload.loc = c.loc;
    payload.target = c.target;
    if (c.quote) payload.quote = c.quote;
  }
  const res = await api.addComment(payload);
  if (res.ok) {
    setCompose(null);
    await loadComments();
  }
  return res;
}

export async function resolveComment(id) {
  const res = await api.resolveComment(id);
  if (res.ok) await loadComments();
  return res;
}

export async function editComment(id, body) {
  const res = await api.editComment(id, body);
  if (res.ok) await loadComments();
  return res;
}

/* Review-handshake long-poll: chain rounds so the server answers as soon
   as a `wcl wdoc review` wait begins or ends; a fresh round after "Send
   to agent" re-raises the banner (the agent finished another pass). */
let pollAbort = null;

export function startReviewPoll() {
  if (pollAbort) return () => {}; // singleton — PreviewPane remounts share it
  const ctl = new AbortController();
  pollAbort = ctl;
  (async () => {
    // The round is an opaque decimal string — a u64 too large for a JS
    // number (parsing it would round and the long-poll would spin).
    let round = '0';
    while (!ctl.signal.aborted) {
      const res = await api.reviewStatus(round, ctl.signal);
      if (ctl.signal.aborted) break;
      if (res.ok) {
        setReviewWaiting(!!res.waiting);
        round = String(res.round ?? '0');
      } else {
        await new Promise((r) => setTimeout(r, 2000));
      }
    }
  })();
  return () => {
    ctl.abort();
    pollAbort = null;
  };
}

export async function sendToAgent() {
  const res = await api.reviewReady();
  if (res.ok) setReviewWaiting(false);
  return res;
}
