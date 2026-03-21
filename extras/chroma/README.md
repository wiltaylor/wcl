# WCL Chroma Lexer

Chroma XML lexer definition for WCL, for use with [Chroma](https://github.com/alecthomas/chroma) and Go tools that embed it.

## Status

Hugo uses Chroma internally for syntax highlighting, but does **not** support loading custom XML lexers from the filesystem — lexers must be compiled into the Chroma binary. Until WCL is upstreamed to Chroma, Hugo sites should use the [highlight.js grammar](../highlightjs/) for client-side WCL highlighting.

This XML lexer is provided for:
- Upstreaming to the Chroma project (once WCL is mature enough)
- Custom Hugo builds with WCL support compiled in
- Any Go tool that uses `chroma.NewXMLLexer()` to load lexers at runtime

## Usage with Chroma (Go)

```go
import "github.com/alecthomas/chroma/v2"

// Load from embedded XML
lexer, err := chroma.NewXMLLexer(wcl_xml_bytes)
```

## Upstreaming

To add WCL to Chroma permanently (so all Hugo/Goldmark sites get it for free), submit this lexer to the [Chroma repository](https://github.com/alecthomas/chroma). Place it in `lexers/embedded/` as `wcl.xml`.
