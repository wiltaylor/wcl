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

- `wcl.serverPath` (default `"wcl"`) — path to the `wcl` binary on disk. Override if the binary isn't on your `$PATH`.

## Status

This is a contributor-built stub — there is no marketplace listing. The extension only wires up the language-client side; all of the language intelligence (diagnostics, completion, hover, semantic tokens, …) comes from the server in `crates/wcl_lsp`.
