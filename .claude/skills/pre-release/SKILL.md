---
name: pre-release
description: "Skill instructs how to do releases and pre-releases of wcl. Use this when ever the user asks you to do a release or a pre-release"
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

# Summery
This repository uses github actions to do a release. They work
by reading the trailers on a git commit.

IMPORTANT: You have the option of doing a pre-release or a release version. When it doubt
always ask the user and default to pre-release.

IMPORTANT: We are not yet at a stable release version of wcl. Do not use stable till I remove this line from the skill file.
IMPORTANT: If the user asks to create a release version ask them to remove these lines from the skill file first. Do not offer to do it for them.

## Pre-Checks
You must check the following before you create a pre-release or release:
- You are on the main branch. If the user is a on different branch ask them to merge their pr first.
- Make sure there are no uncommited changes in the branch.

## How to do a pre-release or a release:
To do a pre-release you need to create a release commit like the following:

```
git commit --allow-empty -F - <<'MSG'
chore: cut pre-release
<Description of changes here>
pre-release: true
```

To do a release you need to put the following trailer at the bottom of a git commit:

```
git commit --allow-empty -F - <<'MSG'
chore: cut release
<Description of changes here>
release
release: true
```

This will trigger a github action build that will also publish the packages to github releases and create a tagged version under there.

## Promoting a Pre-release version to Release
To do this simply create a new release commit above. Don't try to rename a tag.

## Watch CI
If you want to watch the progress of the creation of a release you can run the following commands:

List its current progress:
```
gh run list --commit <sha> --workflow CI --json databaseId,status -q '.[0].databaseId'
```

Watch it and wait till it completes or fails.
```
gh run watch <run-id> --exit-status --interval 20
```

### Verify the release

Confirm the new pre-release and its assets exist:

```
gh release list --limit 1
gh release view <tag> --json tagName,isPrerelease,assets \
  -q '{tag: .tagName, prerelease: .isPrerelease, assets: [.assets[].name]}'
```

Expect a `vX.Y.Z-alpha` tag, `prerelease: true`, and three assets:
`wcl-<ver>-linux-x86_64`, `wcl-<ver>-windows-x86_64.exe`, `wcl-vscode-<num>.vsix`.

