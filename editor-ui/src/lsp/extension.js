/* CodeMirror extensions wiring a .wcl buffer to the shared LSP client:
   completion (autocompletion override) and hover tooltips. Document sync
   (didOpen/didChange/didClose) lives in the buffer store, which already
   sees every edit. Diagnostics deliberately do NOT flow through here — @forge/code
   re-applies its `annotations` prop via setDiagnostics on every value
   change, which would wipe anything an extension pushes into the lint
   state. Instead the client's publishDiagnostics are converted to
   annotations in the buffer store and rendered by CodeEditor itself. */

import { autocompletion } from '@codemirror/autocomplete';
import { hoverTooltip } from '@codemirror/view';

import { lsp } from './client';

/** LSP {line, character} (0-based, UTF-16 — which JS strings already are)
    → CodeMirror offset. */
export function posToOffset(doc, pos) {
  if (pos.line >= doc.lines) return doc.length;
  const line = doc.line(pos.line + 1);
  return Math.min(line.from + pos.character, line.to);
}

/** CodeMirror offset → LSP position. */
export function offsetToPos(doc, offset) {
  const line = doc.lineAt(offset);
  return { line: line.number - 1, character: offset - line.from };
}

/** LSP publishDiagnostics params → @forge/code CodeAnnotation[]
    (1-based line, 0-based col). */
export function toAnnotations(diagnostics) {
  const severity = (s) => ['error', 'error', 'warning', 'info', 'hint'][s ?? 1];
  return (diagnostics ?? []).map((d) => ({
    from: { line: d.range.start.line + 1, col: d.range.start.character },
    to: { line: d.range.end.line + 1, col: d.range.end.character },
    severity: severity(d.severity),
    message: d.message,
    source: d.source ?? 'wcl',
  }));
}

const COMPLETION_KIND = {
  2: 'method', 3: 'function', 4: 'class', 5: 'property', 6: 'variable',
  7: 'class', 8: 'interface', 9: 'namespace', 10: 'property', 13: 'enum',
  14: 'keyword', 20: 'constant', 21: 'constant', 22: 'type', 25: 'type',
};

function hoverText(contents) {
  const one = (c) => (typeof c === 'string' ? c : (c?.value ?? ''));
  return (Array.isArray(contents) ? contents.map(one) : [one(contents)])
    .filter(Boolean)
    .join('\n\n');
}

async function lspComplete(uri, ctx) {
  // Fire on explicit request, mid-word, or right after a trigger char.
  const before = ctx.matchBefore(/[\w@:&.]+$/);
  if (!ctx.explicit && !before) return null;
  let result;
  try {
    result = await lsp.requestAt('textDocument/completion', uri, {
      position: offsetToPos(ctx.state.doc, ctx.pos),
    });
  } catch {
    return null;
  }
  const items = Array.isArray(result) ? result : (result?.items ?? []);
  if (!items.length) return null;
  // Replace from where the server's edits start when provided; else the
  // matched word (or the cursor for a fresh trigger-char completion).
  let from = before ? before.from : ctx.pos;
  const firstEdit = items.find((i) => i.textEdit?.range)?.textEdit;
  if (firstEdit) {
    from = posToOffset(ctx.state.doc, firstEdit.range.start);
  }
  return {
    from,
    options: items.map((i) => ({
      label: i.label,
      apply: i.textEdit?.newText ?? i.insertText ?? i.label,
      type: COMPLETION_KIND[i.kind] ?? 'text',
      detail: i.detail,
      info: hoverText(i.documentation) || undefined,
    })),
  };
}

function lspHover(uri) {
  return hoverTooltip(async (view, pos) => {
    let result;
    try {
      result = await lsp.requestAt('textDocument/hover', uri, {
        position: offsetToPos(view.state.doc, pos),
      });
    } catch {
      return null;
    }
    const text = result ? hoverText(result.contents) : '';
    if (!text) return null;
    const start = result.range ? posToOffset(view.state.doc, result.range.start) : pos;
    const end = result.range ? posToOffset(view.state.doc, result.range.end) : pos;
    return {
      pos: start,
      end,
      above: true,
      create() {
        const dom = document.createElement('div');
        dom.className = 'ed-hover';
        dom.style.cssText =
          'max-width:48em;white-space:pre-wrap;font-family:var(--font-mono);' +
          'font-size:var(--fs-xs);padding:6px 8px;';
        dom.textContent = text;
        return { dom };
      },
    };
  });
}

/** The per-document extension bundle injected through @forge/code's
    `language` prop (any Extension is passed straight through and arrays
    flatten). */
export function wclLspExtensions(uri) {
  return [
    autocompletion({ override: [(ctx) => lspComplete(uri, ctx)] }),
    lspHover(uri),
  ];
}
