# Sites

_The `site` block: one output target — template, title, theme, multi-site routing, and full-text search._

A `site` block configures one output site — its template, title, theme, and navigation. A document can declare several sites; each page joins one or more via its `sites` field.


```wcl
site marketing {
  default_template = :webpage
  title            = "My project"
  root             = true
  theme            = :nord
  theme_toggle     = true
}
```

## Search

Set `search = true` on a site to add client-side full-text search. The build writes a per-page text index to `_wdoc/search-index.json` and the `book` and `webpage` templates render a search box — in the sidebar and the nav respectively.


> [!NOTE]
> **Served, not opened**
> The widget fetches the index over HTTP, so search works when the site is hosted (or under `wcl wdoc serve`), not when a page is opened directly from disk.

## Block reference

The `@document` root: the set of tags legal at the top level of a wdoc document — pages, sites, includes, templates, classes, and more.

| Property | Type | Required | Description |
| --- | --- | --- | --- |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `classes` | `class` | yes |  |
| `stylesheets` | `stylesheet` | yes |  |
| `pages` | `page` | yes |  |
| `patterns` | `inline_pattern` | yes |  |
| `templates` | `template` | yes |  |
| `iconsets` | `iconset` | yes |  |
| `tilesets` | `tileset` | yes |  |
| `themes` | `theme` | yes |  |
| `components` | `wdoc_component` | yes |  |
| `generators` | `wdoc_repeater` | yes |  |
| `partials` | `partial` | yes |  |
| `bodies` | `body` | yes |  |
| `sites` | `site` | yes |  |
| `includes` | `include` | yes |  |

A `site` block configuring one output target — its template, title, theme, navigation, search, and multi-site routing.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | no | Optional site name (the label slot). Required with more than one site; names the output subdirectory referenced by `Page.sites`. |
| `root` | `bool` | no | When `true`, this site renders at `/` (others go under `/<site>/`). |
| `default_template` | `symbol` | no | `:webpage` (header + top nav), `:book` (sidebar + TOC), `:presentation` (slide deck), or `:ai_skill` (skill folder, Markdown-only — build with `wcl wdoc skill`). |
| `title` | `utf8` | no | Site title shown in the header / sidebar. |
| `summary` | `utf8` | no | One-line site description. Surfaced by `included_sites(...)` (not rendered by the built-in templates). |
| `icon` | `utf8` | no | Path to a favicon image (svg/png/ico), resolved relative to the document and copied into `_wdoc/`. Absent ⇒ a default WCL icon is used. |
| `stylesheets` | `list<utf8>` | no | External stylesheet hrefs added to every page `<head>` as `<link rel="stylesheet">` (verbatim — URL, copied asset, or shipped `file`). |
| `scripts` | `list<utf8>` | no | Script srcs added to every page `<head>` as deferred `<script src=… defer>` (verbatim hrefs). |
| `fonts` | `list<utf8>` | no | Web-font stylesheet hrefs added to every page `<head>` as `<link rel="stylesheet">` (e.g. a Google Fonts URL). |
| `assets` | `list<utf8>` | no | Folders copied verbatim (recursively) into the site output — e.g. a Vite `dist/`. Resolved relative to the document; reference copied files by their output path. |
| `theme_toggle` | `bool` | no | When `true`, adds a light/dark toggle button. |
| `search` | `bool` | no | When `true`, adds a client-side full-text search box (book and webpage templates). |
| `theme` | `symbol` | no | Symbol naming a colour `theme` block (`:nord` …) — see Styling. |
| `accent` | `symbol` | no | Symbol naming the accent hue (`:red`..`:pink`); default `:blue`. |
| `ui_theme` | `symbol` | no | UI theme for `wf_*` wireframe elements (the mocked app's theme), separate from the document `theme`. Falls back to `theme`. |
| `ui_accent` | `symbol` | no | Accent hue for `wf_*` wireframe elements; falls back to `accent`. |
| `ui_mode` | `symbol` | no | Mode for `wf_*` wireframe elements — `:dark` (default) or `:light`. |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `toc` | `toc` | no | Chapter tree for the `book` template. |
| `menu` | `menu` | no | Top navbar entries for the `webpage` template. |
| `sidebar_footer` | `sidebar_footer` | no | Pinned buttons at the bottom of the `book` sidebar. |
| `deck` | `deck` | no | Slide grid for the `presentation` template. |
| `skill` | `skill` | no | Skill metadata for the `:ai_skill` target — populates SKILL.md's front matter. |

## Related

- [wdoc Overview](../references/concept_overview.md)

- [Pages](../references/concept_pages.md)

- [Templates](../references/concept_templates.md)

- [Styling](../references/concept_styling.md)

[← Back to SKILL.md](../SKILL.md)
