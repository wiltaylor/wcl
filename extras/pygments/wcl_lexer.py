"""
WCL (Wil's Configuration Language) lexer for Pygments.

Usage with Pygments:
    pygmentize -l wcl_lexer.py:WclLexer -x -f html input.wcl

This lexer can also be converted to a Chroma lexer for Hugo via:
    chroma --convert pygments wcl_lexer.py

Install as a Pygments plugin by adding to entry_points in setup.py:
    'pygments.lexers': ['wcl = wcl_lexer:WclLexer']
"""

from pygments.lexer import RegexLexer, bygroups, include, words
from pygments.token import (
    Comment,
    Keyword,
    Name,
    Number,
    Operator,
    Punctuation,
    String,
    Text,
    Token,
)

__all__ = ["WclLexer"]


class WclLexer(RegexLexer):
    """Lexer for WCL (Wil's Configuration Language)."""

    name = "WCL"
    aliases = ["wcl"]
    filenames = ["*.wcl"]
    mimetypes = ["text/x-wcl"]

    tokens = {
        "root": [
            # Comments
            (r"///.*$", Comment.Special),
            (r"//.*$", Comment.Single),
            (r"/\*", Comment.Multiline, "block_comment"),
            # Whitespace
            (r"\s+", Text.Whitespace),
            # Strings
            (r'"', String.Double, "string"),
            # Heredocs
            (
                r"<<-?'?([a-zA-Z_]\w*)'?",
                String.Heredoc,
                "heredoc",
            ),
            # Numbers (order matters: floats before ints)
            (r"\b\d+\.\d+([eE][+-]?\d+)?\b", Number.Float),
            (r"\b0[xX][0-9a-fA-F][0-9a-fA-F_]*\b", Number.Hex),
            (r"\b0[oO][0-7][0-7_]*\b", Number.Oct),
            (r"\b0[bB][01][01_]*\b", Number.Bin),
            (r"\b\d[0-9_]*\b", Number.Integer),
            # Decorators
            (
                r"(@)([a-zA-Z_]\w*)",
                bygroups(Punctuation, Name.Decorator),
            ),
            # Control flow keywords
            (
                words(("if", "else", "for", "in", "when"), prefix=r"\b", suffix=r"\b"),
                Keyword,
            ),
            # Declaration keywords
            (
                words(
                    (
                        "let",
                        "partial",
                        "macro",
                        "schema",
                        "table",
                        "validation",
                        "decorator_schema",
                        "declare",
                    ),
                    prefix=r"\b",
                    suffix=r"\b",
                ),
                Keyword.Declaration,
            ),
            # Import/export
            (
                words(("import", "export"), prefix=r"\b", suffix=r"\b"),
                Keyword.Namespace,
            ),
            # Other keywords
            (
                words(
                    ("inject", "set", "remove", "check", "message", "target"),
                    prefix=r"\b",
                    suffix=r"\b",
                ),
                Keyword,
            ),
            # Built-in functions / query keywords
            (
                words(
                    ("query", "has", "import_table", "import_raw", "ref"),
                    prefix=r"\b",
                    suffix=r"\b",
                ),
                Name.Builtin,
            ),
            # Boolean / null constants
            (words(("true", "false"), prefix=r"\b", suffix=r"\b"), Keyword.Constant),
            (r"\bnull\b", Keyword.Constant),
            # Type names
            (
                words(
                    ("string", "int", "float", "bool", "identifier", "any"),
                    prefix=r"\b",
                    suffix=r"\b",
                ),
                Keyword.Type,
            ),
            (
                words(("list", "map", "set", "union", "ref"), prefix=r"\b", suffix=r"\b"),
                Keyword.Type,
            ),
            # Function calls (identifier followed by paren)
            (r"\b([a-zA-Z_]\w*)(\s*\()", bygroups(Name.Function, Punctuation)),
            # Identifier literals (contain hyphens)
            (r"\b[a-zA-Z_]\w*-[\w-]*\b", Name.Label),
            # Plain identifiers
            (r"\b[a-zA-Z_]\w*\b", Name.Other),
            # Multi-char operators (before single-char)
            (r"=>|->|==|!=|<=|>=|=~|&&|\|\|", Operator),
            # Single-char operators
            (r"[+\-*/%=!<>|?:]", Operator),
            # Punctuation
            (r"[{}()\[\]]", Punctuation),
            (r"[,.]", Punctuation),
            # Library imports
            (r"<[^>]+>", String.Other),
        ],
        "string": [
            (r'"', String.Double, "#pop"),
            (r"\\[\"\\nrt/]", String.Escape),
            (r"\\u[0-9a-fA-F]{4}", String.Escape),
            (r"\\U[0-9a-fA-F]{8}", String.Escape),
            (r"\$\{", String.Interpol, "interpolation"),
            (r'[^"\\$]+', String.Double),
            (r"\$", String.Double),
        ],
        "interpolation": [
            (r"\}", String.Interpol, "#pop"),
            include("root"),
        ],
        "heredoc": [
            # The heredoc end marker must match the opening marker.
            # Pygments doesn't support backreferences, so we match any
            # identifier at the start of a line as a potential end marker.
            (r"^[a-zA-Z_]\w*\s*$", String.Heredoc, "#pop"),
            (r"[^\n]+", String.Heredoc),
            (r"\n", String.Heredoc),
        ],
        "block_comment": [
            (r"\*/", Comment.Multiline, "#pop"),
            (r"/\*", Comment.Multiline, "#push"),
            (r"[^/*]+", Comment.Multiline),
            (r"[/*]", Comment.Multiline),
        ],
    }
