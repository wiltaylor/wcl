/* CodeMirror StreamLanguage for WCL, ported from the tmLanguage grammar
   (editors/vscode/syntaxes/wcl.tmLanguage.json) and the real lexer's token
   inventory. Highlighting only — diagnostics/completion come from the LSP. */

import { StreamLanguage } from '@codemirror/language';

const KEYWORDS =
  /^(?:if|else|match|type|interface|union|symbol_set|connection|fn|let|in|namespace|import|use|as|extends)$/;
const CONSTANTS = /^(?:true|false|none)$/;
const BUILTIN_TYPES =
  /^(?:bool|i8|i16|i32|i64|i128|isize|u8|u16|u32|u64|u128|usize|f32|f64|utf8|ascii|utf16|utf32|symbol|identifier|list|tensor)$/;

/* Heredoc opener: optional `$` (interpolating), optional string-type
   prefix, `<<`, then TAG or 'TAG' (raw — verbatim body, never
   interpolates). */
const HEREDOC_OPEN =
  /^(\$)?(?:utf8|ascii|utf16|utf32)?<<(?:'([A-Za-z_][A-Za-z0-9_]*)'|([A-Za-z_][A-Za-z0-9_]*))/;

const ESCAPE = /^\\(?:[\\"'nrt0]|x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f]{1,6}\})/;

/* Inside a `"…"` string body. Consumes at least one char per call. */
function tokenStringBody(stream, state) {
  if (state.interp && stream.match(/^\$\{/)) {
    state.slot = true;
    return 'string';
  }
  if (stream.match(ESCAPE)) return 'escape';
  if (stream.match('"')) {
    state.inString = false;
    state.interp = false;
    return 'string';
  }
  stream.next(); // guaranteed progress (lone '$', stray '\', …)
  while (!stream.eol()) {
    const ch = stream.peek();
    if (ch === '"' || ch === '\\' || (state.interp && ch === '$')) break;
    stream.next();
  }
  if (stream.eol()) {
    // Strings are single-line in WCL; an unterminated one is a lex error —
    // stop highlighting it as a string past this line.
    state.inString = false;
    state.interp = false;
  }
  return 'string';
}

export const wclLanguage = StreamLanguage.define({
  name: 'wcl',
  startState: () => ({
    heredoc: null, // active end tag
    heredocInterp: false,
    inString: false,
    interp: false,
    slot: false, // inside a ${…} interpolation slot
  }),
  token(stream, state) {
    // ${…} slot inside an interpolated string/heredoc: one special run.
    if (state.slot) {
      if (!stream.match(/^[^}]*\}/)) stream.skipToEnd();
      else state.slot = false;
      return 'variableName.special';
    }
    if (state.heredoc) {
      if (stream.sol() && stream.string.slice(stream.pos).trim() === state.heredoc) {
        stream.skipToEnd();
        state.heredoc = null;
        state.heredocInterp = false;
        return 'tagName';
      }
      if (!state.heredocInterp) {
        stream.skipToEnd();
        return 'string';
      }
      if (stream.match(/^\$\{/)) {
        state.slot = true;
        return 'string';
      }
      stream.next(); // guaranteed progress past a lone '$'
      while (!stream.eol() && stream.peek() !== '$') stream.next();
      return 'string';
    }
    if (state.inString) return tokenStringBody(stream, state);

    if (stream.eatSpace()) return null;
    if (stream.match(/^(?:\/\/|#).*/)) return 'comment';

    const hd = stream.match(HEREDOC_OPEN);
    if (hd) {
      state.heredoc = hd[2] ?? hd[3];
      state.heredocInterp = !!hd[1] && !hd[2]; // quoted tag = raw
      return 'tagName';
    }
    const str = stream.match(/^(\$)?(?:utf8|ascii|utf16|utf32)?"/);
    if (str) {
      state.inString = true;
      state.interp = !!str[1];
      return 'string';
    }
    if (stream.match(/^@[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*/)) return 'meta';
    if (stream.match(/^:[A-Za-z_]\w*/)) return 'atom';
    if (stream.match(/^&[A-Z]\w*/)) return 'typeName';
    if (
      stream.match(/^0x[0-9a-fA-F_]+\w*/) ||
      stream.match(/^0b[01_]+\w*/) ||
      stream.match(/^0o[0-7_]+\w*/) ||
      // Decimals, floats, exponents, int/float/unit suffixes (5MiB, 2u32).
      stream.match(/^\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?\w*/)
    ) {
      return 'number';
    }
    if (stream.match(/^[A-Za-z_]\w*/)) {
      const w = stream.current();
      if (CONSTANTS.test(w)) return 'bool';
      if (KEYWORDS.test(w)) return 'keyword';
      if (BUILTIN_TYPES.test(w)) return 'typeName';
      return /^[A-Z]/.test(w) ? 'typeName' : 'variableName';
    }
    if (stream.match(/^(?:=>|->|::|\?\?|==|!=|<=|>=|&&|\|\|)/)) return 'operator';
    stream.next();
    return null;
  },
  languageData: {
    commentTokens: { line: '//' },
  },
});
