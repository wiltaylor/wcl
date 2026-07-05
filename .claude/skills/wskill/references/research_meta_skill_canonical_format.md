# Canonical Claude-skill format (meta-skill)

_Researched: 2026-07-05 · status: current · applies to: meta-skill skill-lint.py as of 2026-07_

**Question:** What shape must a generated SKILL.md have to lint clean against the meta-skill validator?

## Finding

SKILL.md needs a lowercase-hyphen name matching its directory, a trigger-phrased description, canonical overview/variables/boundaries sections (always/ask/never), and a body within 150 lines.

Hard failures (exit 1): frontmatter `name` must match `^[a-z][a-z0-9-]{0,62}[a-z0-9]$` AND equal the skill's directory name; line-start lowercase XML tags must balance; `${CLAUDE_SKILL_DIR}/…` file references must resolve.

Warnings: missing `<overview>` / `<variables>` / `<boundaries>` (with `<always>`/`<ask>`/`<never>`) sections; body over 150 lines after frontmatter; description under 40 chars or without trigger phrasing ("use when …").

The frontmatter spec's home for tooling key-values is a `metadata:` map — unknown top-level keys are not rejected by the lint, but `metadata` is the compliant place for them (`wskill_schema_version` lives there).

Description style: third person, states what the skill does AND when to use it, with real trigger keywords a router would match.

## Sources

- ~/.claude/skills/meta-skill/reference/frontmatter-spec.md

- ~/.claude/skills/meta-skill/reference/skill-template.md

- ~/.claude/skills/meta-skill/scripts/skill-lint.py

## Related

- [Anatomy of the AI skill](../references/concept_skill_anatomy.md)

[← Back to SKILL.md](../SKILL.md)
