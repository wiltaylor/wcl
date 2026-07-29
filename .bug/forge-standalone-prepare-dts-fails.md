# forge: `@forge/ui` declaration build fails when the package is prepared standalone (TS2883)

**Repo:** `wiltaylor/forge`
**Bad commit:** `4e120e38f17bd90787b69feb17d614e1b3192ab3` (current `main`)
**Last known-good for this consumer:** `0365090da3584e8a92316bc1d1c4462f755d81c2`
**Reported by:** WCL (`editor-ui/` consumes `@forge/{ui,code,tokens}` as git-subdir deps)

---

## TL;DR

`packages/ui`'s declaration pass (`tsc -p tsconfig.build.json`) fails with **TS2883** when
the package is prepared **standalone from a git tarball**, which is how external consumers
install it. Forge's own `pnpm build` / test suite pass, because in the monorepo turbo and a
hoisted `node_modules` satisfy assumptions that do not hold outside it.

The fix is almost certainly explicit `JSX.Element` return annotations in
`packages/ui/src/internal/icons.tsx` — the annotation TS2883 is literally asking for.

## Why forge's own CI is green

This is the important part. **Do not try to reproduce with `pnpm build` in the monorepo — it
will pass.** Two monorepo-only conditions are doing the work:

1. **turbo orders the builds.** `packages/ui/tsconfig.build.json` clears `paths` so
   declaration emit resolves workspace deps through `node_modules` to a sibling's built
   `dist/index.d.ts`. Its own comment states the assumption:

   > `turbo's dependsOn: ["^build"] guarantees it exists`

   When pnpm prepares a git-subdir dep, **turbo never runs**. `scripts/prepare-package.mjs`
   invokes `tsup && tsc -p tsconfig.build.json` directly, so nothing guarantees a sibling's
   `dist/` exists or is complete.

2. **Dependency layout differs.** In the workspace, `solid-js` resolves through a stable
   hoisted path. In a standalone prepare it resolves to pnpm's isolated store path
   (`.pnpm/solid-js@1.9.14/node_modules/solid-js`), which TypeScript refuses to reference
   from emitted declarations.

## The failure

From WCL's CI (`ubuntu-latest`, Node 22, pnpm 10, clean checkout, cold store):

```
.../packages/ui pnpm-install: . prepare: src/internal/icons.tsx(4,14): error TS2883:
  The inferred type of 'MenuSvg' cannot be named without a reference to
  'Element' from '.pnpm/solid-js@1.9.14/node_modules/solid-js'.
  This is likely not portable. A type annotation is necessary.
```

Same error for all nine exports in that file:

| line | export |
|---|---|
| 4  | `MenuSvg` |
| 11 | `XSvg` |
| 18 | `CheckMark` |
| 25 | `CheckDash` |
| 32 | `ChevronDown` |
| 39 | `ChevronLeftSvg` |
| 46 | `ChevronRightSvg` |
| 53 | `SearchSvg` |
| 60 | `CalendarSvg` |

Then:

```
.../packages/ui pnpm-install: . prepare: Failed
 ELIFECYCLE  Command failed with exit code 1.
ERR_PNPM_PREPARE_PACKAGE  Failed to prepare git-hosted package fetched from
  "https://codeload.github.com/wiltaylor/forge/tar.gz/4e120e38..." :
  @forge/ui@0.1.0 pnpm-install: `pnpm install`
```

## Root cause

`packages/ui/package.json` build is now split (from `chore(deps): update npm workspace to
latest`):

```json
"build": "tsup && tsc -p tsconfig.build.json"
```

with `tsup.config.ts` setting `dts: false` and the new `tsconfig.build.json` doing
`declaration: true` + `emitDeclarationOnly: true` + `paths: {}`. The comments explain this was
needed because `rollup-plugin-dts` caps at TypeScript 6 while forge moved to **TypeScript 7**.

That split is fine in principle. The problem is that declaration emit is far stricter about
*naming* inferred types than the old bundled-dts path was. `packages/ui/src/internal/icons.tsx`
exports arrow components with **inferred** return types:

```tsx
export const MenuSvg = () => (
  <svg …>…</svg>
);
```

To emit a `.d.ts`, TS must write that return type — `JSX.Element` from `solid-js`. With
pnpm's isolated layout there is no portable path to it, so TS2883 fires instead.

## Suggested fix (preferred)

Annotate the return types explicitly. This is portable, local, and is exactly what the
compiler asks for:

```tsx
import type { JSX } from 'solid-js';

export const MenuSvg = (): JSX.Element => (
  <svg …>…</svg>
);
```

Apply to all nine exports in `packages/ui/src/internal/icons.tsx`.

**Expect more files.** `tsc` reports per-file and the build stops at the first failure, so once
`icons.tsx` is clean, other modules with inferred-return exported components will likely
surface the same error. A full sweep for exported arrow components without explicit return
types is worthwhile — as is any exported symbol whose inferred type reaches into a dependency.

### Alternatives, if annotating is impractical

- Add `"declarationMap": false` + explicit `paths` in `tsconfig.build.json` mapping `solid-js`
  to a stable location. Fragile — it re-introduces a layout assumption.
- Keep the peer type reachable by declaring `solid-js` a real `dependency` rather than only a
  peer/dev dep. Undesirable for a UI library (risks duplicate solid instances).

Annotating is the one that does not trade one layout assumption for another.

## Reproduction

The only reliable repro is a **standalone consumer install**, not a monorepo build:

```bash
mkdir /tmp/forge-repro && cd /tmp/forge-repro
npm init -y
cat > package.json <<'JSON'
{
  "name": "forge-repro", "private": true, "type": "module",
  "dependencies": {
    "@forge/ui": "github:wiltaylor/forge#4e120e38f17bd90787b69feb17d614e1b3192ab3&path:packages/ui"
  },
  "pnpm": {
    "onlyBuiltDependencies": [
      "@forge/ui@https://codeload.github.com/wiltaylor/forge/tar.gz/4e120e38f17bd90787b69feb17d614e1b3192ab3#path:packages/ui"
    ]
  }
}
JSON
pnpm install --store-dir /tmp/forge-repro-store   # cold store
```

Notes on reproducing:

- **Use a cold store** (`--store-dir` to a fresh path). A warm store can replay a previously
  prepared package and mask the failure.
- **It is load-order sensitive.** This did *not* reproduce on a dev machine (Node 26) but failed
  deterministically on CI (Node 22, ubuntu-latest) across repeated runs. If a local run passes,
  that is not evidence the bug is absent. Prefer CI, or match Node 22.

## Related: the previous commit failed the same way, different symptom

At `2d81f992` (the commit before this one), the same standalone-prepare path failed as
**TS2305** instead — `@forge/chat`'s dts step could not see `@forge/ui`'s exports:

```
src/index.tsx(13,10): error TS2305: Module '"@forge/ui"' has no exported member 'parseMarkdown'
                                                                                'safeUrl'
src/index.tsx(14,15): error TS2305: … 'MdBlock' / 'MdInline' / 'MdListItem'
```

Those symbols *do* exist (`@forge/ui`'s `index.tsx` has `export * from './md'`); `chat` simply
typechecked before `@forge/ui`'s `dist/index.d.ts` was written. Same missing guarantee — turbo's
`^build` ordering not applying outside the monorepo.

So this is not a one-off regression but a **structural gap**: nothing in forge's CI exercises the
standalone git-subdir prepare that external consumers actually use. Worth adding the repro above
as a CI job — it would have caught both commits.

## Impact on WCL

`crates/wcl/build.rs` runs `pnpm install` for `editor-ui/` as part of `cargo build`, so a forge
commit that fails to prepare **breaks `cargo build` outright**, not just the frontend. WCL is
currently pinned back to `0365090d` and its pre-release is blocked on this.
