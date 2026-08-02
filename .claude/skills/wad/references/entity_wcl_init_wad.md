# wcl init wad

_command_

Scaffolds a complete WAD: schema, empty per-view data hubs, the book templates, extractor starter, justfile.

Prompts for `system_id`, `system_name`, `system_summary`, `date`, and `include_extractors`
(gates `scripts/` + the generated-data placeholder). Answer non-interactively with
`-D key=value` and `--defaults`:


```console
$ wcl init wad ./acme-wad -D system_id=acme -D system_name="Acme Shop" --defaults
```

The scaffolded book renders immediately (`just book-build`) — every chapter shows a “not
recorded yet” stub until its data arrives. All twelve views are always scaffolded; empty
folders are the checklist.


[← Back to SKILL.md](../SKILL.md)
