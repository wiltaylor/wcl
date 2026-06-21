---
name: pre-release
description: "Cut a WCL GitHub pre-release: commit the trailer-gated release trigger, push, watch CI build the artifacts, then deploy the new pre-release locally with install.sh. Use when the user says /pre-release, or asks to cut/ship/release a pre-release, bump the alpha, or deploy the latest build."
user-invocable: true
argument-hint: "[one-line summary of what's being released]"
allowed-tools:
  - Bash(git:*)
  - Bash(gh:*)
  - Bash(./install.sh:*)
  - Bash(install.sh:*)
  - Bash(wcl:*)
  - Bash(sleep:*)
  - Read
---

# Cut a WCL pre-release

Automates the repo's release flow: a `pre-release: true` git trailer on a
push to `main` makes CI (`.github/workflows/ci.yml`, the `version` job) cut a
GitHub pre-release — an `-alpha` tag with the Linux/Windows `wcl` binaries and
the `.vsix` attached. This skill commits that trigger, pushes, watches CI to
completion, then installs the freshly-built pre-release with `install.sh`.

`$ARGUMENTS` (optional) is a one-line summary of what's being released; weave
it into the trigger commit body. If absent, derive the summary from the
commits since the last tag.

> [!IMPORTANT]
> Only stable releases are blocked in CI — `pre-release: true` is the correct,
> supported trailer. A `release: true` trailer fails the build on purpose; do
> not use it.

## Steps

Run these in order. **Stop and report** if any precondition or CI step fails —
never deploy a failed build.

### 1. Preconditions

- Confirm the branch is `main`: `git branch --show-current`. If not, stop and
  tell the user (releases cut from `main` only — the `version` job is gated on
  `refs/heads/main`).
- Confirm the substantive work is already committed:
  `git status --short`. Ignore the untracked `.bug/` directory (local bug
  tracking). If there are **uncommitted tracked changes**, stop and ask the
  user to commit their feat/fix first — uncommitted work won't be in the
  pushed commit CI builds from. This skill only adds the release *trigger*.
- Show what's being released: `git describe --tags --match 'v*' --abbrev=0` for
  the last tag, then `git log <last-tag>..HEAD --format='%h %s'` to list the
  commits that will ship. Conventional-commit subjects drive the bump
  (`feat:` → minor on 0.x, `fix:`/`chore:`/… → patch).

### 2. Create the release-trigger commit

Make an **empty** commit (the work is already committed) carrying the trailer.
Write a body that says what this pre-release covers (use `$ARGUMENTS` if given,
else summarize the commits from step 1). The trailer line must be exactly
`pre-release: true`, separated from the body by a blank line:

```
git commit --allow-empty -F - <<'MSG'
chore: cut pre-release

<one paragraph: what landed since the last pre-release>

pre-release: true
MSG
```

Verify the trailer is on HEAD:
`git log -1 --format='%(trailers:key=pre-release,valueonly)'` → `true`.

### 3. Push

`git push origin main`. Capture HEAD's sha for the next step:
`git rev-parse HEAD`.

### 4. Watch CI

Find the **CI** workflow run for the pushed commit, then watch it to
completion. Give the run a moment to register first (`sleep 6`), then:

```
gh run list --commit <sha> --workflow CI --json databaseId,status -q '.[0].databaseId'
```

(If that returns nothing yet, fall back to `gh run list --branch main --limit 5`
and pick the in-progress **CI** run for this commit.) Then:

```
gh run watch <run-id> --exit-status --interval 20
```

`--exit-status` makes the command fail if CI fails. If it fails, stop, surface
the failing job, and **do not deploy**. The Node.js-20 deprecation warnings in
the annotations are benign — only the job conclusion matters. Confirm success:
`gh run view <run-id> --json status,conclusion`.

### 5. Verify the release

Confirm the new pre-release and its assets exist before deploying:

```
gh release list --limit 1
gh release view <tag> --json tagName,isPrerelease,assets \
  -q '{tag: .tagName, prerelease: .isPrerelease, assets: [.assets[].name]}'
```

Expect a `vX.Y.Z-alpha` tag, `prerelease: true`, and three assets:
`wcl-<ver>-linux-x86_64`, `wcl-<ver>-windows-x86_64.exe`, `wcl-vscode-<num>.vsix`.

### 6. Deploy

Install the newest pre-release from the repo root:

```
./install.sh --pre
```

This downloads the Linux binary into `~/.local/bin/wcl` (per `install.sh`).

### 7. Verify the deployment

`wcl --version` — confirm it reports the version just released (the
`vX.Y.Z-alpha` from step 5, without the leading `v`). Report the final
outcome: the version cut, that CI passed, and that `wcl` is deployed.

## Notes

- Don't predict the version number — CI computes it by bumping from the latest
  remote tag, which may be ahead of your local checkout. Read it back from the
  release (step 5), don't assume.
- This skill does not touch `.bug/` reports, changelogs, or the docs site —
  commit those as part of your feat/fix before invoking `/pre-release`.
