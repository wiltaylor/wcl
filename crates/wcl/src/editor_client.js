// WclEditor — the shared in-browser source-editor component for
// `wcl wdoc serve --edit` (served at /__wdoc_editor.js, loaded before the
// edit client). An enhanced <textarea>: a syntect-highlighted backdrop
// (<pre> under a transparent-text textarea, so native caret/undo/IME all
// keep working), a line-number gutter that doubles as the error margin, a
// debounced dry-run check (syntax errors position exactly; schema errors
// list below), and a `format()` action wired to the `wcl fmt` core.
//
//   const ed = WclEditor.create(container, { value, path, pageFile, onChange });
//   ed.getValue(); ed.setValue(text); await ed.format(); ed.focus();
//
// Highlighting and diagnostics come from the dev server:
//   POST /__wdoc_highlight {text,lang} → {html}
//   POST /__wdoc_check {text,path,page_file} → {ok,diagnostics}
//   POST /__wdoc_format {text} → {text}
(() => {
  'use strict';
  if (window.WclEditor) return;

  const CSS = `
.wcl-src{display:flex;border:1px solid #333;border-radius:6px;background:#111;
  font:12.5px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;min-height:0;flex:1}
.wcl-src-gutter{flex:none;padding:8px 0;background:#161616;color:#555;text-align:right;
  user-select:none;overflow:hidden;border-right:1px solid #262626;border-radius:6px 0 0 6px}
.wcl-src-gutter div{padding:0 8px 0 12px;height:1.5em}
.wcl-src-gutter div.err{color:#f66;background:rgba(255,80,80,.12)}
.wcl-src-body{position:relative;flex:1;overflow:hidden}
.wcl-src-hl,.wcl-src-ta{margin:0;padding:8px 10px;border:0;white-space:pre;overflow-wrap:normal;
  font:inherit;line-height:inherit;tab-size:2;box-sizing:border-box}
.wcl-src-hl{position:absolute;inset:0;overflow:hidden;pointer-events:none;color:#ddd}
.wcl-src-hl code{font:inherit;display:block;min-height:100%}
.wcl-src-ta{position:absolute;inset:0;width:100%;height:100%;resize:none;background:transparent;
  color:transparent;caret-color:#e8e8e8;outline:none;overflow:auto}
.wcl-src-ta::selection{background:rgba(90,140,255,.35);color:transparent}
.wcl-src-problems{font:12px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;
  max-height:7.5em;overflow:auto}
.wcl-src-problems:empty{display:none}
.wcl-src-problems div{padding:2px 8px;color:#f88;border-left:3px solid #a33;margin-top:4px;
  background:rgba(255,80,80,.07);border-radius:0 4px 4px 0;cursor:default}
.wcl-src-problems div.schema{color:#fc7;border-left-color:#a73;background:rgba(255,170,60,.07)}
.wcl-src-wrap{display:flex;flex-direction:column;gap:0;min-height:0;flex:1}
`;

  function injectCss() {
    if (document.getElementById('wcl-src-css')) return;
    const s = document.createElement('style');
    s.id = 'wcl-src-css';
    s.textContent = CSS;
    document.head.appendChild(s);
  }

  const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

  async function post(url, body) {
    const r = await fetch(url, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
    const j = await r.json().catch(() => ({}));
    if (!r.ok) throw new Error(j.error || r.statusText);
    return j;
  }

  const debounce = (fn, ms) => {
    let t = null;
    return (...a) => {
      clearTimeout(t);
      t = setTimeout(() => fn(...a), ms);
    };
  };

  // Buffers past this size skip server highlighting (plain text stays fast).
  const HIGHLIGHT_LIMIT = 256 * 1024;

  function create(container, opts) {
    injectCss();
    opts = opts || {};
    const wrap = document.createElement('div');
    wrap.className = 'wcl-src-wrap';
    wrap.innerHTML =
      '<div class="wcl-src">' +
      '<div class="wcl-src-gutter"></div>' +
      '<div class="wcl-src-body">' +
      '<pre class="wcl-src-hl" aria-hidden="true"><code></code></pre>' +
      '<textarea class="wcl-src-ta" spellcheck="false" autocapitalize="off" autocomplete="off"></textarea>' +
      '</div></div>' +
      '<div class="wcl-src-problems"></div>';
    container.appendChild(wrap);

    const gutter = wrap.querySelector('.wcl-src-gutter');
    const hl = wrap.querySelector('.wcl-src-hl');
    const code = wrap.querySelector('.wcl-src-hl code');
    const ta = wrap.querySelector('.wcl-src-ta');
    const problems = wrap.querySelector('.wcl-src-problems');
    let errLines = [];

    function renderGutter() {
      const lines = ta.value.split('\n').length;
      let html = '';
      for (let i = 1; i <= lines; i++) {
        html += `<div${errLines.includes(i) ? ' class="err"' : ''}>${i}</div>`;
      }
      gutter.innerHTML = html;
      gutter.firstChild && (gutter.scrollTop = ta.scrollTop);
    }

    function syncScroll() {
      hl.scrollTop = ta.scrollTop;
      hl.scrollLeft = ta.scrollLeft;
      gutter.scrollTop = ta.scrollTop;
    }

    const rehighlight = debounce(async () => {
      const text = ta.value;
      if (text.length > HIGHLIGHT_LIMIT) {
        code.textContent = text + '\n';
        return;
      }
      try {
        const j = await post('/__wdoc_highlight', { text, lang: opts.lang || 'wcl' });
        if (ta.value === text) code.innerHTML = j.html + '\n';
      } catch (_) {
        code.textContent = text + '\n';
      }
    }, 160);

    const recheck = debounce(async () => {
      if (!opts.path) return;
      const text = ta.value;
      try {
        const j = await post('/__wdoc_check', {
          text,
          path: opts.path,
          page_file: opts.pageFile || undefined,
        });
        if (ta.value !== text) return;
        const ds = j.diagnostics || [];
        errLines = ds.filter((d) => d.in_edited_file && d.line).map((d) => d.line);
        problems.innerHTML = ds
          .map(
            (d) =>
              `<div class="${d.scope || ''}">${d.line ? `L${d.line}:${d.col} ` : ''}${esc(d.message)}</div>`
          )
          .join('');
        renderGutter();
      } catch (_) {
        /* dev server unreachable — keep the last state */
      }
    }, 380);

    function onInput() {
      // Immediate plain-text repaint keeps the backdrop in step with the
      // caret; the classed highlight replaces it when the server answers.
      code.textContent = ta.value + '\n';
      renderGutter();
      rehighlight();
      recheck();
      if (opts.onChange) opts.onChange();
    }

    ta.addEventListener('input', onInput);
    ta.addEventListener('scroll', syncScroll);
    ta.addEventListener('keydown', (e) => {
      if (e.key === 'Tab' && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        const { selectionStart: s, selectionEnd: en } = ta;
        ta.setRangeText('  ', s, en, 'end');
        onInput();
      }
    });

    const api = {
      el: wrap,
      textarea: ta,
      getValue: () => ta.value,
      setValue(text) {
        ta.value = text;
        onInput();
      },
      focus: () => ta.focus(),
      // Point the dry-run check at a different file (the source view reuses
      // one editor across files).
      setPath(p) {
        opts.path = p;
      },
      async format() {
        const j = await post('/__wdoc_format', { text: ta.value });
        api.setValue(j.text);
        return j.text;
      },
      check: () => recheck(),
    };

    api.setValue(opts.value || '');
    return api;
  }

  window.WclEditor = { create };
})();
