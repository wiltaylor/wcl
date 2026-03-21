# WCL Pygments Lexer

Syntax highlighting lexer for [Pygments](https://pygments.org/), the Python syntax highlighter.

Also usable as a source for [Chroma](https://github.com/alecthomas/chroma) (Go, used by Hugo).

## Usage with Pygments

```bash
# Highlight a file
pygmentize -l wcl_lexer.py:WclLexer -x -f html input.wcl

# Generate CSS
pygmentize -S monokai -f html > wcl.css
```

## Installing as a Pygments Plugin

Add to your `setup.py` or `pyproject.toml`:

```python
entry_points={
    'pygments.lexers': ['wcl = wcl_lexer:WclLexer'],
}
```

Then `pygmentize -l wcl input.wcl` works directly.

## Converting to Chroma (for Hugo)

Chroma can import Pygments lexers. See the [Chroma contributing guide](https://github.com/alecthomas/chroma#adding-lexers) for converting this lexer to a Go implementation.
