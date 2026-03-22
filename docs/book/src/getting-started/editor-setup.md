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

The extension automatically starts `wcl lsp` when a `.wcl` file is opened. Make sure `wcl` is on your `PATH` (i.e., installed via `cargo install --path crates/wcl`).

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

## JetBrains (IntelliJ IDEA, WebStorm, etc.)

A JetBrains plugin is located in `editors/jetbrains/` in the repository. It provides syntax highlighting via a TextMate grammar and full LSP integration via [LSP4IJ](https://github.com/redhat-developer/lsp4ij). Works with all JetBrains IDEs including Community Edition.

**Prerequisites:** The plugin requires the [LSP4IJ](https://plugins.jetbrains.com/plugin/23257-lsp4ij) plugin to be installed in your IDE.

**Install from source:**

```bash
cd editors/jetbrains && ./gradlew buildPlugin
```

Then install the resulting ZIP from `editors/jetbrains/build/distributions/` via **Settings > Plugins > Install Plugin from Disk**.

The plugin automatically starts `wcl lsp` when a `.wcl` file is opened. It looks for the `wcl` binary in this order:

1. Bundled binary (if installed from a platform-specific distribution)
2. `~/.cargo/bin/wcl` (if installed via `cargo install`)
3. System `PATH`

## Zed

A Zed extension is located in `editors/zed/` in the repository. It provides tree-sitter-based syntax highlighting and LSP integration.

**Install from source:**

```bash
ln -sfn "$(pwd)/editors/zed" ~/.local/share/zed/extensions/installed/wcl
```

Restart Zed after linking. The extension automatically starts `wcl lsp` when a `.wcl` file is opened. Make sure `wcl` is on your `PATH`.

## Other Editors

Any editor with LSP support can use the WCL language server. The server communicates over stdio and is started with:

```bash
wcl lsp
```

Refer to your editor's LSP documentation for how to register a custom language server with that command.
