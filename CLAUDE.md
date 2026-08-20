# Project Instructions

## WCL
WCL is a language for describing data. It has the following key design goals:
- Support for any shape of data.
- Easy for humans to read.
- Can be read into meachine readable formats.
- Support for strict schemas.

## WDOC
WDOC is a dsl built on top of WCL that is designed for creating documents. It is designed
to be more rich than markdown and allows generation of documents over data stored in wcl format.

## Directives:
- IMPORTANT: A change to the language lands in three places, each written for its own reader:
    - `crates/` — the behaviour itself.
    - `docs/reference/` — the chapter a human reads on wcl.dev. A book: prose, worked
      examples, read in order the first time.
    - `.claude/skills/wcl/` — what an agent needs in order to *use* wcl. Syntax, the closed
      lists (every builtin, every CLI flag), and the gotchas no config confesses. Shaped for
      lookup, and reached through the router in `SKILL.md`.

  The two documentation trees are **independent**. Each answers to the behaviour in `crates/`,
  never to the other, so a chapter in one needs no counterpart in the other and neither is a
  translation of its neighbour. Write each for its own reader and let them differ.

  Verify a claim against the crate or the binary before writing it down. Both trees state that
  their examples were run.

## Repo Layout

- `crates/wcl_lang` — the language library. This is where the main wcl language lives. This 
    is consumed by external tools and the wcl cli tool.
- `crates/wcl` — the `wcl` binary that ships with wcl. This houses the following:
    - General tools for working with wcl files.
    - LSP server for working with WCL files (called by plugins).
    - WDOC code for templating, validation, building, serving etc.
- `crates/wcl_lsp` — the code for the LSP server that is run by the cli.
- `crates/wcl_wdoc` — the library crate behind wdoc itself. Keep all wdoc specific code in
    here so clients who are just interested in wcl don't get the extra code.
- `crates/wcl_lang/fuzz` — `cargo-fuzz` tests to help find bugs in the code.
- `editors/vscode` — VSCode extention for wcl.
- `editors/tree-sitter-wcl` — a tree-sitter grammar for wcl.
- `examples/` — examples of using wcl and used by tests.
- `docs/` — Documentation for the project lives in here.
  - `landing/main.wcl` - Landing page (https://wcl.dev).
  - `reference/main.wcl` - Reference manual for WCL and WDOC (https:/wcl.dev/reference/).
- `.claude/skills/wcl` — wcl skill for writing and working with wcl and wdoc.
- `README.md` — the user-facing quickstart.

