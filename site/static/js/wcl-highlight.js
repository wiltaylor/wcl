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
    contains: [],
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
      {
        className: "comment",
        begin: /\/\/\//,
        end: /$/,
        relevance: 1,
      },
      hljs.C_LINE_COMMENT_MODE,
      hljs.C_BLOCK_COMMENT_MODE,
      STRING,
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
      {
        className: "meta",
        begin: /@[a-zA-Z_][a-zA-Z0-9_]*/,
        relevance: 5,
      },
      {
        className: "title.function",
        begin: /\b[a-zA-Z_][a-zA-Z0-9_]*(?=\s*\()/,
      },
    ],
  };
});
