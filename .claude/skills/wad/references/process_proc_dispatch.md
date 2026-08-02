# Dispatching implementation

## Purpose

Branch/worktree setup, prompt assembly, launch and collection for each dispatch-ready spec.

## Prerequisites

- A dispatch-ready spec (:todo, all deps :merged)
- Fresh briefs (just specs ran this iteration)

## Flowchart

![diagram](../_wdoc/process_proc_dispatch-diagram-1.svg)

## Steps

### Step 1: Branch + worktree

```console
$ git branch <branch> <TRUNK>            # skip if it already exists (resume)
$ git worktree add .tree/<branch> <branch>
```

The brief's frontmatter names the branch (spec/NNN-name). Create it from current trunk and mount a worktree. If .tree/<branch> already exists from an interrupted run, reuse it after inspecting it (see the orchestrator loop's resume note).

### Step 2: Assemble the prompt

```markdown
# Role

You are an implementation agent. Implement the spec below exactly as written.
Your working directory is: <absolute worktree path>
The git branch and worktree are ALREADY set up — ignore any branch/worktree
setup instructions inside the spec and just work in the directory above.

# Hard rules

- Work ONLY inside your working directory, and within it touch only the paths
  the spec allows. Do not read or modify anything outside it.
- Follow the spec literally. If something is ambiguous, choose the smallest
  reasonable interpretation and note the choice in AGENT_NOTES.md in the
  worktree root — do not expand scope.
- Run the spec's acceptance commands yourself before finishing; make them pass.
- Commit all work on the current branch with clear messages. Do not push,
  do not merge, do not switch branches.
- When the spec is complete: commit, write a short completion summary as the
  final section of AGENT_NOTES.md, and STOP. Do not update any status or
  tracking files of any kind.

# The spec

<full brief text pasted here>
```

Write `.wbuild/prompts/<spec>-<attempt>.md`: the wrapper above followed by the **full brief text** from plan/out/specs/<spec>.md, copied verbatim - never summarise or trim a brief. The closing "do not update any status files" line is deliberate cheap insurance (weak models weight recent task text heavily). Nothing about status.wcl, the plan folder, or other specs may appear in the prompt. After writing, verify the brief went in whole: the file must end with the brief's final line and its line count must be at least wrapper plus brief - a truncated brief is a silent scope cut.

### Step 3: Launch and record

Set the spec's status row to `:in_progress` (by = "orchestrator"). Launch via the runner (or the Task-subagent fallback - see the runner-contract fact), backgrounded, respecting MAX_PARALLEL. Log to .wbuild/logs/.

### Step 4: Collect

When the runner exits: exit 0 **and** the branch has new commits - set `:implemented` and queue verification. Exit 0 but no commits - treat as a failure: write a short report ("agent finished without committing") to .wbuild/reports/ and enter the fix loop. Non-zero exit (crash/timeout) - leave `:in_progress`, inspect the log; re-dispatch once, and if it crashes again, `:blocked` + escalate.

> [!TIP]
> **Verification**
>
> Prompt file contains the whole brief; status row moved; runner logged; outcome classified on exit.

[← Back to SKILL.md](../SKILL.md)
