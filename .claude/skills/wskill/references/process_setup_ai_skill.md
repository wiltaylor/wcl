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

Add the single `skill` block in `wskill.wcl`. Its fields become `SKILL.md`'s front matter
and intro: the `allowed_tools` / `disallowed_tools` / `disable_model_invocation`
permissions, a `summary` overview, invocation `skill_param`s, and `skill_boundary`
guardrails the agent must follow.


### Step 2: Tag content for the skill

```wcl
concept fast_forward { audience = :both  ... }   // book + skill
fact   port_table    { audience = :ai    ... }   // skill only
```

Every unit defaults to `audience = :book`, so it stays out of the skill. Mark the units the
agent needs `:ai` (skill only) or `:both` (book and skill). Curate up — the skill stays lean
because content has to opt in.


### Step 3: Curate the skill navigation

`SKILL.md` is index-driven: add an `index` with `audience = :ai` (or `:both`) whose
`related` lists the units the agent should reach. Each such index is inlined into `SKILL.md`
as a section linking straight to those units. See \*Building the wskill index\*.


### Step 4: Build and install the skill

```console
$ wcl wskill check .
$ wcl wskill install . --repo <repo>
```

Run `wcl wskill check .` to build every declared projection in scratch space, then
`wcl wskill install . --repo <repo>` to render and install `SKILL.md`, `references/*.md`,
bundled files, and any agents.
[Building and installing the AI skill](../references/process_installing_the_skill.md) covers drift checking
and how to verify the agent actually loads it.


> [!TIP]
> **Verification**
>
> `wcl wskill install . --repo <repo> --check` passes and the installed `SKILL.md` lists your `:ai`/`:both` indexes, with a `references/` page per `:ai`/`:both` unit.

## Related

- [Building the wskill index](../references/process_building_the_index.md)

- [Building and installing the AI skill](../references/process_installing_the_skill.md)

[← Back to SKILL.md](../SKILL.md)
