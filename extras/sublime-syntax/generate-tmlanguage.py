#!/usr/bin/env python3
"""
Convert WCL.sublime-syntax to wcl.tmLanguage.json for VS Code / Shiki.

Sublime Text syntax and TextMate grammars share the same regex-based model
but use different formats (YAML vs JSON plist). This script translates the
subset of features WCL uses into a tmLanguage.json that VS Code can consume.

Usage:
    python3 generate-tmlanguage.py > ../../editors/vscode/syntaxes/wcl.tmLanguage.json

Alternatively, use the just recipe: just build vscode-syntax
"""

import json
import sys

# This is a hand-maintained translation from the sublime-syntax.
# If the sublime-syntax changes, update this file accordingly.
# A full automated converter exists (sublime-syntax-convertor) but for
# our relatively simple grammar, a direct JSON definition is clearer.

tmlanguage = {
    "$schema": "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
    "name": "WCL",
    "scopeName": "source.wcl",
    "patterns": [
        {"include": "#comments"},
        {"include": "#strings"},
        {"include": "#heredocs"},
        {"include": "#numbers"},
        {"include": "#constants"},
        {"include": "#decorators"},
        {"include": "#keywords"},
        {"include": "#types"},
        {"include": "#functions"},
        {"include": "#operators"},
        {"include": "#identifiers"},
    ],
    "repository": {
        "comments": {
            "patterns": [
                {"name": "comment.line.documentation.wcl", "match": "///.*$"},
                {"name": "comment.line.double-slash.wcl", "match": "//.*$"},
                {
                    "name": "comment.block.wcl",
                    "begin": "/\\*",
                    "end": "\\*/",
                    "patterns": [
                        {
                            "name": "comment.block.wcl",
                            "begin": "/\\*",
                            "end": "\\*/",
                        }
                    ],
                },
            ]
        },
        "strings": {
            "patterns": [
                {
                    "name": "string.quoted.double.wcl",
                    "begin": '"',
                    "end": '"',
                    "patterns": [
                        {
                            "name": "constant.character.escape.wcl",
                            "match": '\\\\["\\\\/nrt]',
                        },
                        {
                            "name": "constant.character.escape.unicode.wcl",
                            "match": "\\\\u[0-9a-fA-F]{4}",
                        },
                        {
                            "name": "constant.character.escape.unicode.wcl",
                            "match": "\\\\U[0-9a-fA-F]{8}",
                        },
                        {
                            "name": "meta.embedded.expression.wcl",
                            "begin": "\\$\\{",
                            "end": "\\}",
                            "beginCaptures": {
                                "0": {
                                    "name": "punctuation.definition.interpolation.begin.wcl"
                                }
                            },
                            "endCaptures": {
                                "0": {
                                    "name": "punctuation.definition.interpolation.end.wcl"
                                }
                            },
                            "patterns": [{"include": "source.wcl"}],
                        },
                    ],
                }
            ]
        },
        "heredocs": {
            "patterns": [
                {
                    "name": "string.unquoted.heredoc.wcl",
                    "begin": "<<-?'?([a-zA-Z_]\\w*)'?",
                    "end": "^\\1\\s*$",
                }
            ]
        },
        "numbers": {
            "patterns": [
                {
                    "name": "constant.numeric.float.wcl",
                    "match": "\\b\\d+\\.\\d+([eE][+-]?\\d+)?\\b",
                },
                {
                    "name": "constant.numeric.hex.wcl",
                    "match": "\\b0[xX][0-9a-fA-F][0-9a-fA-F_]*\\b",
                },
                {
                    "name": "constant.numeric.octal.wcl",
                    "match": "\\b0[oO][0-7][0-7_]*\\b",
                },
                {
                    "name": "constant.numeric.binary.wcl",
                    "match": "\\b0[bB][01][01_]*\\b",
                },
                {
                    "name": "constant.numeric.integer.wcl",
                    "match": "\\b\\d[0-9_]*\\b",
                },
            ]
        },
        "constants": {
            "patterns": [
                {
                    "name": "constant.language.boolean.wcl",
                    "match": "\\b(true|false)\\b",
                },
                {"name": "constant.language.null.wcl", "match": "\\bnull\\b"},
            ]
        },
        "decorators": {
            "patterns": [
                {
                    "match": "(@)([a-zA-Z_][a-zA-Z0-9_]*)",
                    "captures": {
                        "1": {"name": "punctuation.definition.annotation.wcl"},
                        "2": {"name": "entity.name.function.decorator.wcl"},
                    },
                }
            ]
        },
        "keywords": {
            "patterns": [
                {
                    "name": "keyword.control.wcl",
                    "match": "\\b(if|else|for|in|when)\\b",
                },
                {
                    "name": "keyword.declaration.wcl",
                    "match": "\\b(let|partial|macro|schema|table|validation|decorator_schema|declare)\\b",
                },
                {
                    "name": "keyword.control.import.wcl",
                    "match": "\\b(import|export)\\b",
                },
                {
                    "name": "keyword.other.wcl",
                    "match": "\\b(inject|set|remove|check|message|target)\\b",
                },
                {
                    "name": "support.function.builtin.wcl",
                    "match": "\\b(query|has|import_table|import_raw)\\b",
                },
            ]
        },
        "types": {
            "patterns": [
                {
                    "name": "support.type.builtin.wcl",
                    "match": "\\b(string|int|float|bool|identifier|any)\\b(?!\\s*[=(])",
                },
                {
                    "name": "support.type.builtin.wcl",
                    "match": "\\b(list|map|set|union|ref)\\b(?=\\s*\\()",
                },
            ]
        },
        "functions": {
            "patterns": [
                {
                    "name": "entity.name.function.wcl",
                    "match": "\\b[a-zA-Z_][a-zA-Z0-9_]*(?=\\s*\\()",
                }
            ]
        },
        "operators": {
            "patterns": [
                {
                    "name": "keyword.operator.arrow.wcl",
                    "match": "=>|->",
                },
                {
                    "name": "keyword.operator.comparison.wcl",
                    "match": "==|!=|<=|>=|<|>|=~",
                },
                {
                    "name": "keyword.operator.logical.wcl",
                    "match": "&&|\\|\\||!",
                },
                {
                    "name": "keyword.operator.arithmetic.wcl",
                    "match": "[+\\-*/%]",
                },
                {
                    "name": "keyword.operator.assignment.wcl",
                    "match": "(?<![=!<>])=(?!=)",
                },
                {
                    "name": "keyword.operator.pipe.wcl",
                    "match": "\\|",
                },
                {
                    "name": "keyword.operator.ternary.wcl",
                    "match": "[?:]",
                },
            ]
        },
        "identifiers": {
            "patterns": [
                {
                    "name": "variable.other.wcl",
                    "match": "\\b[a-zA-Z_][a-zA-Z0-9_-]*\\b",
                }
            ]
        },
    },
}


def main():
    json.dump(tmlanguage, sys.stdout, indent=2)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
