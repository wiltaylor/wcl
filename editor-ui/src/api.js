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
  /** Full build of one site with unsaved buffers overlaid → { ok, href } */
  preview: (entry, site, files) => json('POST', '/api/preview', { entry, site, files }),
  /** Resolve an edit_object target to its source → { file, span: {start, end} } */
  locateObject: (payload) => json('POST', '/api/object/locate', payload),
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
