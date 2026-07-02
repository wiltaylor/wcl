# Skill folders

_The \`:ai_skill` target: the `skill { }` block, folder layout, front matter, and `file\` blocks._

`wcl wdoc skill <file> --out <dir>` renders a document to an agent / Claude **skill folder** — a `SKILL.md` plus the conventional `references/`, `scripts/` and `assets/` subfolders. It's a Markdown-backed target (the [Markdown output](../references/concept_markdown.md) mapping applies), but the \*folder layout\* and `SKILL.md` front matter follow the skill convention.


```console
wcl wdoc skill docs/my-skill.wcl --out ./my-skill
```

## Opting in

A `site` becomes a skill by setting `default_template = :ai_skill` and declaring a `skill { }` block. The `skill` block supplies the required front-matter `name` and `description` (and an optional `license`) that the backend writes onto `SKILL.md`.


```wcl
import <wdoc.wcl>

site my_skill {
  default_template = :ai_skill
  skill {
    name        = "demo-skill"
    description = "What this skill does and when to use it."
    license     = "MIT"
  }
}

page overview { start = true
  h1 "Demo Skill"
  p "Instructions… see the [usage guide](usage)."
}

page usage {
  h1 "Usage"
  p "Details. Back to the [overview](overview)."
}
```

## Folder layout

The site's `start` page (`start = true`) becomes `SKILL.md` at the folder root; every other page is written under `references/<name>.md`. Internal links between pages resolve into that layout automatically — a link to the start page points at `SKILL.md`, a link to any other page at `references/<name>.md`.


```text
my-skill/
  SKILL.md            # the start page
  references/
    usage.md          # every other page
  scripts/            # files declared with dir = "scripts"
  assets/             # files declared with dir = "assets"
  _wdoc/              # generated diagram / terminal SVGs
```

## Front matter

`SKILL.md`'s YAML header is built from the `skill { }` block. To add extra keys (for example `allowed-tools`), author a `frontmatter` block on the start page — its keys are merged after the canonical `name` / `description` / `license` (which the `skill` block owns).


## Shipping files

A `file` block copies an arbitrary file from beside the document into the output and keeps its basename, so the path is stable and hand-linkable. `dir` names the target subfolder (`scripts`, `assets`, …). Set `as` to render a link to it; omit `as` to ship it silently and reference it by its path yourself.


```wcl
page overview { start = true
  h1 "Demo Skill"
  // Renders a link: [run setup](scripts/setup.sh)
  file "src/setup.sh" { dir = "scripts"  as = "run setup" }
  // Shipped silently to assets/logo.svg
  file "src/logo.svg" { dir = "assets" }
}
```

> [!NOTE]
> **Not an HTML template**
> `:ai_skill` is a Markdown-only target. Building a skill site with `wcl wdoc build` (HTML) fails with a message pointing you at `wcl wdoc skill`. The `file` block works on every target — on the HTML and Markdown targets a `dir`-less `file` lands in the `_wdoc/` asset folder.

## Sharing pages with a website

A `:ai_skill` site can live in the same document as a `:webpage` or `:book` site. `wcl wdoc build` / `pdf` / `markdown` skip the skill site, and `wcl wdoc skill` builds only it — so one source feeds both a hosted site and a skill. Because a page joins a site through its `sites = [ … ]` list, the \*same\* reference page can belong to both.


```wcl
site handbook  { default_template = :book }
site assistant { default_template = :ai_skill
  skill { name = "handbook"  description = "Project handbook for agents." }
}

// Shared: appears in the book *and* the skill's references/.
page deploys { sites = [:handbook, :assistant]
  h1 "Deploys"
  p "…"
}
```

## Block reference

A `skill` block: the `:ai_skill` front-matter — the required `name` and `description` (plus optional `license`) written onto `SKILL.md`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `utf8` | yes | Skill name (SKILL.md front-matter `name`). A short lowercase slug. |
| `description` | `utf8` | yes | Skill description (SKILL.md front-matter `description`). |
| `license` | `utf8` | no | Optional license string for the front matter. |

A `file` block: copies a file from beside the document into the output (keeping its basename), into the `dir` subfolder, optionally rendering a link via `as`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | File source (the inline label): a doc-relative path, a URL, or a `data:` URI. |
| `dir` | `utf8` | no | Output subdirectory. Skill target: e.g. "scripts" / "assets". HTML/Markdown: defaults to the `_wdoc/` asset folder. |
| `as` | `utf8` | no | Link text. When set, the block renders a link to the copied file; absent ⇒ the file is shipped silently. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional style classes (HTML target, when `as` renders a link). |

## Examples

### Opting a site into a skill

A site becomes a Claude skill by setting default_template = :ai_skill and declaring a skill { } block that supplies the SKILL.md front matter.

```wcl
import <wdoc.wcl>

site my_skill {
  default_template = :ai_skill
  skill {
    name        = "demo-skill"
    description = "What this skill does and when to use it."
    license     = "MIT"
  }
}

page overview { start = true
  h1 "Demo Skill"
  p "Instructions. See the [usage guide](usage)."
}
```

**Expected:** The start page becomes SKILL.md with name/description/license front matter; every other page lands under references/.

## Related

- [Sites](../references/concept_sites.md)

- [Templates](../references/concept_templates.md)

- [Markdown output](../references/concept_markdown.md)

- [Including sub-sites](../references/concept_includes.md)

[← Back to SKILL.md](../SKILL.md)
