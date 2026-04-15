---
name: wcl
description: Authoring and consuming WCL, wdoc, and wdoc drawings. Activate when editing *.wcl files, writing schemas or macros, using the wcl CLI, building wdoc documentation or inline SVG diagrams, or integrating WCL from a host language (Rust, Python, JavaScript/TypeScript, Go, .NET, JVM, Ruby, C/C++, Zig).
user-invocable: true
argument-hint: [topic]
allowed-tools:
  - Read
  - Glob
  - Grep
  - Bash
  - Edit
  - Write
---

<overview>
WCL (Wil's Configuration Language) is a typed configuration language with schemas, macros, queries, tables, and an 11-phase pipeline. wdoc is a documentation format built on WCL; wdoc drawings are inline SVG diagrams authored as WCL blocks. This skill gives Claude authoritative references for the language surface, the `wcl` CLI, the wdoc/drawing system, and every host-language binding — loaded on demand so only the relevant slice lands in context.
</overview>

<when-to-activate>
- Editing, creating, validating, or formatting `*.wcl` files.
- Writing schemas, decorators, macros, validation blocks, queries, or tables.
- Authoring wdoc pages, layouts, content elements, or drawings.
- Running or explaining any `wcl` CLI subcommand.
- Consuming WCL from a host language (parsing, evaluating, reading values).
- Diagnosing WCL error codes (`Enn`, `Wnn`).
</when-to-activate>

<routing>
Read the matching reference file(s) **before** writing or recommending code. Do not load the full reference tree — only what the task needs.

**Authoring WCL / wdoc:**

| Task | Read |
|------|------|
| Blocks, literals, strings, let, imports, namespaces, control flow, macros, queries, tables | `${CLAUDE_SKILL_DIR}/reference/syntax.md` |
| Schema definitions, field types, any `@decorator` | `${CLAUDE_SKILL_DIR}/reference/schemas-and-decorators.md` |
| Any built-in function call (`upper`, `len`, `regex_match`, ...) | `${CLAUDE_SKILL_DIR}/reference/builtin-functions.md` |
| Diagnostic `Enn` / `Wnn` codes | `${CLAUDE_SKILL_DIR}/reference/error-codes.md` |
| `wcl validate/fmt/eval/lsp/set/add/remove/table/docs/transform/wdoc` | `${CLAUDE_SKILL_DIR}/reference/cli.md` |
| wdoc: `doc`, `page`, `section`, `layout`, content elements, styles | `${CLAUDE_SKILL_DIR}/reference/wdoc.md` |
| wdoc drawings: `diagram`, shapes, widgets, connections, layout algorithms | `${CLAUDE_SKILL_DIR}/reference/wdoc-drawings.md` |
| Looking for a canonical example | `${CLAUDE_SKILL_DIR}/reference/examples.md` |

**Consuming WCL from a host language** — pick the file that matches the detected host. If the host is unclear, read `overview.md` first.

| Host | Read |
|------|------|
| (unsure) | `${CLAUDE_SKILL_DIR}/reference/bindings/overview.md` |
| Rust (`Cargo.toml`, `wcl_lang` dep) | `${CLAUDE_SKILL_DIR}/reference/bindings/rust.md` |
| Python (`pyproject.toml`, `pywcl`) | `${CLAUDE_SKILL_DIR}/reference/bindings/python.md` |
| JavaScript / TypeScript (`package.json`, `wcl_wasm`) | `${CLAUDE_SKILL_DIR}/reference/bindings/javascript.md` |
| Go (`go.mod`) | `${CLAUDE_SKILL_DIR}/reference/bindings/go.md` |
| .NET / C# (`*.csproj`, `WclLang`) | `${CLAUDE_SKILL_DIR}/reference/bindings/dotnet.md` |
| JVM / Java / Kotlin (`pom.xml`, `build.gradle`) | `${CLAUDE_SKILL_DIR}/reference/bindings/jvm.md` |
| Ruby (`Gemfile`, `*.gemspec`) | `${CLAUDE_SKILL_DIR}/reference/bindings/ruby.md` |
| C / C++ (`wcl.h`, `libwcl`) | `${CLAUDE_SKILL_DIR}/reference/bindings/c.md` |
| Zig (`build.zig.zon`) | `${CLAUDE_SKILL_DIR}/reference/bindings/zig.md` |
</routing>

<boundaries>
<always>
- Read the matching reference file before authoring syntax, decorators, functions, CLI flags, or binding code.
- If a decorator / function / error code is missing from the reference, check the authoritative source listed in `reference/sync.md` — it may be newly added (and the reference needs updating).
- Cite `file:line` for any claim that affects user code.
</always>

<never>
- Load all reference files at once — route to the specific file(s) the task needs.
- Invent decorators, built-in functions, error codes, CLI flags, or binding APIs.
- Use forms that were removed from the language: `label`/`labels` on blocks, `KindLabel` / `kind-label` selectors, kind+string-label paths (inline args and qualified IDs replaced these).
- Load multiple `bindings/*.md` when only one host language is in use.
</never>
</boundaries>
