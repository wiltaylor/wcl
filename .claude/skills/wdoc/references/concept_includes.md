# Including sub-sites

_The `include` block and `included_sites` builtin discover and embed other wdoc documents._

The `include` block builds \*other\* wdoc documents found under a folder and ships each one's rendered output into a subdirectory of this build — exactly as if you had run `wcl wdoc build` (or `wcl wdoc skill`) on each one separately. Unlike imports (which merge another file's blocks into the current document), an included document stays a **separate artifact**: it keeps its own pages and `_wdoc/` assets under its own output subdirectory.


> [!NOTE]
> **Include vs import**
> `import` pulls another file's declarations into \*this\* document (one merged site). `include` builds another document on its own and copies the result in (many independent sites under one output tree).

## Discovery: pattern vs entry

Name a folder (the inline label), then pick **exactly one** discovery mode. Each match builds into `<folder-basename>/<name>/`. **`pattern`** is a filename glob matched recursively; the sub-site name is the matching file's parent folder. **`entry`** is a fixed relative path checked inside each immediate subdirectory (no recursion).


```wcl
import <wdoc.wcl>

// projects/foo/main.wcl  →  <out>/projects/foo/
include "projects" { pattern = "main.wcl" }

// members/ls/wdoc/book/main.wcl   →  <out>/members/ls/
include "members" { entry = "wdoc/book/main.wcl" }
```

## Picking a site

A member may declare several sites (e.g. a `:book` web site and an `:ai_skill` site over one model). The optional `site` field names which one to build — it is passed as `--site` to the per-member build.


```wcl
include "members" { entry = "main.wcl"  site = "book" }   // each member's :book site only
```

## Wiring navigation

The companion `included_sites(options)` builtin runs the same scan and returns one `{ name, href, title, summary }` record per discovered sub-site. The argument is a **record mirroring the include block's fields** (WCL has no keyword arguments); pass the \*same\* options so the links line up with where the sub-sites were built. Note record fields use `:` where block fields use `=`.


```wcl
site main { root = true  default_template = :webpage
  menu {
    item "Home" { page = index }
    wdoc_repeater { each = included_sites({ folder: "members", entry: "main.wcl", site: "book" })  as = :s
      item $"${s.title}" { href = s.href }   // label from the member's title
    }
  }
}
```

> [!WARNING]
> **Multi-site hrefs**
> `included_sites` returns **root-relative** hrefs (`members/foo/`). A non-root site renders under `/<site>/`, so its menu must reach a sibling sub-site with a `../` prefix (`../members/foo/`).

## Skill collections

Because `include` also embeds for the skill target, one `wcl wdoc skill` renders every member's skill into `<out>/<folder>/<name>/`. A collection document can be pure fan-out — just `include` blocks, no site of its own.


```wcl
import <wdoc.wcl>
include "../members" { entry = "main.wcl"  site = "skill" }   // each member's :skill site
```

```console
wcl wdoc build  landing.wcl    --out out/site     # landing + each member's :book
wcl wdoc skill  collection.wcl --out dist/skills  # each member's :skill → dist/skills/members/<name>/
```

## Block reference

An `include` block: a folder of other wdoc documents to build independently and ship into a subdirectory, discovered by `pattern` or `entry`.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `folder` | `utf8` | yes | Folder to scan (the inline label): a directory resolved relative to this document. |
| `pattern` | `utf8` | no | Recursive filename glob (`*` / `?`) matched against files in the folder's subdirectories — e.g. "main.wcl". Set this OR `entry`, not both. |
| `entry` | `utf8` | no | A path checked inside each immediate subdirectory of the folder (no recursion) — e.g. "main.wcl" or "wdoc/book/main.wcl". Set this OR `pattern`, not both. |
| `site` | `utf8` | no | Which named site of each (multi-site) member to build, passed as `--site`. Absent ⇒ the member's own default site selection. |
| `prefix` | `utf8` | no | Output prefix override: sub-sites ship to `<prefix>/<name>/` instead of `<folder-basename>/<name>/`. Lets two includes over the same folder (different entries — e.g. each member's book and its deck) target distinct subdirectories. |

## Examples

### Building member sub-sites

An include block builds other wdoc documents under a folder and ships each result into a subdirectory. The entry mode checks a fixed relative path inside each immediate subdirectory.

```wcl
import <wdoc.wcl>

// members/ls/wdoc/book/main.wcl   ->  <out>/members/ls/
// members/cat/wdoc/book/main.wcl  ->  <out>/members/cat/
include "members" { entry = "wdoc/book/main.wcl" }
```

**Expected:** Each member with the entry file builds into <out>/members/<name>/ as a separate, self-contained artifact.

## Related

- [Data Views](../references/concept_data_views.md)

- [Sites](../references/concept_sites.md)

- [Skill folders](../references/concept_skills.md)

[← Back to SKILL.md](../SKILL.md)
