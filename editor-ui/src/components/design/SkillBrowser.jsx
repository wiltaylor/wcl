/* The Skill view's canvas: the actual built skill folder (SKILL.md +
   references/ + scripts/assets — exactly what `wcl wdoc skill` ships),
   browsable. Left = the file tree; right = the selected file's raw content
   (that's what the consuming agent reads) or the image itself. Files are
   served from the preview scratch tree; Rebuild regenerates the folder. */

import { For, Show, createEffect, createSignal } from 'solid-js';
import { FileText, Image as ImageIcon } from 'lucide-solid';
import { Spinner } from '@forge/ui';

import { mainPreview } from '../../state/preview';

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg|bmp|ico)$/i;

export default function SkillBrowser() {
  const [active, setActive] = createSignal(null);
  const [content, setContent] = createSignal(null);
  const [loading, setLoading] = createSignal(false);

  const files = () => mainPreview.files();
  const url = (path) => `${mainPreview.base() ?? ''}${path}`;

  // Default to SKILL.md (or the first file) whenever a build lands.
  createEffect(() => {
    const list = files();
    if (list.length === 0) {
      setActive(null);
      return;
    }
    if (!active() || !list.includes(active())) {
      setActive(list.includes('SKILL.md') ? 'SKILL.md' : list[0]);
    }
  });

  createEffect(() => {
    const path = active();
    // Re-fetch when a rebuild replaced the folder too.
    void mainPreview.reloadSeq();
    setContent(null);
    if (!path || IMAGE_EXT.test(path)) return;
    setLoading(true);
    fetch(url(path))
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(r.statusText))))
      .then((text) => setContent(text))
      .catch((e) => setContent(`(failed to load: ${e})`))
      .finally(() => setLoading(false));
  });

  /** Group the flat listing into a foldered display order. */
  const grouped = () => {
    const out = [];
    let lastDir = null;
    for (const f of [...files()].sort((a, b) => {
      const da = a.includes('/') ? 1 : 0;
      const db = b.includes('/') ? 1 : 0;
      return da - db || a.localeCompare(b);
    })) {
      const dir = f.includes('/') ? f.slice(0, f.lastIndexOf('/')) : null;
      if (dir !== lastDir && dir) out.push({ header: dir });
      lastDir = dir;
      out.push({ path: f, name: f.slice(f.lastIndexOf('/') + 1), nested: !!dir });
    }
    return out;
  };

  return (
    <div class="ed-skill-browser">
      <div class="ed-skill-files">
        <For each={grouped()}>
          {(item) =>
            item.header ? (
              <div class="ed-skill-dir">{item.header}/</div>
            ) : (
              <button
                type="button"
                class="ed-skill-file"
                classList={{ 'is-active': active() === item.path, 'is-nested': item.nested }}
                onClick={() => setActive(item.path)}
              >
                {IMAGE_EXT.test(item.path) ? <ImageIcon size={13} /> : <FileText size={13} />}
                {item.name}
              </button>
            )
          }
        </For>
        <Show when={files().length === 0}>
          <div class="ed-data-empty">Rebuild to generate the skill folder.</div>
        </Show>
      </div>
      <div class="ed-skill-content">
        <Show when={active()} fallback={<div class="ed-empty">Pick a file</div>}>
          <div class="ed-skill-path">{active()}</div>
          <Show
            when={!IMAGE_EXT.test(active())}
            fallback={
              <div class="ed-skill-img">
                <img src={url(active())} alt={active()} />
              </div>
            }
          >
            <Show when={!loading()} fallback={<Spinner size={16} label="Loading file" />}>
              <pre class="ed-skill-text">{content() ?? ''}</pre>
            </Show>
          </Show>
        </Show>
      </div>
    </div>
  );
}
