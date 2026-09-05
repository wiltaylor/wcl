# wcl-vscode

A minimal VS Code extension that wires `wcl lsp` into VS Code as the language server for `.wcl` files.

## Build

```bash
cd editors/vscode
npm install
npm run compile
```

Then point VS Code at the folder via `code --extensionDevelopmentPath=$(pwd)` (or use the "Extension Development Host" launch config) and open any `.wcl` file.

## Configuration

- `wcl.serverPath` (default `"wcl"`) — path to the `wcl` binary on disk. When left
  at the default, the extension probes `~/.cargo/bin/wcl`, `/usr/local/bin/wcl`, and
  `/opt/homebrew/bin/wcl` in order so a `cargo install`-installed binary works even
  when VS Code is launched from a desktop launcher (which doesn't inherit your
  shell's `PATH`). Set this to an absolute path if your binary lives elsewhere.

## Highlighting

Syntax highlighting comes from a bundled TextMate grammar at
`syntaxes/wcl.tmLanguage.json` and works the moment a `.wcl` file is opened,
without needing the language server to be reachable. Once the LSP connects, its
semantic tokens layer on top to refine identifier categories the TextMate grammar
can't disambiguate (e.g., distinguishing user-defined types from fields).

## Rename

Rename uses declaration identities and lexical scope, including unsaved imported
files. Comments, string text, and unrelated names are left unchanged.

The server rejects renames of block and decorator kind names, union variants,
and shorthand pattern bindings. Those forms need additional
reference information or syntax changes before the server can rename them safely.
Symbol-set entries do not offer rename.

## Status

This is a contributor-built stub — there is no marketplace listing. Most of the
language intelligence (diagnostics, completion, hover, semantic tokens, …) comes
from the server in `crates/wcl_lsp`; the extension itself wires up the language
client and ships the TextMate grammar for static highlighting.
