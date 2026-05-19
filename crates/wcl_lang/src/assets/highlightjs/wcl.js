/**
 * WCL (Wil's Configuration Language) grammar definition for highlight.js
 *
 * This is the canonical WCL highlight.js grammar. Use it as:
 *   - ES module:   import wcl from './wcl.js'; hljs.registerLanguage('wcl', wcl);
 *   - Script tag:  <script src="wcl.js"></script> -- exposes hljsDefineWcl globally
 *   - CommonJS:    const wcl = require('./wcl'); hljs.registerLanguage('wcl', wcl);
 *
 * The mdbook and Hugo sites source their copies from this file via build recipes.
 *
 * @see https://highlightjs.readthedocs.io/en/latest/language-guide.html
 */
function hljsDefineWcl(hljs) {
  const IDENT = /[a-zA-Z_][a-zA-Z0-9_]*/;
  const IDENT_LIT = /[a-zA-Z_][a-zA-Z0-9_]*-[a-zA-Z0-9_-]*/;

  const KEYWORDS = {
    keyword: [
      'let', 'partial', 'macro', 'schema', 'table', 'validation',
      'decorator_schema', 'declare', 'inject', 'set', 'remove',
      'check', 'message', 'target',
    ],
    'keyword.control': [
      'if', 'else', 'for', 'in', 'when',
    ],
    'keyword.import': [
      'import', 'export',
    ],
    literal: [
      'true', 'false', 'null',
    ],
    type: [
      'string', 'int', 'float', 'bool', 'identifier', 'any',
      'list', 'map', 'set', 'union', 'ref',
    ],
    built_in: [
      'query', 'has', 'import_table', 'import_raw',
      'len', 'keys', 'values', 'contains', 'split', 'join',
      'upper', 'lower', 'trim', 'replace', 'starts_with', 'ends_with',
      'to_string', 'to_int', 'to_float', 'to_bool',
      'abs', 'ceil', 'floor', 'round', 'min', 'max', 'sum',
      'filter', 'map_fn', 'flat_map', 'reduce', 'sort_by', 'group_by',
      'count', 'any_fn', 'all_fn', 'unique', 'flatten', 'zip',
      'range', 'reverse', 'slice', 'concat', 'merge',
      'sha256', 'base64_encode', 'base64_decode', 'json_encode', 'json_decode',
      'env', 'format',
    ],
  };

  const STRING = {
    scope: 'string',
    begin: '"',
    end: '"',
    contains: [
      hljs.BACKSLASH_ESCAPE,
      {
        scope: 'subst',
        begin: /\$\{/,
        end: /\}/,
        keywords: KEYWORDS,
        contains: [],
      },
    ],
  };

  STRING.contains[1].contains = [
    STRING,
    hljs.C_NUMBER_MODE,
  ];

  const HEREDOC = {
    scope: 'string',
    begin: /<<-?'?[a-zA-Z_]\w*'?/,
    end: /^[a-zA-Z_]\w*\s*$/,
    relevance: 5,
  };

  const NUMBER = {
    scope: 'number',
    variants: [
      { begin: /\b\d+\.\d+([eE][+-]?\d+)?\b/ },
      { begin: /\b0[xX][0-9a-fA-F][0-9a-fA-F_]*\b/ },
      { begin: /\b0[oO][0-7][0-7_]*\b/ },
      { begin: /\b0[bB][01][01_]*\b/ },
      { begin: /\b\d[0-9_]*\b/ },
    ],
    relevance: 0,
  };

  const DECORATOR = {
    scope: 'meta',
    begin: /@/,
    end: /(?=[^a-zA-Z0-9_(])/,
    contains: [
      {
        scope: 'keyword',
        begin: /@/,
      },
      {
        scope: 'title.function',
        begin: IDENT,
      },
    ],
  };

  const DOC_COMMENT = hljs.COMMENT(/\/\/\//, /$/);
  DOC_COMMENT.scope = 'comment';

  const BLOCK_TYPE = {
    begin: [
      /(?:partial\s+)?/,
      IDENT,
      /\s+/,
      IDENT_LIT,
    ],
    beginScope: {
      2: 'title.class',
      4: 'title',
    },
    relevance: 0,
  };

  return {
    name: 'WCL',
    aliases: ['wcl'],
    case_insensitive: false,
    keywords: KEYWORDS,
    contains: [
      DOC_COMMENT,
      hljs.C_LINE_COMMENT_MODE,
      hljs.C_BLOCK_COMMENT_MODE,
      STRING,
      HEREDOC,
      NUMBER,
      DECORATOR,
      BLOCK_TYPE,
      {
        begin: [
          /\b(?:schema|validation|decorator_schema)\b/,
          /\s+/,
          /"/,
        ],
        beginScope: {
          1: 'keyword',
        },
      },
      {
        begin: [
          IDENT,
          /\s*\(/,
        ],
        beginScope: {
          1: 'title.function',
        },
        relevance: 0,
      },
      {
        begin: [
          /\bdeclare\b/,
          /\s+/,
          IDENT,
        ],
        beginScope: {
          1: 'keyword',
          3: 'title.function',
        },
      },
      {
        begin: [
          /\bmacro\b/,
          /\s+/,
          IDENT,
        ],
        beginScope: {
          1: 'keyword',
          3: 'title.function',
        },
      },
      {
        begin: [
          /\blet\b/,
          /\s+/,
          IDENT,
        ],
        beginScope: {
          1: 'keyword',
          3: 'variable',
        },
      },
      {
        begin: [
          /\bfor\b/,
          /\s+/,
          IDENT,
        ],
        beginScope: {
          1: 'keyword',
          3: 'variable',
        },
      },
      {
        scope: 'operator',
        match: /=>|->|==|!=|<=|>=|=~|&&|\|\||[+\-*/%=!<>|?:]/,
      },
    ],
  };
}

// Export for various module systems
if (typeof exports === 'object' && typeof module === 'object') {
  module.exports = hljsDefineWcl;
} else if (typeof define === 'function' && define.amd) {
  define([], function () { return hljsDefineWcl; });
}
// In browser: hljsDefineWcl is available as a global
