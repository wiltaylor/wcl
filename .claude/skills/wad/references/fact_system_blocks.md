# Systems blocks

| Block | Fields | Notes |
| --- | --- | --- |
| `system` | `name`, `summary`, `boundary?`, `owner?`, `repos[]`, `body?` | C4 level 1 — one box to an outsider; `boundary` files it under a view-2 `boundary` band |
| `container` | `system`, `name`, `summary`, `kind`, `technology?`, `repo?`, `body?` | C4 level 2 — an independently runnable unit: a service, a database, a web app, a CLI, a queue |
| `component` | `container`, `name`, `summary`, `kind?`, `technology?`, `body?` | C4 level 3 — a cohesive part with no deployment identity; `kind` is a ComponentKind (`:cli` / `:tui` / `:web_api` / `:ui` mark the editor's native content surfaces) |
| `code_item` | `component?` or `container?`, `name`, `summary?`, `kind`, payload children, `body?` | C4 level 4 — extractor-generated, public interface only; `kind` selects the payload family below |
| `cli_command` | `name`, `summary`, `component?`, `description?`, `aliases[]`, args + flags, `body?` | one command or subcommand of a CLI surface — THE way a WAD documents a CLI (never prose paragraphs or a flat table); renders API-docs style on its component's page |
| `cli_arg` / `cli_flag` / `cli_example` | arg: `name`, `description`, `required`; flag: `name`, `value?`, `description`, `default?`, `repeatable`; example: `command`, `description?` | nested in `cli_command` — the usage synopsis derives from args+flags; examples render as bash blocks under the tables |
| `screen` | `name`, `summary?`, `component?`, `container?`, `route?`, `personas[]`, `nav_to[]`, `body?` | a user-facing surface — web page, CLI command, TUI screen; attach it to its `component` (or `container` when components aren't modelled); the body holds the wireframe or terminal mock-up |

A `code_item`'s `kind` selects which payload children the renderer reads:

| kind | payload children | renders as |
| --- | --- | --- |
| :module_graph | `code_node` (`name`, `deps[]`, `summary?`) | layered dependency diagram |
| :db_schema | `db_table` → `db_column` (`type`, `pk`, `nullable`, `ref_table?`, `ref_column?`) | node tables with FK edges on row-level ports |
| :class_diagram | `code_node` with `members[]` | class boxes with member rows, dep edges |
| :api | `api_endpoint` (`method`, `path`, `summary`, `description?`, `auth?`, `request?` + `request_media_type?`, `response?`, params, responses) | endpoint index + a swagger-style detail section per endpoint |
| :other | — | just the `body` |

**Web APIs are documented at swagger depth.** An `api_endpoint` is OpenAPI-shaped on purpose: `api_param` children (`name`, `location` :path/:query/:header/:cookie, `type?`, `required`, `description?`, `example?`) mirror OpenAPI parameters, `request` + `request_media_type` mirror the requestBody, and `api_response` children (`status`, `description`, `media_type?`, `schema?`) mirror per-status responses. That mapping is the point: an existing OpenAPI spec imports through a mechanical extractor (one operation → one `api_endpoint`), and a spec can be generated back out of the WAD as a projection when designing a new API. The bare `request`/`response` text fields remain the simple form for surfaces that don't need per-status detail.

Who populates: systems/containers/components by hand (interview or scan); code items by extractor, and they document the **exposed interface** — internals below it are read from the code, not mirrored into the WAD. Code items and screens both live at **component level**: a component's page renders each owned code item's diagram in place (drill into the code item's own page for detail) and the component's screen-flow diagram (never on the system or container page — wrong altitude). **Every component should end up with code data**: a per-component surface extractor (see the reference WAD's `extract_modules.py` — a component→modules map, one `:class_diagram` item per component with the modules' exposed items as members) keeps the whole drill-down populated mechanically.

> [!WARNING]
> **Reference fields are bare identifiers**
>
> `system`, `container`, `repo`, `owner`, relation endpoints — every field that names another block's id is written **bare**, never quoted: `system = shop_system`, not `system = "shop_system"`. Newer wcl coerces a quoted ref to the identifier it names, but older binaries treat it as a plain string that equals nothing — every derived view (the drill-down, roll-ups, screen attachment) then silently renders empty while `wcl check` stays green. Bare is the canonical form; follow the shape below.

The drill-down authored in full — one system, a container, a component, all linked by bare ids:

```wcl
system shop_system {
  name    = "Shop"
  summary = "The web shop."
  repos   = [repo_shop]
}
container shop_api {
  system     = shop_system        // bare id — never "shop_system"
  name       = "shop-api"
  summary    = "The HTTP API."
  kind       = :service
  technology = "Rust, axum, SQLx"  // linked libraries live HERE, not in externals
  repo       = repo_shop
}
component checkout {
  container = shop_api             // bare id
  name      = "Checkout"
  summary   = "Cart, pricing, order placement."
  kind      = :module              // ComponentKind symbol, not a string
}
```

## Related

- [The C4 drill-down](../references/concept_c4_drilldown.md)

[← Back to SKILL.md](../SKILL.md)
