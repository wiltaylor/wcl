// highlight.js language definition for WCL
// Translated from editors/vscode/syntaxes/wcl.tmLanguage.json
hljs.registerLanguage("wcl", function (hljs) {
  var INTERPOLATION = {
    className: "subst",
    begin: /\$\{/,
    end: /\}/,
    keywords: {
      keyword:
        "if else for in when let partial macro schema table validation " +
        "decorator_schema import export inject set remove self query ref declare fn",
      literal: "true false null",
      type: "string int float bool identifier any list map set union",
    },
    contains: [], // filled later
  };

  var STRING = {
    className: "string",
    begin: '"',
    end: '"',
    contains: [hljs.BACKSLASH_ESCAPE, INTERPOLATION],
  };

  INTERPOLATION.contains = [
    STRING,
    hljs.C_NUMBER_MODE,
    { className: "variable", begin: /\b[a-zA-Z_][a-zA-Z0-9_]*\b/ },
  ];

  return {
    name: "WCL",
    case_insensitive: false,
    keywords: {
      keyword:
        "if else for in when let partial macro schema table validation " +
        "decorator_schema import export inject set remove self query ref declare fn",
      literal: "true false null",
      type: "string int float bool identifier any list map set union",
    },
    contains: [
      // Doc comments (must come before line comments)
      {
        className: "comment",
        begin: /\/\/\//,
        end: /$/,
        relevance: 1,
      },
      hljs.C_LINE_COMMENT_MODE,
      hljs.C_BLOCK_COMMENT_MODE,
      STRING,
      // Hex, octal, binary, float, integer
      {
        className: "number",
        variants: [
          { begin: /\b0[xX][0-9a-fA-F]+\b/ },
          { begin: /\b0[oO][0-7]+\b/ },
          { begin: /\b0[bB][01]+\b/ },
          { begin: /\b\d+\.\d+([eE][+-]?\d+)?\b/ },
          { begin: /\b\d+\b/ },
        ],
        relevance: 0,
      },
      // Decorators
      {
        className: "meta",
        begin: /@[a-zA-Z_][a-zA-Z0-9_]*/,
        relevance: 5,
      },
      // Function calls
      {
        className: "title.function",
        begin: /\b[a-zA-Z_][a-zA-Z0-9_]*(?=\s*\()/,
      },
    ],
  };
});

// Re-highlight all WCL code blocks.
// mdBook's book.js already ran highlightBlock on these before 'wcl' was
// registered, so they got auto-detected as another language. We must
// re-highlight from the raw text using the correct hljs 10.x API.
document.querySelectorAll("code.language-wcl").forEach(function (block) {
  var result = hljs.highlight("wcl", block.textContent, true);
  block.innerHTML = result.value;
  block.classList.add("hljs");
});
