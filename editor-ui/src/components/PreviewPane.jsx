/* The right-hand preview: the last manual build of the selected site in an
   iframe (navigable — the whole site is rendered), or an <img> when the
   active file is an image (the only viewer for binaries). Builds run from
   the topbar's Rebuild button; this pane also hosts the review-comment UI —
   pins and element picking live inside the same-origin iframe, all chrome
   is Forge components (see CommentPanel). */

import { Show, createEffect, onCleanup, onMount } from 'solid-js';
import { Progress } from '@forge/ui';

import { api } from '../api';
import { active } from '../state/buffers';
import { buildError, buildSeq, building, previewHref, selected } from '../state/sites';
import {
  allComments,
  commentPage,
  loadComments,
  pageComments,
  setCommentPage,
  setFocusId,
  setListOpen,
  setPicking,
  startReviewPoll,
} from '../state/comments';
import { CommentMenu, CommentOverlays } from './CommentPanel';
import { injectCss, pageInfo, placePins } from '../preview/frame';

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg|bmp)$/i;

export default function PreviewPane() {
  let iframe;
  let lastSeq = 0;
  let lastHref = null;

  const onPin = (c) => {
    setFocusId(c.id);
    setListOpen(true);
  };

  const refreshFrame = () => {
    const doc = iframe?.contentDocument;
    if (!doc) return;
    injectCss(doc);
    setCommentPage(pageInfo(doc));
    placePins(doc, pageComments(), onPin);
  };

  // Fires on every in-iframe navigation and on rebuild reloads: the old
  // document (with any active pick-mode listeners) is gone, so reset pick
  // state and re-anchor the comment UI to the new page.
  const onFrameLoad = () => {
    setPicking(false);
    refreshFrame();
  };

  // A finished rebuild with an unchanged href reloads the iframe in place
  // so scroll position survives; a new href swaps the src.
  createEffect(() => {
    const seq = buildSeq();
    const href = previewHref();
    if (seq === lastSeq) return;
    lastSeq = seq;
    if (href === lastHref) {
      iframe?.contentWindow?.location.reload();
    }
    lastHref = href;
  });

  // Re-place pins when the comment set changes (add/edit/resolve) without
  // waiting for a reload.
  createEffect(() => {
    void allComments();
    void commentPage();
    const doc = iframe?.contentDocument;
    if (doc && pageInfo(doc)) placePins(doc, pageComments(), onPin);
  });

  onMount(() => {
    loadComments();
    const stopPoll = startReviewPoll();
    onCleanup(stopPoll);
  });

  const imagePath = () => {
    const path = active();
    return path && IMAGE_EXT.test(path) ? path : null;
  };

  return (
    <div class="ed-preview">
      <Show
        when={!imagePath()}
        fallback={
          <div class="ed-preview-img">
            <img src={api.rawUrl(imagePath())} alt={imagePath()} />
          </div>
        }
      >
        <div class="ed-preview-note">
          <span>{selected() ? selected().label : 'no site selected'}</span>
          <Show when={building()}>
            <span>building…</span>
          </Show>
          <Show when={buildError()}>
            <span class="err">{buildError()}</span>
          </Show>
          <span class="spacer" />
          <CommentMenu iframe={() => iframe} />
        </div>
        <Show when={building()}>
          <Progress indeterminate />
        </Show>
        <CommentOverlays iframe={() => iframe} />
        <Show
          when={previewHref()}
          fallback={
            <div class="ed-empty">
              {selected()
                ? `Press Rebuild to render “${selected().label}”`
                : 'No wdoc sites found in this directory'}
            </div>
          }
        >
          <iframe ref={iframe} src={previewHref()} title="wdoc preview" onLoad={onFrameLoad} />
        </Show>
      </Show>
    </div>
  );
}
