# Setting up AI skill generation

## Purpose

Configure the wskill so it projects into a Claude Code skill, and choose what the agent sees.

## Flowchart

![diagram](../_wdoc/process_setup_ai_skill-diagram-1.svg)

## Steps

### Step 1: Configure the skill block

```wcl
// wskill.wcl
skill {
  allowed_tools = ["Bash", "Read"]
  summary { overview = ["What the skill does and when to use it."] }
  skill_param "$ARGUMENTS" { description = "..."  value = "..." }
  skill_boundary { always = ["..."]  never = ["..."] }
}
```

Add the single `skill` block in `wskill.wcl`. Its fields become `SKILL.md`'s front matter and intro: the `allowed_tools` / `disallowed_tools` / `disable_model_invocation` permissions, a `summary` overview, invocation `skill_param`s, and `skill_boundary` guardrails the agent must follow.

### Step 2: Tag content for the skill

```wcl
concept fast_forward { audience = :both  ... }   // book + skill
fact   port_table    { audience = :ai    ... }   // skill only
```

Every unit defaults to `audience = :book`, so it stays out of the skill. Mark the units the agent needs `:ai` (skill only) or `:both` (book and skill). Curate up — the skill stays lean because content has to opt in.

### Step 3: Curate the skill navigation

`SKILL.md` is index-driven: add an `index` with `audience = :ai` (or `:both`) whose `related` lists the units the agent should reach. Each such index is inlined into `SKILL.md` as a section linking straight to those units. See \*Building the wskill index\*.

### Step 4: Build and install the skill

```console
$ wcl wdoc skill wdoc/skill/main.wcl --out out/skill
$ cp -r out/skill <repo>/.claude/skills/<name>
```

Render the skill folder with `wcl wdoc skill` — it writes `SKILL.md` plus `references/*.md`. Install it by copying `out/skill` into a repo's `.claude/skills/<name>/`; [Building and installing the AI skill](../references/process_installing_the_skill.md) covers the install and how to verify the agent actually loads it.

> [!TIP]
> **Verification**
> `wcl wdoc skill` produces a `SKILL.md` whose reference section lists your `:ai`/`:both` indexes, with a `references/` page per `:ai`/`:both` unit.

## Related

- [Structured data](../references/concept_structured_data.md)

- [Building the wskill index](../references/process_building_the_index.md)

- [Creating a new wskill](../references/process_creating_a_wskill.md)

- [Building and installing the AI skill](../references/process_installing_the_skill.md)

- [Anatomy of the AI skill](../references/concept_skill_anatomy.md)

[← Back to SKILL.md](../SKILL.md)
