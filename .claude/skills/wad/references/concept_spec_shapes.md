# Brownfield spec shapes

_The bug-fix spec (test-first, usually the whole DAG), feature decomposition along ownership lines, ownership in an existing tree, and the conditional prep spec._

Specs use wplan's schema unchanged (`spec` blocks with `title`, `objective`, `depends_on`,
`branch`, `owns`, `allowed`, `not_allowed`, `done`, `task`/`accept` children, and a `body` of
copied-in context). Numbering: `spec_0NN_name`, branch `spec/0NN-name`, gaps of 10. What
differs in brownfield is the \*content patterns\*.


**Requirement phrasing.** EARS form throughout - and the brownfield signature pattern
`WHEN [condition] THE SYSTEM SHALL CONTINUE TO [existing behaviour]` pins behaviour the change
must NOT break. Write one for each adjacent behaviour recon showed the change could plausibly
disturb; each becomes a regression assertion in the full-suite accept.


**The bug-fix spec** (usually the whole DAG) is test-first structurally, not as advice: task 1
writes a failing regression test reproducing the bug exactly as the
r_repro finding recorded it, and confirms it FAILS for the reported reason before any source changes; task 2 is the smallest fix that makes it pass; task 3 runs the full suite and STOPS (reporting in AGENT_NOTES.md)
if anything outside the owned files breaks. Accepts: the regression test, then the full suite.
Accept commands come from finding r_commands - the repo's real commands, never guessed. The
body copies in the reproduction, the affected-code map, and the conventions verbatim.


**Feature decomposition** works as in plan mode: split along disjoint ownership,
dependency-order the splits, and prefer fewer, larger specs over many entangled ones -
brownfield merges are riskiest where specs interleave in existing files. Every spec's body
copies in its slice of the findings; every spec's final accept runs the full existing suite.


**Ownership in an existing tree.** `owns` covers every path the spec will create **or modify**
- for brownfield that means existing files. The owns_disjoint gate is exact-string only, so
check file-vs-directory overlaps yourself. Shared integration points (DI registration, route
tables, manifest files, migration indexes) that several specs would touch: give ownership to
the **last** dependency-ordered spec that touches them; earlier specs note their contribution
under `allowed` (the stub-then-own pattern).


**spec_000_prep - conditional.** Only when the scaffold script reports `.tree/` NOT
gitignored, or recon found no green baseline command. Owns `[".gitignore"]`; tasks: add
`.tree/` to .gitignore, run the baseline build/test from
r_commands and record the result, commit. Everything else then `depends_on = [spec_000_prep]`. When neither condition holds, omit it - a one-spec bug fix should not pay a two-merge tax.


## Examples

### A complete brownfield bug-fix spec

Test-first is structural: the failing regression test is task 1, the fix is task 2, and the full existing suite gates completion. The body copies in the recon findings verbatim.

```wcl
spec spec_010_fix_login_timeout {
  title = "Fix login timeout on slow LDAP"
  objective = "Login must not time out when LDAP responds within the configured limit."
  depends_on = []
  covers = [req_login_timeout]
  branch = "spec/010-fix-login-timeout"
  owns = ["src/Auth/LdapAuthenticator.cs", "tests/Auth/LdapAuthenticatorTests.cs"]
  allowed = ["adding test helpers inside the owned test file"]
  not_allowed = ["changing the public API of LdapAuthenticator", "touching other authenticators", "reformatting untouched code", "adding dependencies"]
  done = ["regression test exists and passes", "full test suite green"]
  task t1 { text = "Write a failing regression test reproducing the bug exactly as described in the reproduction section below. Run it and confirm it FAILS for the reported reason before changing any source." }
  task t2 { text = "Fix the defect (see Affected code below). Smallest change that makes t1 pass." }
  task t3 { text = "Run the full suite; fix nothing outside your owned files - if something else breaks, STOP and report it in AGENT_NOTES.md." }
  accept a1 { check = "Regression test passes" command = "dotnet test --filter LdapAuthenticatorTests" }
  accept a2 { check = "Full suite green" command = "dotnet test" }
  body {
    p "Reproduction (finding r_repro): <verbatim repro command, observed vs expected output>."
    p "Affected code (finding r_map): <call path, key symbols, likely defect site>."
    p "Conventions (finding r_conventions): <test naming/location, error-handling pattern to imitate>."
  }
}
```

**Expected:** wcl check plan.wcl passes; just specs emits a standalone brief with the copied-in context. Wire it in with two literal lines: the import in plan.wcl and `status spec_010_fix_login_timeout { state = :todo }` in status.wcl.

## Related

- [File ownership](../references/concept_ownership.md) — File ownership supports Brownfield spec shapes: Each path has exactly one owning spec; disjoint ownership - not luck - is what makes parallel agents merge-safe.

[← Back to SKILL.md](../SKILL.md)
