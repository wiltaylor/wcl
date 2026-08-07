# Introduction and quick start

WCL is a typed configuration and schema language. A `.wcl` file carries **both** the data and
the schema that describes it, so a configuration can be checked before anything consumes it.
`wdoc` is a document generator built on top of the language: you declare pages and sites as WCL
blocks and render them to a static website, to Markdown, or to a PDF.

Read this file once. After it, open only the reference that matches your question.

## Why not JSON, YAML or TOML

Those formats give you untyped keys and values. A consumer must do all of the validation, and
it must do it again in each language that reads the file.

WCL adds four things they do not have:

1. **Types.** Numbers carry a width (`u16`, `i64`, `f64`), strings carry an encoding (`utf8`),
   and there are unions, interfaces, optionals, symbol sets and reference types.
2. **A schema in the file.** Decorators such as `@block`, `@document`, `@children` and
   `@default` declare which blocks and fields are legal. `wcl check` enforces it.
3. **Expressions.** Fields hold arithmetic, string interpolation, `match`, `if`, `let`
   bindings, function literals and calls to a builtin library — not just literals.
4. **A document model.** Blocks nest, carry labels, and are *gathered* into typed lists on a
   root `@document` type. That is what lets a tool read a whole tree of files as one value.

The cost is that WCL is not a data interchange format. It is a language you evaluate. Use
`wcl get --json` when you need plain data out of it.

## Install

WCL is pre-release only for now, so install the newest pre-release with the install script:

```console
curl -fsSL https://wcl.dev/install.sh | sh -s -- --pre
```

On a platform with no prebuilt binary, build from source with Cargo instead:

```console
cargo install --git https://github.com/wiltaylor/wcl -p wcl --locked
```

The script installs into `~/.local/bin` by default (`--bin-dir <dir>` or `WCL_INSTALL_DIR`
changes it). Add that directory to `PATH` if it is not there already, then confirm the install:

```console
$ wcl --version
wcl 0.33.2-alpha
```

## A first document

Save this as `config.wcl`. It declares one block type, gathers instances of it on the document
type, and then writes one instance.

```wcl
@block("server") type Server {
  @inline(0) id: identifier
  host: utf8
  @default(8080) port: u16
}

@document type Config {
  @children("server") servers: list<Server>
}

server web {
  host = "localhost"
}
```

Line by line:

- `@block("server")` says that instances of `Server` are written with the keyword `server`.
- `@inline(0)` puts the `id` field in the block's first **label** position — the `web` in
  `server web { … }` — instead of inside the braces.
- `@default(8080)` makes `port` optional. An instance that omits it still has a `port`.
- `@document` marks the root type. It is what the file as a whole is checked against.
- `@children("server")` gathers every top-level `server` block into the `servers` field.

## Check it

`wcl check` validates the document against its own schema:

```console
$ wcl check config.wcl
OK
```

Break it — change `host = "localhost"` to `host = 3` — and the type is what catches it:

```console
$ wcl check config.wcl
wcl::eval::schema_violation

  × field 'host' declared as utf8 but value is i64

config.wcl: 1 schema violation
```

## Read values out

`wcl eval` (aliased `wcl get`) resolves a dotted path from the document root. The path walks
gathered lists by label, so `servers.web` is the block you wrote:

```console
$ wcl get config.wcl servers.web.host
"localhost"
$ wcl get config.wcl servers.web.port
8080
```

`port` answers `8080` even though the instance never sets it: the `@default` is part of the
evaluated view, not a fallback the reader has to know about. Add `--json` for machine output.

A path must end at a **leaf** value. `wcl get config.wcl servers` fails with `not_a_leaf`,
because a gathered block list is not a scalar.

## Write values back

`wcl set` edits one leaf field in place. The value is a WCL **expression**, so a string needs
its quotes to survive the shell:

```console
$ wcl set config.wcl servers.web.host '"example.com"'
updated servers.web.host in config.wcl
```

The edit goes through the same validation as `wcl check`: a write that violates the schema is
rolled back rather than saved. Comments and blank-line groupings in the file survive it.

Two neighbours of `set`:

- `wcl fmt <file>` prints the file in canonical form; `--in-place` overwrites it.
- `wcl diff <a> <b>` compares two documents *after evaluation*, so a re-format produces no
  diff. It reports changed entities and fields:

  ```console
  $ wcl diff config.wcl edited.wcl
  # wcl diff config.wcl -> edited.wcl — generated

  modified "server:web" {
    field "host" {
      kind = :changed
      old = "localhost"
      new = "example.com"
    }
  }
  ```

## The wdoc half

wdoc is a document generator whose vocabulary is ordinary WCL blocks. A document opts in by
importing the embedded standard library, and from then on `site`, `page`, `h1`, `p`, `code`,
`table`, `diagram` and the rest are in scope.

Save this as `site.wcl`:

```wcl
import <wdoc.wcl>

site handbook {
  title = "Handbook"

  toc {
    chapter "Getting started" { page = intro }
  }
}

page intro {
  start = true
  title = "Getting started"

  h1 "Getting started"

  p "wdoc renders this document to a static site."
}
```

Then render it:

```console
$ wcl wdoc build site.wcl --out _site
wrote 1 page
$ ls _site
_wdoc  index.html  intro.html
```

`wcl wdoc serve site.wcl` runs the same build behind a local dev server that watches for
changes. Other targets render the same document elsewhere: `wcl wdoc markdown` for a folder of
`.md` files, `wcl wdoc pdf` for a paginated PDF.

Note what did **not** happen: no template language, no front matter, no second syntax. A page
is a block, its heading is a block, and both are checked by the same schema machinery that
checked `server web` above. That is the whole relationship between the two halves — wdoc is a
schema written in WCL, and `wdoc` is a subcommand of the `wcl` CLI.

## Where to go next

The language references are grouped under `language/` and the wdoc ones under `wdoc/`.
[`SKILL.md`](../SKILL.md) indexes all of them with a one-line hook each; open the one or two
that match the question.

Three starting points cover most first questions:

- [`language/lang_documents.md`](language/lang_documents.md) — fields, blocks, labels and nesting in depth.
- [`language/lang_schemas.md`](language/lang_schemas.md) — the decorators that declare a document shape.
- [`wdoc/wdoc_sites.md`](wdoc/wdoc_sites.md) — the entry document, `site`, `page` and the output tree.
