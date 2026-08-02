# Template adaptation: the brownfield surgery

`scripts/new-issue-plan.sh` (bundled with this skill) scaffolds wplan's verified template with
`wcl init wplan <repo>/plans/<slug>/plan --defaults` and performs deterministic brownfield
surgery:


1. Deletes the greenfield bootstraps: `specs/spec_000_repo.wcl` and `specs/spec_010_build.wcl`.
2. Removes exactly their import lines from plan.wcl and their status rows from status.wcl. The script greps for each expected line **before** removing it and aborts loudly if any is missing - that means the built-in template has drifted from what this surgery was written against; re-verify the surgery by hand and update this skill.
3. Reports whether `.tree/x` is git-ignored in the target repo (`git check-ignore`), which decides the conditional prep spec (see the spec-shapes concept).

It never touches schema/, gates.wcl, the justfile, or the wdoc projections - the brief
renderer derives waves from whatever specs exist, so removing specs is safe. After the script:
`just check` in the new plan folder must pass with zero specs. If it doesn't, stop and
investigate before adding content.


**What you add by hand:** brownfield rules in prd.wcl alongside the template's defaults -
these are copied verbatim into every brief. Adjust wording to the repo (e.g. name the actual
suite command) but keep the three intents: convention-matching, no drive-by changes,
whole-suite green.


```wcl
rule rule_conventions { text = "Match the surrounding code's existing conventions exactly - naming, error handling, test structure. Do not introduce new patterns." }
rule rule_no_reformat { text = "Never reformat, reorder, or 'clean up' code you are not functionally changing." }
rule rule_suite_green { text = "The project's full existing test suite must pass before you finish. If a pre-existing test fails for reasons unrelated to your change, STOP and report it in AGENT_NOTES.md rather than fixing it." }
```

Everything else - questions, research + finding files, goals/non-goals/requirements, specs,
status rows - is normal wplan authoring. One import line in plan.wcl per new spec file and per
finding file; one status row per spec.


[← Back to SKILL.md](../SKILL.md)
