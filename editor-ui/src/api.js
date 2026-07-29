/* Thin fetch wrappers over the `wcl editor` backend. Every JSON endpoint
   answers { ...payload } on success or { error } with a 4xx status. */

async function json(method, url, body) {
  let res;
  try {
    res = await fetch(url, {
      method,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (e) {
    return { ok: false, status: 0, error: String(e) };
  }
  let data = {};
  try {
    data = await res.json();
  } catch {
    /* non-JSON error body */
  }
  if (!res.ok) {
    return { ok: false, status: res.status, error: data.error ?? res.statusText };
  }
  return { ok: true, status: res.status, ...data };
}

export const api = {
  /** → { root, files: [{path, type:"file"|"dir"}] } */
  files: () => json('GET', '/api/files'),
  /** → { path, text, etag } */
  readFile: (path) => json('GET', `/api/file?path=${encodeURIComponent(path)}`),
  /** → { ok, etag } | 409 conflict | 400 validation message */
  saveFile: (path, text, baseEtag) =>
    json('POST', '/api/file', {
      path,
      text,
      ...(baseEtag ? { base_etag: baseEtag } : {}),
    }),
  /** → { text } */
  format: (text) => json('POST', '/api/format', { text }),
  /** → { sites: [{entry, site, label, skill, children: […]}] } */
  sites: () => json('GET', '/api/sites'),
  /** Full build of one site with unsaved buffers overlaid → { ok, href }.
      `extra` may carry { pages, changed } for a targeted Design-mode
      re-render into a warm output dir. */
  preview: (entry, site, files, extra = {}) =>
    json('POST', '/api/preview', { entry, site, files, ...extra }),
  /** Resolve an edit_object target to its source → { file, span: {start, end} } */
  locateObject: (payload) => json('POST', '/api/object/locate', payload),
  /** A block's exact source + slot classification → { kind, source, etag,
      labels: [{slot, state, text}], fields: {name: {state, text}} } */
  blockSource: (payload) => json('POST', '/api/block/source', payload),
  /** A batch of span-addressed mutations on one file → { file, etag,
      file_text, spans: [{role, span}] } */
  blockOps: (payload) => json('POST', '/api/block/ops', payload),
  /** Set one field on a located data object (edit_field write path) */
  unitField: (payload) => json('POST', '/api/unit/field', payload),
  /** Create a data-object instance (file placement + optional index pin) */
  unitCreate: (payload) => json('POST', '/api/unit/create', payload),
  /** → { site_type, wskill, unit_kinds, body_kinds, components } */
  palette: (entry, site, pageFile) =>
    json(
      'GET',
      `/api/palette?entry=${encodeURIComponent(entry)}&site=${encodeURIComponent(site ?? '')}` +
        (pageFile ? `&page_file=${encodeURIComponent(pageFile)}` : ''),
    ),
  /** → { site_type, wskill, nav, units?, pages, container? } */
  nav: (entry, site) =>
    json('GET', `/api/nav?entry=${encodeURIComponent(entry)}&site=${encodeURIComponent(site ?? '')}`),
  /** Structural nav edit → { ok } */
  navOp: (payload) => json('POST', '/api/nav/op', payload),
  /** Enable/disable a wskill profile (artifact + projection files) */
  wskillProfile: (registry, kind, enable) =>
    json('POST', '/api/wskill/profile', { registry, kind, enable }),
  /** The unit graph: laid-out nodes + edges + per-view block visibility.
      `kinds` maps each site to its artifact kind (`site=kind` pairs) so the
      server can fold audience routing into the per-view booleans. */
  graph: (entry, sites, kinds = []) =>
    json(
      'GET',
      `/api/graph?entry=${encodeURIComponent(entry)}&sites=${encodeURIComponent(sites.join(','))}&kinds=${encodeURIComponent(kinds.join(','))}`,
    ),
  /** The Systems view's model: schema-derived containment metadata
      (`kinds` with their parent links / refs / edge role) plus every data
      object as a node and every relation as an edge. */
  systems: (entry) => json('GET', `/api/systems?entry=${encodeURIComponent(entry)}`),
  /** One object in full: properties, child-block families, body, relations. */
  systemsDetail: (payload) => json('POST', '/api/systems/detail', payload),
  /** Data mode: `@wdoc.editable` types with form metadata + target files */
  dataTypes: (entry) => json('GET', `/api/data/types?entry=${encodeURIComponent(entry)}`),
  /** Data mode: one kind's instances as classified table rows */
  dataRows: (entry, kind) =>
    json(
      'GET',
      `/api/data/rows?entry=${encodeURIComponent(entry)}&kind=${encodeURIComponent(kind)}`,
    ),
  rawUrl: (path) => `/api/raw?path=${encodeURIComponent(path)}`,
  /** → { comments: [{id, scope, page, page_file, loc, target, quote, body, …}] } */
  comments: () => json('GET', '/api/comments'),
  /** payload: { page, page_file?, loc?, target?, body, quote? } → { id } */
  addComment: (payload) => json('POST', '/api/comments', payload),
  resolveComment: (id) => json('POST', '/api/comments/resolve', { id }),
  editComment: (id, body) => json('POST', '/api/comments/edit', { id, body }),
  reviewReady: () => json('POST', '/api/review/ready'),
  /** Review-handshake long-poll (parks server-side up to ~30s). The signal
      lets the poll loop abort on teardown. → { waiting, round } */
  reviewStatus: async (round, signal) => {
    try {
      const res = await fetch(`/api/review/status?round=${round}`, { signal });
      if (!res.ok) return { ok: false, status: res.status };
      return { ok: true, ...(await res.json()) };
    } catch (e) {
      return { ok: false, status: 0, error: String(e) };
    }
  },
};
