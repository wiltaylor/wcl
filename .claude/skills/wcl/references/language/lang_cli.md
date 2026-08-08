# The CLI

`wcl` is one binary with thirteen commands. Nine read or rewrite a document — `parse`, `check`,
`eval` (aliased `get`), `set`, `fmt`, `diff`, `init`, `repl` and `lsp`. Four sit under
`wcl wdoc` and render a document to a website, a dev server, a PDF or a folder of Markdown.
This file covers all of them: what each one takes, every flag it accepts, and what it prints.

## One binary, one document model

Each command names a `<file>`. That file is the **entry document**: `wcl` parses it, follows
its imports, and works on the result. Nothing is configured outside the file — no project
file, no lockfile, no `wcl.toml`.

Every command opens a document the same way. Disk imports resolve relative to the importing
file, and system imports (`import <wdoc.wcl>`) resolve against the embedded wdoc standard
library. The CLI does **not** read a document's imports to decide whether it is "a wdoc
document": every open carries the wdoc registry and environment. So `wcl check` validates a
wdoc document as thoroughly as `wcl wdoc build` does.

Two flags work before any command: `--version` and `--help`. `wcl <command> --help` prints
the detail for one command.

## Exit codes

Five codes, meaning the same thing in every command.

| Code | Name | Means |
| --- | --- | --- |
| 0 | OK | The command did what it was asked. |
| 1 | Parse | The source did not parse. Also reported when the entry file cannot be read at all. |
| 2 | Schema | The document parsed, but it violates its own schema. |
| 3 | Eval | Evaluation failed — an unresolved name, a type mismatch, a path resolving to nothing, a refused block. |
| 4 | I/O | The document was good; writing the output, copying an asset or reaching the disk was not. |

Diagnostics go to **stderr**, results to **stdout**.

## `wcl parse`

```console
$ wcl parse <file> [--profile]
```

| Argument | Meaning |
| --- | --- |
| `<file>` | Path to a WCL source file. |
| `--profile` | Record a call-tree profile of the forcing and print it as JSON on stderr after the dump. |

Parses, forces every field, and prints the evaluated document tree — what the document
*became*, not what you typed.

```console
$ wcl parse inventory.wcl
...
owner = "platform"
service web {
  port = 8080u32
  region = "us-east-1"
}
```

`parse` prints **everything in scope**, including the language's own decorator declarations
(`@block`, `@children`, `@inline`, …) and the `std` unit types. Your items follow them in
source order; pipe through `tail` when you want only the data.

## `wcl check`

```console
$ wcl check <file> [--json]
```

| Argument | Meaning |
| --- | --- |
| `<file>` | Path to a WCL source file, or `-` to read stdin (relative imports then resolve against the current directory). |
| `--json` | Emit a JSON result object on stdout instead of diagnostics. Exit codes do not change. |

Runs the parser **and** the schema validator. On success it prints `OK`.

```console
$ wcl check sch.wcl
wcl::eval::schema_violation

  × top-level field 'zone' is not declared by @document schema 'C'

sch.wcl: 1 schema violation
$ echo $?
2
```

`--json` always carries the same four keys, so a consumer never branches on success:

```console
$ wcl check sch.wcl --json
{
  "errors": [
    {
      "code": "wcl::eval::schema_violation",
      "length": 10,
      "message": "top-level field 'zone' is not declared by @document schema 'C'",
      "offset": 45
    }
  ],
  "file": "sch.wcl",
  "ok": false,
  "warnings": []
}
```

`offset` and `length` are byte positions into the file. `check` is the one command that reads
stdin, so `cat generated.wcl | wcl check -` validates a document never written to disk.

## `wcl eval` and `wcl get`

`get` is an alias for `eval`. Same command.

```console
$ wcl get  <file> <path> [--json] [--profile]
$ wcl eval <file> <path> [--json] [--profile]
```

| Argument | Meaning |
| --- | --- |
| `<file>` | Path to a WCL source file. |
| `<path>` | Dotted path, resolved from the document root. |
| `--json` | Print the value as JSON instead of the WCL display form. A function value becomes `null`. |
| `--profile` | Record a call-tree profile of this evaluation and print it as JSON on stderr. |

A path walks the tree `wcl parse` prints: a field, a gather field, a block label, a nested
block, a field on it.

```console
$ wcl get inventory.wcl owner
"platform"
$ wcl get inventory.wcl services.web.port
8080u32
```

The output is the **WCL display form**, not JSON: a string keeps its quotes and a number keeps
its type suffix. Only what the document model exposes is addressable. A `let` item is not, since
it is not document data, and a table row is not, since it has no name and no label.

A path that resolves to nothing exits 3 and suggests a near name:

```console
$ wcl get inventory.wcl services.web.prot
no such path: services.web.prot
did you mean: service?
```

A path that resolves to a **block** is an error, because a block is not a leaf:

```console
$ wcl get inventory.wcl services.web --json
wcl::eval::not_a_leaf

  × cannot evaluate block as a leaf value
```

`--json` is the form to pipe:

```console
$ wcl get inventory.wcl services.web.port --json
8080
```

JSON drops the `u32` suffix and the unit on a value like `512MiB` (already the number
536870912 by then). Read the WCL form when the type matters; the JSON when the number does.

Three commands answer three questions: `check` — *is this valid?*; `parse` — *what is in it?*;
`get` — *what is this one value?* `--profile` on `parse` or `get` answers *why is it slow?*

## `wcl set`

```console
$ wcl set <file> <path> <value>
```

| Argument | Meaning |
| --- | --- |
| `<file>` | Entry document. Imports are followed. |
| `<path>` | Dotted path to the field whose value is replaced. |
| `<value>` | The new value, written as a WCL **expression**. |

```console
$ wcl set inventory.wcl services.web.port 9090u32
updated services.web.port in inventory.wcl
```

`set` is the edit path, not the evaluation path: it works on the syntax tree, so comments,
blank-line groupings and layout all survive. The value is an expression, so the shell and WCL
both want a say in the quoting:

```console
$ wcl set inventory.wcl owner '"infrastructure"'
$ wcl set inventory.wcl services.web.port 9090u32
$ wcl set site.wcl accent :gold
$ wcl set site.wcl tags '[:a, :b]'
```

Two behaviours to know before scripting it:

- **`set` follows imports.** When `<path>` resolves through an import, `set` edits the file
  that *declares* the field, which need not be the file you named. The message says which.
- **`set` does not validate.** It re-parses its own output before writing atomically, so it
  cannot leave a broken file behind. It does not run the schema. A `set` that violates a `@min`,
  or writes a `utf8` where a `u32` is declared, is written anyway. Follow a scripted `set` with
  `wcl check`.

A path that names nothing exits 3 and changes no file.

## `wcl fmt`

```console
$ wcl fmt <file> [--in-place] [--indent N] [--no-trailing-comma]
```

| Argument | Default | Meaning |
| --- | --- | --- |
| `<file>` | — | Path to a WCL source file, or `-` to read stdin and write stdout. |
| `--in-place` | off | Overwrite the file, atomically. Without it the formatted source goes to stdout. |
| `--indent N` | 2 | Spaces per indentation level. |
| `--no-trailing-comma` | off | Drop the trailing comma after every `match` arm. The parser accepts either form. |

Indentation, brace style, number radix, string-delimiter choice and spacing are normalized.
Comments survive, blank-line groupings survive up to one blank line, and item order is never
touched. The one authored choice it rewrites is the comment marker: `//` becomes `#`.

```console
$ cat ugly.wcl
owner="x"
@document
type C { owner: utf8 }

$ wcl fmt ugly.wcl
owner = "x"
@document
type C {
  owner: utf8
}
```

### Checking format in CI

There is no `--check` flag. Compare the output against the file:

```console
$ wcl fmt config.wcl | diff -u config.wcl - && echo "formatted"
formatted
```

`diff` exits non-zero on any difference, so the `&&` chain fails the job and the unified diff
shows what `--in-place` would have done. Across a tree:

```console
$ find . -name '*.wcl' -exec sh -c 'wcl fmt "$1" | diff -u "$1" -' _ {} \;
```

Run `wcl check` in the same job. `fmt` parses; it does not evaluate and does not run the
schema, so a gate that runs only `fmt` proves nothing about the data.

## `wcl diff`

```console
$ wcl diff <old> <new> [--format wcl|json]
```

| Argument | Meaning |
| --- | --- |
| `<old>` | The base document — a path, or a `<rev>:<path>` git specifier. |
| `<new>` | The new document — a path, or a `<rev>:<path>` git specifier. |
| `--format` | `wcl` (default) prints a re-parseable WCL tree; `json` prints the flat change array. |

Not `diff(1)`. It compares the **evaluated** documents with imports resolved, so it reports
changes in the data rather than in the text. Each top-level block is an entity keyed
`kind:label`; nested field edits are reported by path, recursing into lists by index.

```console
$ wcl diff inventory.wcl inv2.wcl
# wcl diff inventory.wcl -> inv2.wcl — generated

modified "service:web" {
  field "port" {
    kind = :changed
    old = 8080u32
    new = 9090u32
  }
}
```

The default output is itself valid WCL — a change set is a document. `--format json` gives the
same information flat:

```console
$ wcl diff inventory.wcl inv2.wcl --format json
[
  {
    "entity": "service:web",
    "field": "port",
    "kind": "changed",
    "new": 9090,
    "old": 8080,
    "op": "modified"
  }
]
```

A formatting-only edit produces **no diff at all**. Neither does a comment edit, a reordering,
or a rewrite that moves a field into an import.

### Reading a revision out of git

Either side may be `<rev>:<path>`. `wcl` materializes that revision into a temp tree, so the
old document's **imports resolve from that same revision**, not from your working copy.

```console
$ wcl diff HEAD~1:config.wcl config.wcl
$ wcl diff main:a.wcl feature:a.wcl --format json
$ wcl diff v1.2.0:schema/main.wcl schema/main.wcl
```

### Upgrading a document

That form makes `wcl diff` an upgrade tool. When an imported library gains fields, changes a
default or renames a kind, ask what your *data* became:

```console
$ wcl diff v0.4.0:main.wcl main.wcl
```

Both sides evaluate against their own imports. The answer therefore folds in a new default that
now fills a field you never wrote, a computed value that moved because a builtin changed, and a
block that gathers somewhere else now. An empty diff after a library bump is a real result.

The working order is three commands: bump the import; run `wcl check` and fix what no longer
validates; run `wcl diff <old-rev>:main.wcl main.wcl` to see what moved silently. The third
step is the one people skip.

## `wcl init`

```console
$ wcl init <template> [dest] [-D key=value]... [--answers f] [--defaults] [--force]
$ wcl init --list
```

| Argument | Meaning |
| --- | --- |
| `<template>` | A built-in name, a user template name, or a path to a template `.wcl` file (or a folder holding `template.wcl`). Optional with `--list`. |
| `[dest]` | Destination directory. Defaults to the answered `name` property, then the template name. |
| `-D key=value` | Answer one property inline. Repeatable. Highest precedence. |
| `--answers <f>` | Answer file, `.wcl` or `.json`. |
| `--defaults` | Never prompt: use each property's default, and fail on one that has none. |
| `--force` | Write into the destination even if it exists and is not empty. |
| `--list` | List the templates and exit. |

```console
$ wcl init --list
Built-in templates:
  minimal
  page
  book
  website
  presentation

User templates (/home/you/.local/share/wcl/templates):
  (none — add one as <that dir>/<name>/template.wcl)

$ wcl init book ./handbook -D name=handbook --defaults
Created ./handbook from template 'book'
  main.wcl
  schema/main.wcl
  data/main.wcl
  wdoc/main.wcl
```

**Where an answer comes from**, first match wins: `-D key=value`; then `--answers <file>`;
then the interactive prompt (skipped under `--defaults`); then the property's `default` (and a
property with no default, under `--defaults`, is an error).

**Template resolution** has its own order: built-in name, then user template, then disk path.
A built-in name shadows a user template of the same name.

A template is a WCL document declaring `property` blocks for the questions and `file` /
`folder` blocks for what to generate. Two rules catch every template author. First, a generated
file's content must use the **interpolating** heredoc `$<<TAG` — a plain `<<TAG` is literal, so
`${answer("name")}` lands verbatim. Second, a `property` instance sets its fields with `=`
(`prompt = "Project name"`), because it is a block instance and not a `type` declaration.

## `wcl repl`

```console
$ wcl repl [file]
```

With no argument it evaluates self-contained expressions. With a file, identifiers also
resolve against that document's top-level fields.

```console
$ wcl repl inventory.wcl
wcl> owner
"platform"
wcl> len(services)
2
wcl> :quit
```

`:quit` / `:q` or EOF (Ctrl-D) exits. No history, no line editing — it reads lines, which is
what makes it scriptable. A multi-line expression continues while brackets are unbalanced,
and the prompt becomes `... `.

The exit code depends on whether a human was watching. An **interactive** session always exits
0. A **piped** session reports the worst thing that happened — 3 if any evaluation failed,
else 1 if any parse failed.

```console
$ printf '1 +* 2\n' | wcl repl
parse error: wcl::parse

  × expected value, found '*'
$ echo $?
1
```

## `wcl lsp`

```console
$ wcl lsp [--tcp host:port] [--log file]
```

| Argument | Meaning |
| --- | --- |
| `--tcp <addr>` | Listen on `host:port` instead of stdio. Each connection is an independent LSP session. |
| `--log <file>` | Write `tracing` log lines to this file. |

The default transport is **stdio**, which is what an editor expects — you configure your
editor to spawn this, you do not type it. `--log` takes a file and only a file: the server
never logs to stderr, because that would corrupt the stdio LSP stream.

The server provides diagnostics, formatting, document symbols, workspace symbol search,
go-to-definition and find-references across files, hover, completion, signature help, semantic
tokens and schema-violation code actions. An open buffer shadows the copy on disk.

## `wcl wdoc build`

```console
$ wcl wdoc build <file> --out <dir> [--site NAME] [--profile]
```

| Argument | Meaning |
| --- | --- |
| `<file>` | The entry document. |
| `--out <dir>` | Output directory. Created if missing. **Required.** |
| `--site <name>` | Build only this site, flat at `<out>`. Omitted, every site renders into `<out>/<name>/`. |
| `--profile` | Print the evaluation profile as JSON on stderr. |

```console
$ wcl wdoc build main.wcl --out _site
wrote 2 pages
```

## `wcl wdoc serve`

```console
$ wcl wdoc serve <file> [--addr ADDR] [--out DIR] [--site NAME]
```

| Argument | Default | Meaning |
| --- | --- | --- |
| `<file>` | — | The entry document. |
| `--addr` | `127.0.0.1:8080` | Bind address, or `auto` to take the first free port near 8080. |
| `--out` | a temp directory | Output directory. A temp directory is removed on shutdown. |
| `--site` | every site | Serve only this site at `/`. Omitted, each site is served under `/<name>/` with a chooser at `/`. |

```console
$ wcl wdoc serve main.wcl --out _serve
rendered 2 pages
serving http://127.0.0.1:8080  (source: main.wcl, out: _serve)
auto-rebuild is off — press Enter here to rebuild after edits
```

The watcher **accumulates** changes and rebuilds only when asked — Enter in the console, or
`POST /__wdoc_rebuild`.

## `wcl wdoc pdf`

```console
$ wcl wdoc pdf <file> --out <dir> [--site NAME] [--page-size a4|letter]
```

| Argument | Default | Meaning |
| --- | --- | --- |
| `<file>` | — | The entry document. |
| `--out <dir>` | — | Output directory. Created if missing. **Required.** |
| `--site <name>` | every site | Render only this site. With no `site` block, the source file stem names the PDF. |
| `--page-size` | `a4` | `a4` or `letter`. |

```console
$ wcl wdoc pdf main.wcl --out _pdf
wrote 1 pdf
$ ls _pdf
handbook.pdf
```

The output is named after the **site**, not after the source file.

## `wcl wdoc markdown`

`wcl wdoc md` is an alias.

```console
$ wcl wdoc markdown <file> --out <dir> [--site NAME]
```

| Argument | Meaning |
| --- | --- |
| `<file>` | The entry document. |
| `--out <dir>` | Output directory. Created if missing. **Required.** |
| `--site <name>` | Render only this site, flat at `<out>`. Omitted, each site gets its own `<out>/<name>/`. |

```console
$ wcl wdoc markdown main.wcl --out _md
wrote 1 page
$ find _md -type f
_md/index.md
_md/one.md
_md/_wdoc/one-diagram-1.svg
```

One `.md` per page, plus the standalone `.svg` files the Markdown references for diagrams,
terminals and wireframes.

## See also

- [`lang_documents.md`](lang_documents.md) — the tree `parse`, `get` and `set` walk.
- [`lang_evaluation.md`](lang_evaluation.md) — the evaluate-and-edit split, and the error model behind the exit codes.
- [`lang_builtins.md`](lang_builtins.md) — every function a `repl` expression or a `set` value can call.
- [`../wdoc/wdoc_outputs.md`](../wdoc/wdoc_outputs.md) — the four `wcl wdoc` commands in depth.
