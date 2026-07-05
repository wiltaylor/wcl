// Questionnaire client for `wcl wdoc serve --answer`.
//
// A floating pill shows how many questions are pending; clicking it opens a
// drawer that renders each one as a form — radio group for :single_select,
// checkboxes for :multi_select, and an always-present free-text input (the
// options never constrain the answer). Submitting POSTs to /__wdoc_answer,
// which writes the .wcl source through the validating edit pipeline one
// question at a time; the card disappears on success, so an interrupted
// session loses nothing. Skip shows only when the schema declares a skipped
// status.
(() => {
  const CSS = `
.wcl-ans-pill{position:fixed;bottom:18px;left:18px;z-index:99999;background:#4c8bf5;color:#fff;
 border:0;border-radius:20px;padding:9px 16px;font:600 13px system-ui;cursor:pointer;
 box-shadow:0 6px 20px rgba(0,0,0,.4)}
.wcl-ans-drawer{position:fixed;top:0;right:0;bottom:0;z-index:100001;width:min(420px,94vw);
 background:#1c1c1c;color:#eee;border-left:1px solid #444;font:13px system-ui;
 box-shadow:-8px 0 40px rgba(0,0,0,.5);display:flex;flex-direction:column}
.wcl-ans-h{display:flex;justify-content:space-between;align-items:center;padding:12px 14px;
 border-bottom:1px solid #333;font-weight:600}
.wcl-ans-h button{background:#333;color:#eee;border:0;border-radius:6px;padding:4px 10px;cursor:pointer}
.wcl-ans-list{overflow:auto;flex:1}
.wcl-ans-q{padding:12px 14px;border-bottom:1px solid #2a2a2a}
.wcl-ans-q .prompt{font-weight:600;margin-bottom:8px}
.wcl-ans-q label{display:flex;gap:8px;align-items:baseline;margin:4px 0;cursor:pointer}
.wcl-ans-q label .note{opacity:.6;font-size:12px}
.wcl-ans-q textarea{width:100%;box-sizing:border-box;min-height:48px;margin-top:6px;background:#111;
 color:#eee;border:1px solid #444;border-radius:6px;padding:6px;font:13px system-ui;resize:vertical}
.wcl-ans-q .acts{margin-top:8px;display:flex;gap:8px;align-items:center}
.wcl-ans-q button{background:#4c8bf5;color:#fff;border:0;border-radius:6px;padding:5px 12px;
 cursor:pointer;font:13px system-ui}
.wcl-ans-q button.ghost{background:#333}
.wcl-ans-q button:disabled{opacity:.5;cursor:default}
.wcl-ans-err{color:#f88;margin-top:6px;font-size:12px;white-space:pre-wrap}
.wcl-ans-done{padding:24px;text-align:center;opacity:.7}
.wcl-ans-warn{padding:8px 14px;color:#e0a000;font-size:12px;border-bottom:1px solid #333}
`;
  const st = document.createElement('style');
  st.textContent = CSS;
  document.head.appendChild(st);

  const pageEl = document.querySelector('[data-wcl-page-file]');
  const pageFile = pageEl ? pageEl.getAttribute('data-wcl-page-file') : '';
  const qs = pageFile ? '?page_file=' + encodeURIComponent(pageFile) : '';

  function esc(s) {
    return (s || '').replace(/[&<>]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
  }

  let pill = null, drawer = null, data = null;

  async function refresh() {
    try {
      const r = await fetch('/__wdoc_answers' + qs);
      if (!r.ok) return;
      data = await r.json();
    } catch (_) { return; }
    renderPill();
    if (drawer) renderDrawer();
  }

  function renderPill() {
    const n = data && data.questions ? data.questions.length : 0;
    if (!n) { if (pill) { pill.remove(); pill = null; } return; }
    if (!pill) {
      pill = document.createElement('button');
      pill.className = 'wcl-ans-pill';
      pill.onclick = () => { drawer ? closeDrawer() : openDrawer(); };
      document.body.appendChild(pill);
    }
    pill.textContent = `? ${n} question${n === 1 ? '' : 's'}`;
  }

  function openDrawer() {
    drawer = document.createElement('div');
    drawer.className = 'wcl-ans-drawer';
    document.body.appendChild(drawer);
    renderDrawer();
  }
  function closeDrawer() {
    if (drawer) { drawer.remove(); drawer = null; }
  }

  function renderDrawer() {
    const questions = (data && data.questions) || [];
    const warnings = (data && data.warnings) || [];
    drawer.innerHTML = '<div class="wcl-ans-h"><span>Pending questions</span>' +
      '<button type="button">Close</button></div>' +
      warnings.map(w => `<div class="wcl-ans-warn">⚠ ${esc(w)}</div>`).join('') +
      '<div class="wcl-ans-list"></div>';
    drawer.querySelector('.wcl-ans-h button').onclick = closeDrawer;
    const list = drawer.querySelector('.wcl-ans-list');
    if (!questions.length) {
      list.innerHTML = '<div class="wcl-ans-done">All questions answered ✓</div>';
      return;
    }
    for (const q of questions) list.appendChild(card(q));
  }

  function card(q) {
    const el = document.createElement('div');
    el.className = 'wcl-ans-q';
    const multi = q.kind === 'multi_select';
    const type = multi ? 'checkbox' : 'radio';
    const group = 'wcl-ans-' + q.span.replace(':', '-');
    el.innerHTML = `<div class="prompt">${esc(q.prompt)}</div>` +
      q.options.map(o =>
        `<label><input type="${type}" name="${group}" value="${esc(o.id)}">` +
        `<span>${esc(o.label)}${o.note ? ` <span class="note">${esc(o.note)}</span>` : ''}</span></label>`
      ).join('') +
      `<textarea placeholder="${q.options.length ? 'Other / add detail…' : 'Your answer…'}"></textarea>` +
      `<div class="acts"><button type="button" class="go">Answer</button>` +
      (q.skippable ? '<button type="button" class="ghost sk">Skip</button>' : '') +
      '</div>';
    const err = m => {
      let e = el.querySelector('.wcl-ans-err');
      if (!e) { e = document.createElement('div'); e.className = 'wcl-ans-err'; el.appendChild(e); }
      e.textContent = '⚠ ' + m;
    };
    const post = async body => {
      const btns = el.querySelectorAll('button');
      btns.forEach(b => b.disabled = true);
      try {
        const r = await fetch('/__wdoc_answer', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(Object.assign({ file: q.file, span: q.span, page_file: pageFile }, body)),
        });
        if (!r.ok) {
          let m = await r.text();
          try { m = JSON.parse(m).error || m; } catch (_) { }
          err(m);
          btns.forEach(b => b.disabled = false);
          return;
        }
      } catch (e) {
        err(String(e));
        btns.forEach(b => b.disabled = false);
        return;
      }
      refresh();
    };
    el.querySelector('.go').onclick = () => {
      const picks = [...el.querySelectorAll('input:checked')].map(i => i.value);
      const other = el.querySelector('textarea').value.trim();
      if (!picks.length && !other) { err('pick an option or type an answer'); return; }
      post({ action: 'answer', picks, other });
    };
    const sk = el.querySelector('.sk');
    if (sk) sk.onclick = () => post({ action: 'skip' });
    return el;
  }

  refresh();
})();
