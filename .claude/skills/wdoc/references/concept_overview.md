# wdoc Overview

_What wdoc is and the page → site → template → render-target model._

`wdoc` is WCL's documentation generator. It supports different types of pages and is designed to present information held in `.wcl` files in a human-readable format. One model projects to multiple render targets: HTML sites, a Markdown folder, PDFs, and Claude / agent skill folders.


The shape of every wdoc document is: `page` blocks hold content; each page joins one or more `site` blocks; each site picks a **template** (`:webpage`, `:book`, `:presentation`, `:ai_skill`, or a custom one); and a CLI **render target** turns sites into output.


## Build and serve

```console
wcl wdoc build docs/main.wcl --out docs/_site
wcl wdoc serve docs/main.wcl                       # http://127.0.0.1:8080
wcl wdoc serve docs/main.wcl --addr 127.0.0.1:3000 # custom address
wcl wdoc build docs/main.wcl --out _site --site docs   # filter to one site
```

## Multi-site routing

A document can declare several `site` blocks. The site with `root = true` renders at the output root (`/`); other sites render into per-site subdirectories (`/<site>/`). A `chooser` index is auto-generated when no site is rooted.


## Example

```wcl
import <wdoc.wcl>

page index { sites = [:mysite]  start = true
  h1 "My project"
  p "Welcome — see [the docs](docs)."
}
```

## Render targets

| Target | Renders |
| --- | --- |
| `wcl wdoc build` | One `.html` per page; multi-site documents nest under per-site subdirectories. |
| `wcl wdoc serve` | A watch-rebuild dev server (default `127.0.0.1:8080`). |
| `wcl wdoc markdown` (alias `md`) | A folder of `.md` files, aimed at AI / text consumers. |
| `wcl wdoc pdf` | A pure-Rust PDF per site, paginated onto A4 / US-Letter. |
| `wcl wdoc skill` | A Claude / agent skill folder (`SKILL.md` + `references/`). |

## Related

- [Pages](../references/concept_pages.md)

- [Sites](../references/concept_sites.md)

- [Templates](../references/concept_templates.md)

[← Back to SKILL.md](../SKILL.md)
