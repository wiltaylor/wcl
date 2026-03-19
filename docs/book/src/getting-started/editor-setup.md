# Editor Setup

WCL ships a language server (`wcl lsp`) that implements the Language Server Protocol. Any LSP-capable editor can use it. This page covers setup for VS Code, Neovim, and Helix.

## LSP Features

- Real-time diagnostics (parse errors, type mismatches, schema violations)
- Hover documentation for blocks, attributes, and schema definitions
- Go-to-definition for block references and macro uses
- Completions for attribute names, block types, schema names, and decorator names
- Semantic token highlighting
- Signature help for macro and built-in function calls
- Find references
- Document formatting (`wcl fmt` integration)

## VS Code

A bundled VS Code extension is located in `editors/vscode/` in the repository. It handles syntax highlighting, LSP integration, and file association for `.wcl` files.

**Install with `just`** (if you have [just](https://github.com/casey/just) installed):

```bash
just install-vscode
```

**Install manually:**

```bash
cd editors/vscode && npm install
ln -sfn "$(pwd)" ~/.vscode/extensions/wil.wcl-0.1.0
```

The symlink approach means changes to the extension source are picked up immediately without reinstalling. Restart VS Code (or run "Developer: Reload Window") after linking.

The extension automatically starts `wcl lsp` when a `.wcl` file is opened. Make sure `wcl` is on your `PATH` (i.e., installed via `cargo install --path wcl_cli`).

## Neovim

Add the following to your Neovim configuration (e.g., in an `ftplugin/wcl.lua` or your main `init.lua`):

```lua
vim.lsp.start({
    name = "wcl",
    cmd = { "wcl", "lsp" },
    root_dir = vim.fs.dirname(vim.fs.find({ ".git" }, { upward = true })[1]),
    filetypes = { "wcl" },
})
```

You will also want to register the `.wcl` filetype so Neovim recognizes it:

```lua
vim.filetype.add({
    extension = {
        wcl = "wcl",
    },
})
```

If you use `nvim-lspconfig`, a custom server entry works the same way — pass `cmd = { "wcl", "lsp" }` and set `filetypes = { "wcl" }`.

## Helix

Add the following to your `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "wcl"
scope = "source.wcl"
file-types = ["wcl"]
language-servers = ["wcl-lsp"]

[language-server.wcl-lsp]
command = "wcl"
args = ["lsp"]
```

Helix will start `wcl lsp` automatically when a `.wcl` file is opened. Run `hx --health wcl` to verify the language server is detected correctly.

## Other Editors

Any editor with LSP support can use the WCL language server. The server communicates over stdio and is started with:

```bash
wcl lsp
```

Refer to your editor's LSP documentation for how to register a custom language server with that command.
