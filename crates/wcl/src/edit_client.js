/* wcl wdoc serve --edit — WYSIWYG editor client.
 *
 * Two surfaces, one floating toolbar (bottom-left):
 *   • Page editor — select a rendered block to edit its fields in a side panel,
 *     double-click a text block to edit text inline, and add / move / delete
 *     blocks. Locates blocks by the `data-wcl-span` / `data-wcl-file` anchors
 *     the build stamps in edit mode.
 *   • Object editor — browse / add / edit / delete schema-defined data objects.
 *
 * Every save POSTs to the /__wdoc_edit* / /__wdoc_object endpoints, which write
 * real `.wcl` source; the watcher then rebuilds and the reload script reloads
 * the page with fresh anchors. Because a save triggers a reload, the open view
 * is stashed in sessionStorage and restored on load, so editing feels
 * continuous across the reload.
 */
(() => {
  const CSS = `
body.wcl-ed-picking{cursor:crosshair}
body.wcl-ed-picking [data-wcl-block].wcl-ed-hot{outline:2px solid #16a34a;outline-offset:2px;
 background:rgba(22,163,74,.08)}
[data-wcl-block].wcl-ed-sel{outline:2px solid #16a34a!important;outline-offset:2px!important}
.wcl-ed-bar{position:fixed;bottom:18px;left:18px;z-index:99999;display:flex;flex-direction:column;
 gap:8px;align-items:flex-start}
.wcl-ed-actions{display:flex;flex-direction:column;gap:8px;align-items:flex-start;
 opacity:0;transform:translateY(8px);pointer-events:none;transition:opacity .15s,transform .15s}
.wcl-ed-bar.wcl-ed-open .wcl-ed-actions{opacity:1;transform:none;pointer-events:auto}
.wcl-ed-bar button{background:#16a34a;color:#fff;border:0;border-radius:20px;padding:9px 16px;
 font:600 13px system-ui;cursor:pointer;box-shadow:0 6px 20px rgba(0,0,0,.4)}
.wcl-ed-bar button.on{background:#d97706}
.wcl-ed-bar button.wcl-ed-toggle{width:48px;height:48px;padding:0;border-radius:50%;font-size:20px}
.wcl-ed-hint{position:fixed;top:0;left:0;right:0;z-index:99999;background:#16a34a;color:#fff;
 text-align:center;padding:7px;font:600 13px system-ui}
.wcl-ed-panel{position:fixed;top:0;right:0;bottom:0;width:340px;max-width:90vw;z-index:100000;
 background:#1c1c1c;color:#eee;border-left:1px solid #444;box-shadow:-8px 0 30px rgba(0,0,0,.5);
 font:13px system-ui;display:flex;flex-direction:column}
.wcl-ed-panel h3{margin:0;padding:12px 14px;border-bottom:1px solid #333;font-size:14px;
 display:flex;justify-content:space-between;align-items:center}
.wcl-ed-panel .wcl-ed-body{padding:12px 14px;overflow:auto;flex:1}
.wcl-ed-row{margin-bottom:12px;display:flex;flex-direction:column;gap:4px}
.wcl-ed-row label{font-weight:600;font-size:12px}
.wcl-ed-row .hint{opacity:.6;font-size:11px}
.wcl-ed-row input,.wcl-ed-row textarea,.wcl-ed-row select{background:#111;color:#eee;
 border:1px solid #444;border-radius:6px;padding:6px;font:13px system-ui;width:100%;box-sizing:border-box}
.wcl-ed-row textarea{min-height:64px;resize:vertical}
.wcl-ed-code{width:100%;box-sizing:border-box;min-height:45vh;background:#111;color:#eee;
 border:1px solid #444;border-radius:6px;padding:8px;resize:vertical;
 font:13px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;white-space:pre;tab-size:2}
.wcl-ed-panel .wcl-ed-foot{padding:10px 14px;border-top:1px solid #333;display:flex;gap:8px;flex-wrap:wrap}
.wcl-ed-panel button,.wcl-ed-modal button{background:#16a34a;color:#fff;border:0;border-radius:6px;
 padding:6px 12px;cursor:pointer;font:13px system-ui}
.wcl-ed-panel button.ghost,.wcl-ed-modal button.ghost{background:#333}
.wcl-ed-panel button.danger,.wcl-ed-modal button.danger{background:#b91c1c}
.wcl-ed-x{background:#333!important;border-radius:6px!important;padding:2px 8px!important}
.wcl-ed-err{color:#f88;margin-top:8px;font-size:12px;white-space:pre-wrap;max-height:10em;overflow:auto}
.wcl-ed-ro{opacity:.6;font-style:italic;font-size:12px;margin:6px 0}
.wcl-ed-modal{position:fixed;inset:0;z-index:100001;background:rgba(0,0,0,.5);display:flex;
 align-items:center;justify-content:center}
.wcl-ed-modal-box{background:#1c1c1c;color:#eee;border:1px solid #444;border-radius:10px;
 width:min(560px,92vw);max-height:80vh;overflow:auto;font:13px system-ui}
/* The text-editor view fills most of the viewport so the source isn't squished. */
.wcl-ed-modal-box.wcl-ed-wide{width:min(1200px,96vw);height:94vh;max-height:94vh;
 display:flex;flex-direction:column}
.wcl-ed-wide .wcl-ed-body{flex:1;display:flex;flex-direction:column;overflow:auto}
.wcl-ed-wide .wcl-ed-code{flex:1;min-height:60vh}
.wcl-ed-modal-box h3{margin:0;padding:12px 14px;border-bottom:1px solid #333;position:sticky;top:0;
 background:#1c1c1c;display:flex;justify-content:space-between;align-items:center}
.wcl-ed-modal-box .wcl-ed-body{padding:12px 14px}
.wcl-ed-item{display:flex;justify-content:space-between;align-items:center;gap:8px;
 padding:8px;border:1px solid #2a2a2a;border-radius:6px;margin-bottom:6px}
/* Source view: file tree beside the editor, both filling the wide modal. */
.wcl-ed-srcgrid{display:flex;gap:10px;flex:1;min-height:0}
.wcl-ed-files{flex:none;width:240px;overflow:auto;border:1px solid #2a2a2a;border-radius:6px;
 padding:6px;font:12px ui-monospace,Menlo,Consolas,monospace}
.wcl-ed-files div{padding:3px 6px;border-radius:4px;cursor:pointer;color:#bbb;
 white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.wcl-ed-files div:hover{background:#262626;color:#eee}
.wcl-ed-files div.on{background:#14532d;color:#fff}
.wcl-ed-srcpane{flex:1;display:flex;flex-direction:column;gap:8px;min-width:0;min-height:0}
.wcl-ed-srchdr{display:flex;align-items:center;gap:8px;font-size:12px;color:#aaa}
.wcl-ed-srchdr .dirty{color:#fbbf24}
.wcl-ed-preview{flex:1;min-width:0;border:1px solid #2a2a2a;border-radius:6px;background:#fff}
.wcl-ed-item .acts{display:flex;gap:6px}
.wcl-ed-tag{font-size:11px;opacity:.6}
`;
  const style = document.createElement('style');
  style.textContent = CSS;
  document.head.appendChild(style);

  const pageEl = document.querySelector('[data-wcl-page-file]');
  // The current page's source file — sent with every request so the server can
  // resolve the sub-site (e.g. a wskill) this page belongs to and introspect
  // that document, not just the top-level one `--edit` was pointed at.
  const pageFile = () => (pageEl && pageEl.getAttribute('data-wcl-page-file')) || '';
  const pageName = () => (pageEl && pageEl.getAttribute('data-wcl-page-name')) || '';
  const pfq = () => '&page_file=' + encodeURIComponent(pageFile());
  const esc = s => (s == null ? '' : String(s)).replace(/[&<>"]/g, c =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  // Ignore clicks on our own chrome.
  const chrome = t => t.closest && (t.closest('.wcl-ed-bar') || t.closest('.wcl-ed-panel') ||
    t.closest('.wcl-ed-modal') || t.closest('.wcl-ed-hint'));

  async function getJSON(u) {
    const r = await fetch(u);
    const j = await r.json().catch(() => ({}));
    if (!r.ok) throw new Error(j.error || r.statusText);
    return j;
  }
  async function postJSON(u, p) {
    const r = await fetch(u, {
      method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(p),
    });
    const j = await r.json().catch(() => ({}));
    if (!r.ok) throw new Error(j.error || r.statusText);
    return j;
  }

  // Stash the view to reopen after the post-save reload.
  const SS = 'wcl-ed-restore';
  const stash = v => { try { sessionStorage.setItem(SS, JSON.stringify(v)); } catch (_) {} };
  const popStash = () => {
    try { const v = sessionStorage.getItem(SS); sessionStorage.removeItem(SS); return v && JSON.parse(v); }
    catch (_) { return null; }
  };

  // Run a mutating save: on success the watcher rebuild reloads us (with the
  // view stashed for restore); on failure surface the error and stay put.
  async function save(payload, url, restore, onErr) {
    if (payload && payload.page_file == null) payload.page_file = pageFile();
    if (restore) stash(restore);
    try {
      await postJSON(url, payload);
      // success → wait for the reload the watcher triggers.
    } catch (e) {
      sessionStorage.removeItem(SS);
      if (onErr) onErr(String(e.message || e));
    }
  }

  const schemaCache = {};
  async function schemaFor(kind) {
    if (!schemaCache[kind]) schemaCache[kind] = await getJSON('/__wdoc_schema?kind=' + encodeURIComponent(kind) + pfq());
    return schemaCache[kind];
  }

  // ---- field <-> input mapping -------------------------------------------

  // Build the input element(s) for a field descriptor; `cur` is the current
  // value from /__wdoc_object ({kind:"raw"|"expr", value}) or undefined.
  function makeInput(desc, cur) {
    const w = desc.widget;
    const curRaw = cur && cur.kind === 'raw' ? cur.value : null;
    const curExpr = cur && cur.kind === 'expr' ? cur.value : null;
    let elHtml;
    if (w === 'text') {
      const v = curRaw != null ? curRaw : (curExpr || '');
      elHtml = `<textarea data-w="text">${esc(v)}</textarea>`;
    } else if (w === 'bool') {
      const on = curExpr === 'true';
      elHtml = `<input type="checkbox" data-w="bool" ${on ? 'checked' : ''}>`;
    } else if (w === 'number') {
      elHtml = `<input type="text" inputmode="decimal" data-w="number" value="${esc(curExpr || '')}">`;
    } else if (w === 'symbol') {
      const v = (curExpr || '').replace(/^:/, '');
      elHtml = `<input type="text" data-w="symbol" data-ty="${esc(desc.type)}" value="${esc(v)}">`;
    } else if (w === 'enum') {
      const cv = (curExpr || '').replace(/^:/, '');
      const opts = (desc.variants || []).map(v =>
        `<option ${v === cv ? 'selected' : ''}>${esc(v)}</option>`).join('');
      elHtml = `<select data-w="enum"><option value=""></option>${opts}</select>`;
    } else if (w === 'ref') {
      elHtml = `<select data-w="ref" data-refkind="${esc(desc.ref_kind)}" data-cur="${esc(curExpr || '')}"></select>`;
    } else {
      // union / child / children / expr fallback → raw expression input
      elHtml = `<input type="text" data-w="expr" value="${esc(curExpr || curRaw || '')}">`;
    }
    return elHtml;
  }

  // Read a field's input back into a {raw} or {expr} payload fragment, or null
  // to skip (empty optional).
  function readInput(row) {
    const el = row.querySelector('[data-w]');
    if (!el) return null;
    const w = el.getAttribute('data-w');
    if (w === 'text') {
      return { raw: el.value };
    } else if (w === 'bool') {
      return { expr: el.checked ? 'true' : 'false' };
    } else if (w === 'number') {
      if (el.value.trim() === '') return null;
      return { expr: el.value.trim() };
    } else if (w === 'symbol') {
      if (el.value.trim() === '') return null;
      const ty = el.getAttribute('data-ty');
      return { expr: ty === 'symbol' ? ':' + el.value.trim() : el.value.trim() };
    } else if (w === 'enum') {
      if (!el.value) return null;
      return { expr: ':' + el.value };
    } else if (w === 'ref') {
      if (!el.value) return null;
      return { expr: el.value };
    }
    if (el.value.trim() === '') return null;
    return { expr: el.value.trim() };
  }

  // Populate any @ref dropdowns in a container from their kind's instances.
  async function fillRefs(container) {
    for (const sel of container.querySelectorAll('select[data-w="ref"]')) {
      const kind = sel.getAttribute('data-refkind');
      const cur = sel.getAttribute('data-cur');
      try {
        const objs = await getJSON('/__wdoc_objects?kind=' + encodeURIComponent(kind) + pfq());
        sel.innerHTML = '<option value=""></option>' + objs.map(o =>
          `<option ${o.label === cur ? 'selected' : ''}>${esc(o.label)}</option>`).join('');
      } catch (_) { /* leave empty */ }
    }
  }

  // Render a <div class=wcl-ed-row> per editable field. Skips child/children
  // (edited on the page) but notes them.
  function renderForm(schema, values) {
    const rows = [];
    for (const f of schema.fields) {
      if (f.widget === 'child' || f.widget === 'children') {
        rows.push(`<div class="wcl-ed-ro">${esc(f.name)}: nested ${esc(f.child_kind || '')} — edit on the page</div>`);
        continue;
      }
      const cur = values && values[f.name];
      const req = f.optional ? '' : ' *';
      const hint = f.doc ? `<div class="hint">${esc(f.doc)}</div>` : '';
      rows.push(
        `<div class="wcl-ed-row" data-name="${esc(f.name)}" data-slot="${f.inline_slot == null ? '' : f.inline_slot}">` +
        `<label>${esc(f.name)}${req} <span class="wcl-ed-tag">${esc(f.type)}</span></label>` +
        makeInput(f, cur) + hint + `</div>`);
    }
    return rows.join('');
  }

  // Collect the form rows into a fields[] payload (name, inline_slot?, raw|expr).
  function collectFields(container) {
    const out = [];
    for (const row of container.querySelectorAll('.wcl-ed-row')) {
      const val = readInput(row);
      if (!val) continue;
      const slot = row.getAttribute('data-slot');
      const entry = { name: row.getAttribute('data-name'), ...val };
      if (slot !== '') entry.inline_slot = Number(slot);
      out.push(entry);
    }
    return out;
  }

  // ---- page block side panel ---------------------------------------------

  let panel = null;
  let selected = null;
  function closePanel() {
    if (panel) { panel.remove(); panel = null; }
    if (selected) { selected.classList.remove('wcl-ed-sel'); selected = null; }
  }

  async function selectBlock(elOrSpan) {
    closePanel();
    let span, file, kind, el = null;
    if (typeof elOrSpan === 'object' && elOrSpan.nodeType) {
      el = elOrSpan;
      span = el.getAttribute('data-wcl-span');
      file = el.getAttribute('data-wcl-file');
      kind = el.getAttribute('data-wcl-kind');
    }
    if (!span || !file) return;
    if (el) { selected = el; el.classList.add('wcl-ed-sel'); }

    panel = document.createElement('div');
    panel.className = 'wcl-ed-panel';
    panel.innerHTML =
      `<h3><span>Edit: ${esc(kind)}</span><button class="wcl-ed-x" data-x>✕</button></h3>` +
      `<div class="wcl-ed-body">Loading…</div>` +
      `<div class="wcl-ed-foot">` +
      `<button data-save>Save</button>` +
      `<button class="ghost" data-add>＋ Add after</button>` +
      `<button class="ghost" data-up>↑</button><button class="ghost" data-down>↓</button>` +
      `<button class="danger" data-del>Delete</button></div>`;
    document.body.appendChild(panel);
    panel.querySelector('[data-x]').onclick = closePanel;
    const body = panel.querySelector('.wcl-ed-body');

    let schema, values;
    try {
      [schema, values] = await Promise.all([
        schemaFor(kind),
        getJSON('/__wdoc_object?kind=' + encodeURIComponent(kind) +
          '&file=' + encodeURIComponent(file) + '&span=' + encodeURIComponent(span) + pfq()),
      ]);
    } catch (e) { body.innerHTML = `<div class="wcl-ed-err">⚠ ${esc(e.message || e)}</div>`; return; }

    const ro = schema.is_imported;
    body.innerHTML = (ro ? `<div class="wcl-ed-ro">Schema defined in an imported library — fields read-only.</div>` : '') +
      renderForm(schema, values);
    fillRefs(body);

    const showErr = m => {
      let e = body.querySelector('.wcl-ed-err');
      if (!e) { e = document.createElement('div'); e.className = 'wcl-ed-err'; body.appendChild(e); }
      e.textContent = '⚠ ' + m;
    };
    panel.querySelector('[data-save]').onclick = async () => {
      const fields = collectFields(body);
      // Save each field via the field endpoint (carries its own inline_slot).
      for (const f of fields) {
        const payload = { file, span, name: f.name, ...(f.inline_slot != null ? { inline_slot: f.inline_slot } : {}) };
        if ('raw' in f) payload.raw = f.raw; else payload.expr = f.expr;
        await save(payload, '/__wdoc_edit/field', { view: 'page' }, showErr);
      }
    };
    panel.querySelector('[data-del]').onclick = () =>
      save({ file, span }, '/__wdoc_edit/delete', { view: 'page' }, showErr);
    panel.querySelector('[data-up]').onclick = () =>
      save({ file, span, direction: 'up' }, '/__wdoc_edit/move', { view: 'page' }, showErr);
    panel.querySelector('[data-down]').onclick = () =>
      save({ file, span, direction: 'down' }, '/__wdoc_edit/move', { view: 'page' }, showErr);
    panel.querySelector('[data-add]').onclick = () => openAdd(file, span, showErr);
  }

  // Add-block popup: choose a kind, fill its inline text, insert after `span`.
  const COMMON_KINDS = ['p', 'h1', 'h2', 'h3', 'h4', 'callout', 'code'];
  async function openAdd(file, afterSpan, showErr) {
    const kind = window.prompt('New block kind:', 'p');
    if (!kind) return;
    let schema;
    try { schema = await schemaFor(kind.trim()); }
    catch (e) { showErr('unknown kind: ' + kind); return; }
    const inlineField = schema.fields.find(f => f.inline_slot === 0);
    const fields = [];
    if (inlineField) {
      const text = window.prompt(inlineField.name + ':', '') || '';
      fields.push({ name: inlineField.name, inline_slot: 0, raw: text });
    }
    await save({ file, after_span: afterSpan, kind: kind.trim(), fields },
      '/__wdoc_edit/add', { view: 'page' }, showErr);
  }

  // ---- inline text editing -----------------------------------------------

  const TEXT_KINDS = new Set(['p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6']);
  async function inlineEdit(el) {
    const kind = el.getAttribute('data-wcl-kind');
    const span = el.getAttribute('data-wcl-span');
    const file = el.getAttribute('data-wcl-file');
    if (!TEXT_KINDS.has(kind) || !span || !file) return;
    let schema;
    try { schema = await schemaFor(kind); } catch (_) { return; }
    const field = schema.fields.find(f => f.inline_slot === 0 && (f.widget === 'text' || f.widget === 'symbol'));
    if (!field) return;
    const target = el; // the block's root carries the text
    const before = target.textContent;
    target.setAttribute('contenteditable', 'true');
    target.focus();
    const finish = (commit) => {
      target.removeAttribute('contenteditable');
      target.onblur = null; target.onkeydown = null;
      if (!commit) { target.textContent = before; return; }
      const text = target.textContent;
      if (text === before) return;
      save({ file, span, name: field.name, inline_slot: 0, raw: text },
        '/__wdoc_edit/field', { view: 'page' }, m => alert('Edit failed: ' + m));
    };
    target.onblur = () => finish(true);
    target.onkeydown = e => {
      if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); target.blur(); }
      if (e.key === 'Escape') { e.preventDefault(); finish(false); }
    };
  }

  // ---- object editor ------------------------------------------------------

  let modal = null;
  function closeModal() { if (modal) { modal.remove(); modal = null; } }
  function openModalShell(title) {
    closeModal();
    modal = document.createElement('div');
    modal.className = 'wcl-ed-modal';
    modal.innerHTML = `<div class="wcl-ed-modal-box"><h3><span>${esc(title)}</span>` +
      `<button class="wcl-ed-x" data-x>✕</button></h3><div class="wcl-ed-body">Loading…</div></div>`;
    document.body.appendChild(modal);
    modal.querySelector('[data-x]').onclick = closeModal;
    modal.addEventListener('click', e => { if (e.target === modal) closeModal(); });
    return modal.querySelector('.wcl-ed-body');
  }

  // Remember the chosen namespace filter across reopens within a session.
  let objNs = null;
  async function openObjects() {
    const body = openModalShell('Data objects');
    let kinds;
    try { kinds = await getJSON('/__wdoc_object_kinds?_=1' + pfq()); }
    catch (e) { body.innerHTML = `<div class="wcl-ed-err">⚠ ${esc(e.message || e)}</div>`; return; }
    // Distinct namespaces, sorted ('' = root). Default to root if present.
    const namespaces = [...new Set(kinds.map(k => k.namespace || ''))].sort();
    if (objNs == null) objNs = namespaces.includes('') ? '' : '*';
    const nsLabel = ns => ns === '' ? '(root)' : ns;
    const opts = ['*', ...namespaces].map(ns =>
      `<option value="${esc(ns)}" ${ns === objNs ? 'selected' : ''}>${ns === '*' ? 'All namespaces' : esc(nsLabel(ns))}</option>`).join('');
    const renderList = () => {
      const list = kinds.filter(k => objNs === '*' || (k.namespace || '') === objNs); // already alphabetical
      const items = list.map(k =>
        `<div class="wcl-ed-item"><span>${esc(k.kind)} <span class="wcl-ed-tag">${esc(k.type_name)}` +
        `${k.is_imported ? ' · imported' : ''}</span></span>` +
        `<button data-kind="${esc(k.kind)}">Open</button></div>`).join('');
      return items || `<div class="wcl-ed-ro">No object kinds in this namespace.</div>`;
    };
    const draw = () => {
      body.innerHTML =
        `<div class="wcl-ed-row"><label>Namespace</label><select id="wcl-ed-ns">${opts}</select></div>` +
        `<div id="wcl-ed-kinds">${renderList()}</div>`;
      body.querySelector('#wcl-ed-ns').onchange = e => {
        objNs = e.target.value;
        body.querySelector('#wcl-ed-kinds').innerHTML = renderList();
        wire();
      };
      wire();
    };
    const wire = () => body.querySelectorAll('[data-kind]').forEach(b =>
      b.onclick = () => openKind(b.getAttribute('data-kind')));
    draw();
  }

  async function openKind(kind) {
    const body = openModalShell('Objects: ' + kind);
    let objs;
    try { objs = await getJSON('/__wdoc_objects?kind=' + encodeURIComponent(kind) + pfq()); }
    catch (e) { body.innerHTML = `<div class="wcl-ed-err">⚠ ${esc(e.message || e)}</div>`; return; }
    body.innerHTML =
      `<button class="ghost" data-back>← Kinds</button>` +
      objs.map(o =>
        `<div class="wcl-ed-item"><span>${esc(o.label)} <span class="wcl-ed-tag">${esc(shortFile(o.file))}</span></span>` +
        `<span class="acts"><button data-edit='${esc(JSON.stringify({ file: o.file, span: o.span }))}'>Edit</button>` +
        `<button class="danger" data-del='${esc(JSON.stringify({ file: o.file, span: o.span }))}'>Delete</button></span></div>`
      ).join('') + `<button data-new>＋ New ${esc(kind)}</button>`;
    body.querySelector('[data-back]').onclick = openObjects;
    body.querySelector('[data-new]').onclick = () => openObjectText(kind, null);
    body.querySelectorAll('[data-edit]').forEach(b =>
      b.onclick = () => openObjectText(kind, JSON.parse(b.getAttribute('data-edit'))));
    body.querySelectorAll('[data-del]').forEach(b =>
      b.onclick = () => {
        const t = JSON.parse(b.getAttribute('data-del'));
        save({ op: 'delete', file: t.file, span: t.span }, '/__wdoc_object',
          { view: 'kind', kind }, m => alert('Delete failed: ' + m));
      });
  }

  // Edit an object as raw WCL text. For an existing object the text box is its
  // source (saved as a byte-splice); for a new one it's a schema template, with
  // a target-file picker prefilled from the `@wdoc.file` decorator.
  async function openObjectText(kind, target) {
    const isNew = !target;
    const body = openModalShell((isNew ? 'New ' : 'Edit ') + kind);
    // Give the text editor most of the viewport — the source needs room.
    body.closest('.wcl-ed-modal-box').classList.add('wcl-ed-wide');
    let data;
    try {
      data = isNew
        ? await getJSON('/__wdoc_object_template?kind=' + encodeURIComponent(kind) + pfq())
        : await getJSON('/__wdoc_object_source?file=' + encodeURIComponent(target.file) +
          '&span=' + encodeURIComponent(target.span) + pfq());
    } catch (e) { body.innerHTML = `<div class="wcl-ed-err">⚠ ${esc(e.message || e)}</div>`; return; }
    const fd = isNew ? data.file_default : null;
    const fileRow = isNew
      ? `<div class="wcl-ed-row"><label>Target file</label>` +
        `<input type="text" id="wcl-ed-target" value="${esc(fd ? fd.path : '')}" placeholder="(document's own file)">` +
        `<div class="hint">${fd && fd.folder ? 'folder: one file per object' : "where the new object is written"}</div></div>`
      : `<div class="wcl-ed-row"><label>Source <span class="wcl-ed-tag">${esc(shortFile(target.file))}</span></label></div>`;
    body.innerHTML = fileRow +
      `<div class="wcl-ed-srcmount" style="display:flex;flex-direction:column;min-height:45vh;flex:1"></div>`;
    // The shared source-editor component (highlighting, line numbers, live
    // dry-run diagnostics) replaces the bare textarea; the check runs against
    // the object's real file, so only existing objects get schema checking.
    const ed = window.WclEditor.create(body.querySelector('.wcl-ed-srcmount'), {
      value: data.text || '',
      path: isNew ? null : target.file,
      pageFile: pageFile() || undefined,
    });
    const foot = document.createElement('div');
    foot.className = 'wcl-ed-foot';
    foot.innerHTML = `<button data-save>Save</button><button class="ghost" data-fmt>Format</button><button class="ghost" data-cancel>Cancel</button>`;
    body.appendChild(foot);
    const showErr = m => {
      let e = body.querySelector('.wcl-ed-err');
      if (!e) { e = document.createElement('div'); e.className = 'wcl-ed-err'; body.appendChild(e); }
      e.textContent = '⚠ ' + m;
    };
    foot.querySelector('[data-cancel]').onclick = () => openKind(kind);
    foot.querySelector('[data-fmt]').onclick = () =>
      ed.format().catch(e => showErr(e.message || e));
    foot.querySelector('[data-save]').onclick = () => {
      const text = ed.getValue();
      const payload = isNew
        ? { op: 'create', kind, text, ...(targetFileValue(body) ? { target_file: targetFileValue(body) } : {}) }
        : { op: 'save', file: target.file, span: target.span, text };
      save(payload, '/__wdoc_object', { view: 'kind', kind }, showErr);
    };
    ed.focus();
  }

  const targetFileValue = body => ((body.querySelector('#wcl-ed-target') || {}).value || '').trim();
  const shortFile = f => f.split('/').slice(-2).join('/');

  // ---- source view: file tree + whole-file editor -------------------------

  // Browse every .wcl under the page's sub-site (or the served root) and edit
  // whole files — highlighted, live-checked, saved through the validating
  // pipeline (a save that would introduce schema errors is rejected).
  async function openSource() {
    const body = openModalShell('Source');
    body.closest('.wcl-ed-modal-box').classList.add('wcl-ed-wide');
    let listing;
    try { listing = await getJSON('/__wdoc_files?_=1' + pfq()); }
    catch (e) { body.innerHTML = `<div class="wcl-ed-err">⚠ ${esc(e.message || e)}</div>`; return; }
    body.innerHTML =
      `<div class="wcl-ed-srcgrid"><div class="wcl-ed-files"></div>` +
      `<div class="wcl-ed-srcpane">` +
      `<div class="wcl-ed-srchdr"><span data-cur>select a file…</span><span class="dirty" data-dirty></span></div>` +
      `<div class="wcl-ed-srcmount" style="display:flex;flex-direction:column;flex:1;min-height:0"></div>` +
      `<div class="wcl-ed-foot"><button data-save disabled>Save</button>` +
      `<button data-sr disabled>Save &amp; Rebuild</button>` +
      `<button class="ghost" data-pv disabled>Preview</button>` +
      `<button class="ghost" data-fmt disabled>Format</button></div>` +
      `</div>` +
      `<iframe class="wcl-ed-preview" style="display:none" title="preview"></iframe>` +
      `</div>`;
    const filesEl = body.querySelector('.wcl-ed-files');
    const curEl = body.querySelector('[data-cur]');
    const dirtyEl = body.querySelector('[data-dirty]');
    const btnSave = body.querySelector('[data-save]');
    const btnSR = body.querySelector('[data-sr]');
    const btnFmt = body.querySelector('[data-fmt]');
    const btnPv = body.querySelector('[data-pv]');
    const frame = body.querySelector('.wcl-ed-preview');
    const showErr = m => {
      let e = body.querySelector('.wcl-ed-err');
      if (!e) { e = document.createElement('div'); e.className = 'wcl-ed-err'; body.querySelector('.wcl-ed-srcpane').appendChild(e); }
      e.textContent = '⚠ ' + m;
    };
    const clearErr = () => { const e = body.querySelector('.wcl-ed-err'); if (e) e.remove(); };

    let cur = null; // { path, etag, dirty }
    const ed = window.WclEditor.create(body.querySelector('.wcl-ed-srcmount'), {
      value: '',
      pageFile: pageFile() || undefined,
      onChange: () => { if (cur && !cur.dirty) { cur.dirty = true; dirtyEl.textContent = '● unsaved'; } },
    });

    listing.files.forEach(rel => {
      const row = document.createElement('div');
      row.textContent = rel;
      row.title = rel;
      row.onclick = () => open(rel, row);
      filesEl.appendChild(row);
    });

    async function open(rel, row) {
      if (cur && cur.dirty && !confirm('Discard unsaved changes?')) return;
      clearErr();
      let data;
      try { data = await getJSON('/__wdoc_file?path=' + encodeURIComponent(listing.root + '/' + rel)); }
      catch (e) { showErr(e.message || e); return; }
      filesEl.querySelectorAll('.on').forEach(x => x.classList.remove('on'));
      row.classList.add('on');
      cur = { path: data.path, etag: data.etag, dirty: false };
      ed.setPath(data.path);
      ed.setValue(data.text);
      cur.dirty = false;
      dirtyEl.textContent = '';
      curEl.textContent = rel;
      btnSave.disabled = btnSR.disabled = btnFmt.disabled = btnPv.disabled = false;
      ed.focus();
    }

    async function saveCurrent() {
      if (!cur) return false;
      clearErr();
      try {
        const r = await fetch('/__wdoc_file', {
          method: 'POST', headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ path: cur.path, text: ed.getValue(), base_etag: cur.etag, page_file: pageFile() || undefined }),
        });
        const j = await r.json().catch(() => ({}));
        if (!r.ok) { showErr(j.error || r.statusText); return false; }
        cur.etag = j.etag;
        cur.dirty = false;
        dirtyEl.textContent = '✓ saved';
        setTimeout(() => { if (!cur.dirty) dirtyEl.textContent = ''; }, 1500);
        return true;
      } catch (e) { showErr(e.message || e); return false; }
    }

    btnSave.onclick = saveCurrent;
    btnFmt.onclick = () => ed.format().catch(e => showErr(e.message || e));
    // Preview: render the CURRENT BROWSER PAGE with the unsaved buffer
    // overlaid — nothing touches disk. First preview warms the scratch
    // build (slow once); later ones re-render just the page.
    btnPv.onclick = async () => {
      if (!cur) return;
      clearErr();
      const first = frame.style.display === 'none';
      dirtyEl.textContent = first ? '⟳ warming preview…' : '⟳ previewing…';
      try {
        const r = await fetch('/__wdoc_preview', {
          method: 'POST', headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            page: pageName() || 'index',
            page_file: pageFile() || undefined,
            files: [{ path: cur.path, text: ed.getValue() }],
          }),
        });
        const j = await r.json().catch(() => ({}));
        if (!r.ok) { dirtyEl.textContent = cur.dirty ? '● unsaved' : ''; showErr(j.error || r.statusText); return; }
        frame.style.display = '';
        frame.src = j.href + '?t=' + Date.now();
        dirtyEl.textContent = cur.dirty ? '● unsaved (previewed)' : '';
      } catch (e) { dirtyEl.textContent = cur.dirty ? '● unsaved' : ''; showErr(e.message || e); }
    };
    btnSR.onclick = async () => {
      if (!(await saveCurrent())) return;
      dirtyEl.textContent = '⟳ rebuilding…';
      try {
        await fetch('/__wdoc_rebuild', {
          method: 'POST', headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ page_file: pageFile() || undefined }),
        });
      } catch (_) { /* the reload below shows whatever state the build left */ }
      location.reload();
    };
    body.addEventListener('keydown', e => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); saveCurrent(); }
    });
  }

  // Open the object editor for one specific instance, matched by its inline
  // label (its id for the wskill units) — backs the in-page `edit_object`
  // buttons. Falls back to the kind's instance list when there's no target or
  // the target can't be resolved.
  async function openObjectByTarget(kind, targetId) {
    if (!targetId) { openKind(kind); return; }
    let objs;
    try { objs = await getJSON('/__wdoc_objects?kind=' + encodeURIComponent(kind) + pfq()); }
    catch (e) { openKind(kind); return; }
    const hit = objs.find(o => o.label === targetId);
    if (hit) openObjectText(kind, { file: hit.file, span: hit.span });
    else openKind(kind);
  }

  // ---- select mode + toolbar ---------------------------------------------

  let picking = false, hot = null, hint = null;
  function setPick(on) {
    picking = on;
    document.body.classList.toggle('wcl-ed-picking', on);
    selBtn.classList.toggle('on', on);
    selBtn.textContent = on ? '✕ Cancel' : '✎ Edit blocks';
    if (hot) { hot.classList.remove('wcl-ed-hot'); hot = null; }
    if (on && !hint) {
      hint = document.createElement('div'); hint.className = 'wcl-ed-hint';
      hint.textContent = 'Click a block to edit it · double-click text to edit inline · Esc to stop';
      document.body.appendChild(hint);
    }
    if (!on && hint) { hint.remove(); hint = null; }
  }
  document.addEventListener('mousemove', e => {
    if (!picking) return;
    const el = chrome(e.target) ? null : (e.target.closest && e.target.closest('[data-wcl-block]'));
    if (el !== hot) { if (hot) hot.classList.remove('wcl-ed-hot'); hot = el; if (hot) hot.classList.add('wcl-ed-hot'); }
  });
  document.addEventListener('click', e => {
    if (!picking || chrome(e.target)) return;
    const el = e.target.closest && e.target.closest('[data-wcl-block]');
    if (el) { e.preventDefault(); e.stopPropagation(); selectBlock(el); }
  }, true);
  document.addEventListener('dblclick', e => {
    if (!picking || chrome(e.target)) return;
    const el = e.target.closest && e.target.closest('[data-wcl-block]');
    if (el) { e.preventDefault(); e.stopPropagation(); inlineEdit(el); }
  }, true);
  document.addEventListener('keydown', e => {
    if (e.key === 'Escape') { if (picking) setPick(false); else { closePanel(); closeModal(); } }
  });

  const bar = document.createElement('div');
  bar.className = 'wcl-ed-bar';
  const actions = document.createElement('div');
  actions.className = 'wcl-ed-actions';
  const selBtn = document.createElement('button');
  selBtn.textContent = '✎ Edit blocks';
  selBtn.onclick = () => { setOpen(false); setPick(!picking); };
  const objBtn = document.createElement('button');
  objBtn.textContent = '⛁ Objects';
  objBtn.onclick = () => { setOpen(false); setPick(false); openObjects(); };
  const srcBtn = document.createElement('button');
  srcBtn.textContent = '⌨ Source';
  srcBtn.onclick = () => { setOpen(false); setPick(false); openSource(); };
  actions.appendChild(selBtn);
  actions.appendChild(objBtn);
  actions.appendChild(srcBtn);
  const toggle = document.createElement('button');
  toggle.className = 'wcl-ed-toggle';
  toggle.title = 'Editor tools';
  toggle.textContent = '✎';
  function setOpen(open) { bar.classList.toggle('wcl-ed-open', open); toggle.textContent = open ? '✕' : '✎'; }
  toggle.onclick = () => setOpen(!bar.classList.contains('wcl-ed-open'));
  bar.appendChild(actions);
  bar.appendChild(toggle);
  document.body.appendChild(bar);

  // In-page `edit_object` buttons (rendered by the server, edit mode only):
  // jump straight into the object editor for the targeted instance.
  document.querySelectorAll('.wcl-edit-object-btn').forEach(b => {
    b.addEventListener('click', e => {
      e.preventDefault(); e.stopPropagation();
      const kind = b.getAttribute('data-wcl-edit-kind');
      if (kind) { setOpen(true); openObjectByTarget(kind, b.getAttribute('data-wcl-edit-target')); }
    });
  });

  // Restore the view after a save-triggered reload.
  const r = popStash();
  if (r) {
    setOpen(true);
    if (r.view === 'kind' && r.kind) openKind(r.kind);
    else if (r.view === 'objects') openObjects();
    else if (r.view === 'page') setPick(true);
  }
})();
