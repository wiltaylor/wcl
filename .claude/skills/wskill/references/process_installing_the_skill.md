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
$ wcl wskill check .        # model + every declared projection + coverage
```

`check` resolves artifacts from the parsed wskill model and builds every declared projection
in scratch space. It fails loudly on broken entries, links, schema errors, or template/render
errors, and reports how many model nodes each view reaches without writing generated output.


### Step 2: Install into the host repo

```console
$ wcl wskill install . --repo <repo>
```

`install` renders the AI-skill artifact itself, reads each generated SKILL.md name, and
replaces the matching `.claude/skills/<name>/` folder wholesale so removed pages do not
linger. It installs generated agents into `.claude/agents/` after checking the flat agent
namespace for collisions. Use `--check` in CI to detect drift and stale generated output
without writing; subagent files load at session start, so restart after installing.


### Step 3: Verify the agent loads it

Open an agent session in the target repo and ask something squarely inside the skill's
description. The agent should invoke the skill and answer citing its reference pages. If it
never triggers, sharpen the `description` in the `skill` block — it is the trigger text.


> [!TIP]
> **Verification**
>
> `wcl wskill install . --repo <repo> --check` passes, `.claude/skills/<name>/SKILL.md` has the expected frontmatter, and an agent session in that repo invokes the skill on a matching request.

## Related

- [Setting up AI skill generation](../references/process_setup_ai_skill.md) — Setting up AI skill generation supports Building and installing the AI skill by defining the skill block, audience, and indexes that installation renders.

[← Back to SKILL.md](../SKILL.md)
