# AGENTS.md

Read `CLAUDE.md` first: it has the project directives (a language change lands in
`crates/`, `docs/reference/` and `.claude/skills/wcl/`) and the repo layout.
This file adds the Terrarium delivery flow for agents.

## Validation

One command validates a change. Run it on the target `wcl-helios-build` against
your pushed commit.

Your sandbox workspace may not be visible on the target, and the target cannot
reach GitHub over SSH. Clone the pushed branch on the target over HTTPS into
`/tmp/<card>` (HOME on the target is `/tmp`; nothing there survives a target
recreate), check the commit, and validate there:

```sh
terrarium exec wcl-helios-build -- sh -c 'rm -rf /tmp/wcl-12 && \
  git clone -q --branch wcl-12 https://github.com/wiltaylor/wcl.git /tmp/wcl-12/wcl && \
  git -C /tmp/wcl-12/wcl rev-parse HEAD'
terrarium run wcl-helios-build --name validate -- sh -c 'cd /tmp/wcl-12/wcl && just ci::check'
```

The image preinstalls the pinned toolchain (`rust-toolchain.toml`). Never run
`rustup component add/remove` or `rustup toolchain` on the target: the install is
shared by every job and card. A missing component is a target problem: move the
card to `blocked` and tell the lead. The target has no Node or uv: install them
under `/tmp/<card>` only when the card needs them (`just vscode-build`,
`just vscode-package`).

`just ci::check` is the merge bar: `fmt-check`, `workspace-lint` (clippy, warnings
are errors), `workspace-doc` (rustdoc, warnings are errors), `workspace-test`, and
the docs site build with its relative URL check. GitHub CI runs the same recipes
(`.github/workflows/ci.yml`). A cold build takes several minutes, so start it with
`terrarium run wcl-helios-build --name validate -- sh -c '...'` and end your turn.

The affected-tests selector is `just ci::affected`. It reads
`TERRARIUM_CHANGED_FILES` (repo-relative paths, separated by newlines, spaces or
commas) and runs fmt, clippy, rustdoc and `cargo test -p` for the crates under
the changed paths plus their reverse dependants from `cargo metadata` (a change in
`crates/wcl_lang` also tests `wcl`, `wcl_lsp` and `wcl_wdoc`). A change to
`examples/` tests every crate. The docs build runs when `docs/` changed or the
change reaches the `wcl` binary. An empty list, a workspace file (`Cargo.toml`,
`Cargo.lock`, `rust-toolchain.toml`, `justfile`, `.just/`) or a path with no rule
runs the full `just ci::check`. Card validation may use it in place of the full
recipe:

```sh
terrarium exec wcl-helios-build \
  -e TERRARIUM_CHANGED_FILES="$(git diff --name-only origin/next...HEAD)" \
  -- sh -c 'cd /tmp/wcl-12/wcl && just ci::affected'
```

The merge queue still runs the full `just ci::check` on each batch.

While you work, narrower recipes are fine: `just ci::workspace-test`,
`cargo test -p <crate>`, `just wdoc-test`. They do not replace `just ci::check`
or `just ci::affected` before `review`.

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
