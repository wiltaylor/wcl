# WCL Tree-sitter Highlight Queries

Additional tree-sitter query files for editor integration beyond basic syntax highlighting.

The canonical **highlights.scm** lives at `extras/tree-sitter-wcl/queries/highlights.scm` and is used directly by tree-sitter, Neovim, Helix, Zed, and GitHub.

This directory contains supplementary query files:

| File | Purpose | Used by |
|------|---------|---------|
| `locals.scm` | Scope-aware variable highlighting | Neovim, tree-sitter-highlight |
| `textobjects.scm` | Structural text objects (select block, function, etc.) | Neovim (nvim-treesitter-textobjects), Helix |
| `injections.scm` | Embedded language injection (string interpolation) | Neovim, Helix |

## Neovim Setup

Copy or symlink all query files to your Neovim runtime:

```bash
mkdir -p ~/.config/nvim/queries/wcl
cp extras/tree-sitter-wcl/queries/highlights.scm ~/.config/nvim/queries/wcl/
cp extras/highlight-queries/*.scm ~/.config/nvim/queries/wcl/
```

## Helix Setup

```bash
mkdir -p ~/.config/helix/runtime/queries/wcl
cp extras/tree-sitter-wcl/queries/highlights.scm ~/.config/helix/runtime/queries/wcl/
cp extras/highlight-queries/textobjects.scm ~/.config/helix/runtime/queries/wcl/
cp extras/highlight-queries/injections.scm ~/.config/helix/runtime/queries/wcl/
```
