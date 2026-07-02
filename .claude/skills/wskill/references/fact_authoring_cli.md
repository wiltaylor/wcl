# The wcl commands an author uses

Everything shells out to the `wcl` CLI. These are the commands the wskill workflows lean on:

| Command | What it does |
| --- | --- |
| `wcl init wskill <dir>` | Scaffold a new wskill folder (prompts for topic id/name/summary; `--defaults` / `-D key=value` for non-interactive) |
| `wcl init wskill-registry <dir>` | Scaffold a registry/collection landing that discovers wskills under `wskills/` |
| `wcl check wskill.wcl` | Validate the whole model against the schema — run before every render |
| `wcl wdoc build wdoc/book/main.wcl --out out/book` | Render the book (also builds the presentation / training projections from their mains) |
| `wcl wdoc skill wdoc/skill/main.wcl --out out/skill` | Render the AI skill folder (SKILL.md + references/) |
| `wcl wdoc serve wdoc/book/main.wcl` | Live-preview a projection (Enter in the console rebuilds) |
| `wcl wdoc serve … --comment` | Review mode: click a block in the browser to pin a note into a `comments.wcl` sidecar |
| `wcl wdoc serve … --edit` | WYSIWYG mode: edit blocks and schema objects in the browser, writing real `.wcl` source |
| `wcl wdoc comments <root>` | List review comments (`--format json`; `resolve <id>` deletes one) |
| `wcl wdoc review <root>` | Agent side of the review loop: block until the reviewer clicks "Send to agent", then print the comments |
| `wcl fmt <file>` | Canonically format a `.wcl` file |

Each wskill's `justfile` wraps the common ones — see [the standard recipes](../references/fact_justfile_recipes.md).

## Related

- [Creating a new wskill](../references/process_creating_a_wskill.md)

- [Reviewing a wskill (human ⇄ agent loop)](../references/process_reviewing_a_wskill.md)

- [Editing a wskill in the browser](../references/process_editing_via_serve.md)

[← Back to SKILL.md](../SKILL.md)
