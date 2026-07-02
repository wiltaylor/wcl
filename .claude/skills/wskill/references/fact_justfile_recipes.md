# The standard just recipes

Every scaffolded wskill ships a `justfile` wrapping the wcl CLI, so the everyday loop is
the same in every wskill folder. Run bare `just` to list them.


| Recipe | What it does |
| --- | --- |
| `just wskill-check` | Validate the model and every projection template |
| `just book-build` | Render the book to `out/book` |
| `just skill-build` | Render the AI skill to `out/skill` |
| `just presentation-build` | Render the overview deck to `out/presentation` (shipped views only) |
| `just training-build` | Render the training book to `out/training` (shipped views only) |
| `just render` | Render every shipped projection |
| `just book-serve` | Live-preview the book |
| `just out-clean` | Delete generated output |

Installing the rendered skill is a copy: `cp -r out/skill <repo>/.claude/skills/<name>` —
see [Installing the skill](../references/process_installing_the_skill.md). A host repo that embeds wskills
(like the WCL repo itself) usually adds its own wrapper recipe pointing at the same
entry files.


## Related

- [The wcl commands an author uses](../references/fact_authoring_cli.md)

- [Building and installing the AI skill](../references/process_installing_the_skill.md)

- [The view family](../references/concept_views.md)

[← Back to SKILL.md](../SKILL.md)
