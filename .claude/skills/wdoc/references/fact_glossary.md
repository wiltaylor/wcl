# Glossary

| Term | Definition |
| --- | --- |
| **backend** | One of wdoc's output engines — HTML (`build`/`serve`), Markdown (`markdown`, and `skill` on top of it), or PDF (`pdf`) — all rendering the same document model. |
| **block** | The unit of wdoc content — a typed WCL block (`p`, `h1`, `table`, `diagram`, …) declared inside a page or another block, validated against its schema type. |
| **component** | A `wdoc_component`: a reusable fragment of wdoc markup with named slots, instantiated by its own name anywhere a block is allowed. |
| **frontmatter** | A schemaless `frontmatter` block of `key = value` entries serialized as a YAML header on a page's Markdown output; the HTML and PDF targets ignore it. |
| **fundamental** | A primitive the renderer emits directly — an HTML, SVG, or terminal element — the fixed vocabulary every block ultimately lowers to. |
| **lowering** _(also: lower)_ | The recursive expansion of a high-level block into simpler blocks via its `<kind>_lower` function, repeated until only fundamentals remain for the renderer. |
| **native** | A block wdoc renders in Rust rather than by lowering, marked `@native` on its type — and `@native(backends = …)` names the targets that implement it. A block declares a lowering or `@native`, never both and never neither; using one on a target it doesn't cover is a build error, waived per instance with `@except(backends = …)`. |
| **page** | A `page` block: one rendered output page — an id, a title, the sites it joins via `sites`, and the content blocks that make up its body. |
| **projection** | Rendering document content from a typed data model rather than hand-authored blocks — components, repeaters, and `body`/`project` derive the pages from the data. |
| **repeater** | A `wdoc_repeater`: renders its body once per element of `each`, binding the element to the symbol named by `as` — the single iteration concept for blocks, pages, and nav entries. |
| **site** | A `site` block configuring one output target — its template, title, theme, and navigation; a document can declare several, each rendering into its own subdirectory. |
| **slot** | A `wdoc_slot` inside a component: a named, optionally-defaulted parameter filled at instantiation and referenced in the body via interpolation or bare identifiers. |
| **template** | The shape a site's pages render into — `:webpage`, `:book`, `:presentation`, `:ai_skill`, or a custom `template` block mapping a TemplateCtx to HTML fundamentals. |
| **theme** | A site-wide palette selected with `theme = :<name>` — seven ship (forge, nord, tokyonight, gruvbox, catppuccin, rose, paper), each with dark and light variants. |

[← Back to SKILL.md](../SKILL.md)
