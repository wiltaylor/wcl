# Quick Start

_Get WCL running in a few minutes: declare a type, write data, check and evaluate._

## Install

WCL is pre-release only for now, so install the newest pre-release with the install script:

```console
curl -fsSL https://wcl.dev/install.sh | sh -s -- --pre
```

On a platform without a prebuilt binary (e.g. macOS), build from source with Cargo instead:

```console
cargo install --git https://github.com/wiltaylor/wcl -p wcl --locked
```

If `~/.local/bin` is not on your `PATH`, add it. Verify with `wcl --version`.

## A minimal document

Declare a block type, point a `@document` at it, then write an instance:

```wcl
@block("server") type Server {
  @inline(0) id: identifier
  host: utf8
  @default(8080) port: u16
}
@document type Config { @children("server") servers: list<Server> }

server web { host = "localhost" }
```

## Check and evaluate

Validate the document against its schema, then evaluate it:

```console
$ wcl check config.wcl     # type-checks the document, reports errors
$ wcl eval config.wcl      # prints the evaluated data
```

## Related

- [What WCL Is](../references/concept_what_is_wcl.md)
