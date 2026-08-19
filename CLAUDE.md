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
- IMPORTANT: When you make changes to the language you must do the following:
    - UPDATE `docs/reference/pages/<area>/<stem>.wcl` — the chapter a human reads on wcl.dev.
    - UPDATE `.claude/skills/wcl/references/<area>/<stem>.md` — the same material an agent reads.

Failing to make these updates will confuse both human users and AI Agents.

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

