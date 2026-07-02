# Claude Code

_software_

Anthropic's coding agent — the host the rendered AI skill installs into.

Claude Code is the agent host the AI-skill view targets. It loads skills from a repo's
`.claude/skills/<name>/` folder: the SKILL.md frontmatter `description` tells the agent
when to invoke the skill, and the references/ pages are what it reads to answer with.


| Field | Value |
| --- | --- |
| Skill location | `<repo>/.claude/skills/<name>/` |
| Loads first | `SKILL.md` (frontmatter + boundaries + reference index) |
| Invocation | automatic on matching requests, or explicit via `/<name>` |

## Related

- [Anatomy of the AI skill](../references/concept_skill_anatomy.md)

- [Building and installing the AI skill](../references/process_installing_the_skill.md)

[← Back to SKILL.md](../SKILL.md)
