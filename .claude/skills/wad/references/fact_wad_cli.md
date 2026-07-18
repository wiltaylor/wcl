# WAD command table

| Command | Does |
| --- | --- |
| `wcl init wad <dir>` | scaffold a WAD (`-D key=value --defaults` for non-interactive) |
| `wcl check wad.wcl` | validate the whole model — the gate after every data edit |
| `wcl editor wdoc/book/main.wcl` | browser editor whose preview pane renders the live book with click-to-comment review (for humans) |
| `wcl wdoc build wdoc/book/main.wcl --out _site` | render the HTML book |
| `wcl diff <rev>:wad.wcl wad.wcl` | what changed since a revision (evaluated views) — a delta source for spec writing |
| `wcl wad spec --from <rev> [wad.wcl]` | seed a spec skeleton in data/specs/ from the diff (`--id`, `--title`, `--format json`, `--include-specs`) |

A scaffolded WAD's justfile wraps these as `wad-check`, `book-build`, `book-serve`, `md-build`, `extract`, `spec-new REV`, `clean`. In the WCL repository the root justfile prefixes them `wad-*`.

## Related

- [wcl init wad](../references/entity_wcl_init_wad.md)

- [wcl diff (WAD usage)](../references/entity_wcl_diff_wad.md)

[← Back to SKILL.md](../SKILL.md)
