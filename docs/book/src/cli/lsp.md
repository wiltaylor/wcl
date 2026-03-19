# wcl lsp

Start the WCL Language Server Protocol (LSP) server.

## Usage

```bash
wcl lsp [options]
```

## Options

| Flag | Description |
|------|-------------|
| `--tcp <addr>` | Listen on a TCP address instead of stdio (e.g. `127.0.0.1:9257`) |

## Description

`wcl lsp` starts the WCL language server, which implements the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/). Editors and IDEs connect to it to receive language intelligence features for WCL documents.

By default, the server communicates over stdio, which is the standard transport for most editor integrations (VS Code, Neovim, Helix, etc.). The `--tcp` flag enables a TCP transport useful for debugging or unconventional editor setups.

## Features

| Feature | Description |
|---------|-------------|
| Diagnostics | Real-time errors and warnings as you type, sourced from the full pipeline |
| Hover | Type information and documentation for identifiers, blocks, and schema fields |
| Go to definition | Jump to where a name, macro, schema, or imported identifier is defined |
| Completions | Context-aware completions for identifiers, attribute names, block types, and decorators |
| Semantic tokens | Syntax highlighting based on semantic meaning (not just token type) |
| Signature help | Parameter hints when calling macros or functions |
| Find references | Locate all uses of a definition across the document |
| Formatting | Full-document formatting via `wcl fmt` |

## Editor Integration

### VS Code

Install the `wcl-vscode` extension. It starts `wcl lsp` automatically.

### Neovim (nvim-lspconfig)

```lua
require('lspconfig').wcl.setup({
  cmd = { 'wcl', 'lsp' },
  filetypes = { 'wcl' },
  root_dir = require('lspconfig.util').root_pattern('.git', '*.wcl'),
})
```

### Helix

Add to `languages.toml`:

```toml
[[language]]
name = "wcl"
language-servers = ["wcl-lsp"]

[language-server.wcl-lsp]
command = "wcl"
args = ["lsp"]
```

## Examples

Start in stdio mode (used by editors automatically):

```bash
wcl lsp
```

Start on a TCP port for debugging:

```bash
wcl lsp --tcp 127.0.0.1:9257
```
