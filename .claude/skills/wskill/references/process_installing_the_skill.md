# Building and installing the AI skill

## Purpose

Render the skill projection and install it where an agent loads it.

## Prerequisites

- The `skill` block is configured — see [Setting up AI skill generation](../references/process_setup_ai_skill.md).

## Flowchart

![diagram](../_wdoc/process_installing_the_skill-diagram-1.svg)

## Steps

### Step 1: Render the skill folder

```console
$ just skill-build          # → out/skill/  (SKILL.md + references/)
```

`wcl wdoc skill` writes `SKILL.md`, one `references/*.md` per `:ai`/`:both` unit, and any bundled `scripts/`/`assets/` files. The build fails loudly on broken links or schema errors — fix and re-run.

### Step 2: Install into the host repo

```console
$ cp -r out/skill <repo>/.claude/skills/<name>
```

Copy the rendered folder into the target repo's `.claude/skills/<name>/`. Use the `name` from the generated SKILL.md frontmatter — that is what the agent invokes. A repo that hosts the wskill source usually wraps this in its own just recipe so the installed copy regenerates with the model.

### Step 3: Verify the agent loads it

Open an agent session in the target repo and ask something squarely inside the skill's description. The agent should invoke the skill and answer citing its reference pages. If it never triggers, sharpen the `description` in the `skill` block — it is the trigger text.

> [!TIP]
> **Verification**
> `.claude/skills/<name>/SKILL.md` exists with the expected frontmatter, and an agent session in that repo invokes the skill on a matching request.

## Related

- [Anatomy of the AI skill](../references/concept_skill_anatomy.md)

- [Setting up AI skill generation](../references/process_setup_ai_skill.md)

- [Claude Code](../references/entity_claude_code.md)

- [The standard just recipes](../references/fact_justfile_recipes.md)

[← Back to SKILL.md](../SKILL.md)
