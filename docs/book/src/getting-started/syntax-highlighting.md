# Syntax Highlighting

WCL provides syntax highlighting definitions for a wide range of editors, tools, and platforms. These are distributed as standalone files in the `extras/` directory of the repository.

## Overview

| Package | Format | Unlocks |
|---------|--------|---------|
| [Tree-sitter Highlight Queries](#tree-sitter-highlight-queries) | `.scm` | Neovim, Helix, Zed, GitHub |
| [Sublime Syntax](#sublime-text--syntect--bat) | `.sublime-syntax` | Sublime Text, Syntect, bat |
| [TextMate Grammar](#vs-code--shiki) | `.tmLanguage.json` | VS Code, Shiki, Monaco |
| [highlight.js](#highlightjs--mdbook) | `.js` | mdbook, any highlight.js site |
| [Pygments Lexer](#pygments--chroma--hugo) | `.py` | Pygments, Chroma, Hugo |

## Tree-sitter Highlight Queries

The tree-sitter grammar (`extras/tree-sitter-wcl/`) provides the most accurate highlighting since it uses a full parse tree rather than regex heuristics. The query files are:

| File | Location | Purpose |
|------|----------|---------|
| `highlights.scm` | `extras/tree-sitter-wcl/queries/` | Core syntax highlighting |
| `locals.scm` | `extras/highlight-queries/` | Scope-aware variable highlighting |
| `textobjects.scm` | `extras/highlight-queries/` | Structural text objects (select block, function, etc.) |
| `injections.scm` | `extras/highlight-queries/` | String interpolation injection |

### Neovim

Register the WCL filetype and copy the query files:

```bash
mkdir -p ~/.config/nvim/queries/wcl
cp extras/tree-sitter-wcl/queries/highlights.scm ~/.config/nvim/queries/wcl/
cp extras/highlight-queries/*.scm ~/.config/nvim/queries/wcl/
```

Add filetype detection in your `init.lua`:

```lua
vim.filetype.add({ extension = { wcl = "wcl" } })
```

If you use [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter), you can register WCL as a custom parser. The query files above will then provide highlighting, text objects (with `nvim-treesitter-textobjects`), and scope-aware local variable highlights automatically.

### Helix

Copy the query files into the Helix runtime directory:

```bash
mkdir -p ~/.config/helix/runtime/queries/wcl
cp extras/tree-sitter-wcl/queries/highlights.scm ~/.config/helix/runtime/queries/wcl/
cp extras/highlight-queries/textobjects.scm ~/.config/helix/runtime/queries/wcl/
cp extras/highlight-queries/injections.scm ~/.config/helix/runtime/queries/wcl/
```

Then add the language configuration to `~/.config/helix/languages.toml` (see [Editor Setup](./editor-setup.md#helix) for the full LSP config):

```toml
[[language]]
name = "wcl"
scope = "source.wcl"
file-types = ["wcl"]

[[grammar]]
name = "wcl"
source = { path = "/path/to/extras/tree-sitter-wcl" }
```

### Zed

Zed supports tree-sitter grammars natively. Place the query files in an extension directory:

```
languages/wcl/
  highlights.scm
  injections.scm
```

### GitHub

GitHub uses tree-sitter for syntax highlighting in repositories. Once tree-sitter-wcl is published and registered with [github-linguist](https://github.com/github-linguist/linguist), `.wcl` files will be highlighted automatically on GitHub.

## VS Code / Shiki

The VS Code extension (`editors/vscode/`) includes a TextMate grammar (`wcl.tmLanguage.json`) that provides syntax highlighting. See [Editor Setup](./editor-setup.md#vs-code) for installation.

The TextMate grammar is generated from the canonical Sublime Syntax definition:

```bash
just build vscode-syntax
```

This same `wcl.tmLanguage.json` file can be used with [Shiki](https://shiki.matsu.io/) (used by VitePress, Astro, and other static site generators):

```javascript
import { createHighlighter } from 'shiki';
import wclGrammar from './wcl.tmLanguage.json';

const highlighter = await createHighlighter({
  langs: [
    {
      id: 'wcl',
      scopeName: 'source.wcl',
      ...wclGrammar,
    },
  ],
  themes: ['github-dark'],
});

const html = highlighter.codeToHtml(code, { lang: 'wcl' });
```

## Sublime Text / Syntect / bat

The Sublime Syntax definition (`extras/sublime-syntax/WCL.sublime-syntax`) is the canonical regex-based syntax file. It supports nested block comments, string interpolation, heredocs, and all WCL keywords and types.

### Sublime Text

```bash
cp extras/sublime-syntax/WCL.sublime-syntax \
   ~/.config/sublime-text/Packages/User/
```

### bat

[bat](https://github.com/sharkdp/bat) uses Syntect internally and can load custom `.sublime-syntax` files:

```bash
mkdir -p "$(bat --config-dir)/syntaxes"
cp extras/sublime-syntax/WCL.sublime-syntax "$(bat --config-dir)/syntaxes/"
bat cache --build
```

Then `bat file.wcl` will use WCL highlighting.

### Syntect (Rust)

If you're building a Rust tool that uses [Syntect](https://github.com/trishume/syntect) for highlighting, load the syntax definition at build time:

```rust
use syntect::parsing::SyntaxSet;

let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
builder.add_from_folder("path/to/extras/sublime-syntax/", true)?;
let ss = builder.build();
```

## highlight.js / mdbook

The highlight.js grammar (`extras/highlightjs/wcl.js`) provides syntax highlighting for any site using [highlight.js](https://highlightjs.org/), including [mdbook](https://rust-lang.github.io/mdBook/) (which uses highlight.js by default).

### mdbook

To add WCL highlighting to an mdbook project:

1. Copy the grammar file:

```bash
mkdir -p docs/book/theme
cp extras/highlightjs/wcl.js docs/book/theme/
```

2. Register it in `book.toml`:

```toml
[output.html]
additional-js = ["theme/wcl.js"]
```

3. Add a registration snippet. Create `theme/highlight-wcl.js`:

```javascript
if (typeof hljs !== 'undefined') {
  hljs.registerLanguage('wcl', function(hljs) {
    // The module exports a default function
    return wcl(hljs);
  });
}
```

And add it to `additional-js`:

```toml
[output.html]
additional-js = ["theme/wcl.js", "theme/highlight-wcl.js"]
```

4. Use `wcl` as the language in fenced code blocks:

````markdown
```wcl
server web-prod {
    host = "0.0.0.0"
    port = 8080
}
```
````

### Standalone highlight.js

```javascript
import hljs from 'highlight.js/lib/core';
import wcl from './wcl.js';

hljs.registerLanguage('wcl', wcl);
hljs.highlightAll();
```

## Pygments / Chroma / Hugo

The Pygments lexer (`extras/pygments/wcl_lexer.py`) provides syntax highlighting for Python-based tools and can be converted for use with Go-based tools.

### Pygments

Use directly with the `-x` flag:

```bash
pygmentize -l extras/pygments/wcl_lexer.py:WclLexer -x -f html input.wcl
```

Or install as a plugin by adding to your package's entry points:

```python
# In setup.py or pyproject.toml
[project.entry-points."pygments.lexers"]
wcl = "wcl_lexer:WclLexer"
```

Then `pygmentize -l wcl input.wcl` works directly.

### Chroma / Hugo

[Chroma](https://github.com/alecthomas/chroma) is the Go syntax highlighter used by Hugo. Chroma can import Pygments-style lexers. To add WCL support to a Hugo site:

1. Convert the Pygments lexer to a Chroma Go implementation (see the [Chroma contributing guide](https://github.com/alecthomas/chroma#adding-lexers))
2. Or use the `chroma` CLI to test directly:

```bash
chroma --lexer pygments --filename input.wcl < input.wcl
```

## What's Highlighted

All highlighting definitions cover the same WCL syntax elements:

| Element | Examples |
|---------|----------|
| **Keywords** | `if`, `else`, `for`, `in`, `let`, `macro`, `schema`, `table`, `import`, `export` |
| **Declaration keywords** | `declare`, `validation`, `decorator_schema`, `partial` |
| **Transform keywords** | `inject`, `set`, `remove`, `when`, `check`, `message`, `target` |
| **Built-in types** | `string`, `i64`, `f64`, `bool`, `any`, `identifier`, `list`, `map`, `set`, `union`, `ref` |
| **Built-in functions** | `query`, `has`, `import_table`, `import_raw` |
| **Constants** | `true`, `false`, `null` |
| **Numbers** | Integers, floats, hex (`0xFF`), octal (`0o77`), binary (`0b101`) |
| **Strings** | Double-quoted with `${interpolation}` and `\escape` sequences |
| **Heredocs** | `<<EOF ... EOF` |
| **Decorators** | `@optional`, `@deprecated(reason = "...")` |
| **Comments** | `//`, `/* */`, `///` (doc comments) |
| **Operators** | `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `&&`, `||`, `=>`, `->`, `|`, `?:` |

The tree-sitter queries additionally provide context-aware highlighting (distinguishing block types from identifiers, function calls from variables, parameters from local bindings, etc.) which regex-based highlighters cannot fully replicate.
