# AGENTS.md

Read `CLAUDE.md` first: it has the project directives (a language change lands in
`crates/`, `docs/reference/` and `.claude/skills/wcl/`) and the repo layout.
This file adds the Terrarium delivery flow for agents.

## Validation

One command validates a change. Run it on the target `wcl-helios-build`, in your
checkout, against your pushed commit:

```sh
terrarium exec wcl-helios-build -- sh -c 'cd <your checkout> && just ci::check'
```

`just ci::check` is the merge bar: `fmt-check`, `workspace-lint` (clippy, warnings
are errors), `workspace-doc` (rustdoc, warnings are errors), `workspace-test`, and
the docs site build with its relative URL check. GitHub CI runs the same recipes
(`.github/workflows/ci.yml`). There is no affected-tests selector yet: run the
full recipe. A cold build takes several minutes, so start it with
`terrarium run wcl-helios-build --name validate -- sh -c '...'` and end your turn.

While you work, narrower recipes are fine: `just ci::workspace-test`,
`cargo test -p <crate>`, `just wdoc-test`. They do not replace the full recipe
before `review`.

Not in the merge bar (run them only when the card asks): `just ci::fuzz-sweep`
(nightly and cargo-fuzz), `just skill-test` (billed model calls), `just fuzz-soak`.

## Flow

The board follows the Terrarium delivery process (skill `delivery-process`).

- Branches: start each card branch from `next`, named after the card
  (`wcl-12`). A bug card of a release starts from `release/<X.Y.Z>`. Never push
  to `main`, `next` or a release branch.
- Commits: conventional subjects (`feat(lang): ...`, `fix(wdoc): ...`). The
  release CI derives the version bump from them.
- Before `review`: push, run `just ci::check` on the target against the pushed
  commit, and put the branch, the commit, the command, the exit code and the
  pass count in the move comment.
- The lead reviews, moves the card to `merge_queue`, and merges the whole
  column onto `next` as one job. That job runs `just ci::check` on the merged
  result. Green cards go to `accepted`.
- Releases: the lead cuts `release/<X.Y.Z>` from `next` and merges it into
  `main` with a `pre-release: true` trailer. The CI publishes `X.Y.Z-alpha`.
  Stable releases (`release: true`) are disabled in the CI.
