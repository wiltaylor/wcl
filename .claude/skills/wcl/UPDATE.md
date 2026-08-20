# Update: wcl

<purpose>
This skill mirrors the behaviour of the crates in this repository, so it goes stale on the
commit, not on a release. What drifts fastest are the closed lists it promises are closed —
every builtin, every host decorator, every `wcl` subcommand and flag, every wdoc block kind —
because adding one is a one-file change in `crates/` that nothing forces back into
`references/`. Up to date means: every member of every closed list appears in the reference
that owns it, and every behavioural claim still holds against a binary built from `HEAD`.
</purpose>

<baseline>
The commit this skill was last checked against: `none`

`none` means no check has been recorded, so step 3 runs over the whole source tree rather
than a range of commits. Step 8 rewrites this line.
</baseline>

<sources>
The crates are the source of truth. `docs/reference/` is **not** a source — it is an
independent tree that answers to `crates/` in parallel with this one, and copying between
them propagates errors instead of catching them.

- `crates/wcl_lang/src/functions/*.rs` — the builtins. One `register` per family; each
  `add_builtin` call carries the name, the `.doc()`, the `.param()` list and the `.returns()`
  that `lang_builtins.md` restates.
- `crates/wcl/src/main.rs` — the CLI. The `Command` and `WdocCommand` clap enums, their
  `#[arg]` attributes and their doc comments are what `lang_cli.md` documents. The exit-code
  constants live in the same file.
- `crates/wcl_wdoc/lib/*.wcl` — the wdoc stdlib, one file per block family. Each `@block("…")`
  type is a kind an author can write, and its `@doc` strings are the field documentation.
- `crates/wcl_lang/src/` outside `functions/` — parser, evaluator, schema checking and the
  diagnostics. Behaviour changes here land in the `lang_*.md` chapters.
- `crates/wcl_wdoc/src/` — the html, markdown, pdf and svg backends. What each one can and
  cannot draw is `wdoc_outputs.md` and `wdoc_visibility.md`.
- `target/release/wcl` built from `HEAD` — the last word on anything the source leaves
  ambiguous. Run the thing rather than reading about it.
</sources>

<questions>
| ID | Question | Default |
|----|----------|---------|
| Q1 | Cover the whole skill, or only the references touched by the commit range? | Whole skill: run the closed-list sweep every time, and the commit range on top of it |
| Q2 | A block kind or builtin exists in `crates/` but no reference documents it. Add a section, or report it? | Add a section to the reference that owns the family, written to match its neighbours |
| Q3 | A reference documents something `crates/` no longer has. Delete the section, or leave it with a note? | Delete it. A closed list that lists a member the binary rejects is worse than a short list |
| Q4 | The commit range changed a public behaviour with no reference chapter of its own. Where does it go? | Into the existing chapter whose subject it belongs to. Do not add a chapter or change the router |
</questions>

<procedure>
<step order="1">
Build the binary, so every later step reads `HEAD` rather than a stale artifact:

```sh
cargo build --release -p wcl
```

`target/release/wcl --version` should now report the workspace version. Everything below calls
this binary; a `wcl` already on `PATH` may be older.
</step>

<step order="2">
Read `<baseline>` above. It decides the shape of step 3 and nothing else — step 4 runs either
way.
</step>

<step order="3">
**When the baseline is a commit**, list what changed under the crates since it:

```sh
git log --oneline <baseline>..HEAD -- crates/
git diff --stat <baseline>..HEAD -- crates/
```

Read the diff for each commit that touches a path in `<sources>`. Skip commits confined to
tests, benches or `fuzz/`. For each behavioural change, name the reference file it belongs in
before moving on; a change you cannot place is Q4.

**When the baseline is `none`**, there is no range, so sweep the source tree instead: read
`crates/wcl/src/main.rs` end to end, every `crates/wcl_lang/src/functions/*.rs`, and the
`@doc` strings of every `crates/wcl_wdoc/lib/*.wcl`. This is the slow path and it is meant to
be — it is the only pass that catches something the skill never documented in the first place.

Ends when every changed or unread behaviour has a reference file named against it.
</step>

<step order="4">
Sweep the closed lists. Each command enumerates one from `HEAD`; each list must appear, member
for member, in the reference named beside it. Run these every time, whatever the baseline says.

Builtins — `references/language/lang_builtins.md`:

```sh
cat > /tmp/wcl-probe.wcl <<'EOF'
@document
type R { builtins: list<utf8> }

builtins = builtin_names()
EOF
target/release/wcl get /tmp/wcl-probe.wcl builtins --json
```

Host decorators — `references/language/lang_decorators.md` and `lang_schemas.md`. `wcl parse`
prints the prelude ahead of the document, and the prelude is where the host declares them:

```sh
: > /tmp/wcl-empty.wcl
target/release/wcl parse /tmp/wcl-empty.wcl | grep -oP '@decorator\("\K[^"]+' | sort -u
```

wdoc block kinds — the `references/wdoc/` chapter for the family. The leading-whitespace anchor
matters: `@block(` also appears inside `//` comment examples, and those are not real kinds:

```sh
grep -rhoP '^\s*@block\("\K[^"]+' crates/wcl_wdoc/lib/*.wcl | sort -u
```

Built-in themes — `references/wdoc/wdoc_styling.md`:

```sh
grep -rhoP '^\s*theme \K[a-z_]+' crates/wcl_wdoc/lib/theme.wcl
```

Subcommands, flags and exit codes — `references/language/lang_cli.md`:

```sh
target/release/wcl help
for c in parse check eval set fmt repl lsp init wdoc diff; do target/release/wcl help "$c"; done
target/release/wcl help wdoc build
target/release/wcl help wdoc serve
```

Ends when each diff is empty, or the gap is written into the reference that owns it.
</step>

<step order="5">
Apply the changes, one reference file at a time. A reference chapter is self-contained by
design: it does not send the reader to a sibling, so a fact that belongs in two chapters is
written into both rather than cross-linked. Match the surrounding voice and the density of
worked examples already in the file.
</step>

<step order="6">
Run every example you added or changed. Write it to a scratch `.wcl` file and put it through
the binary — `target/release/wcl check`, `wcl get`, or `wcl wdoc build --out` into a temp
directory for a wdoc block. Both documentation trees state that their examples were run, so an
example that has not been run cannot land.
</step>

<step order="7">
Check the router. If step 3 or step 4 added a section, `SKILL.md` still has to point at the
file that now holds it — the entry's one-line summary is what an agent chooses on. Adding a
*chapter* is out of scope; correcting the summary of one that already exists is not.
</step>

<step order="8">
Rewrite the `<baseline>` line above with the commit this pass checked:

```sh
git rev-parse --short HEAD
```

Ends with `<baseline>` naming that commit, so the next update reads a range instead of the
whole tree.
</step>
</procedure>

<verification>
Re-run every command in step 4 and diff each list against its reference file again. An empty
diff on all five is the bar.

Then re-run the examples from step 6 and confirm each still exits 0.

`.claude/skills/wcl/test.py` is the end-to-end check. Its second half has an agent write a
connection graph, a wdoc page and an awkwardly-quoted string from the references alone, then
puts each through the binary built in step 1 — so a reference that now teaches something the
binary rejects fails a test rather than sitting there. It will not catch a *missing* entry in
a closed list; step 4 is what does that.

Every run spends real model calls, so leave it to the user to say when.
</verification>

<out-of-scope>
An update refreshes what the skill knows. It does not:

- Add, remove or rename a reference file, or change the router's structure. A new chapter is
  `/meta-skill audit`, not `update`.
- Change the frontmatter `description`, or the `<overview>`, `<variables>` and `<boundaries>`
  sections of `SKILL.md`.
- Touch `docs/reference/`. That tree is independent of this one and is updated on its own,
  against `crates/`.
- Change `crates/` to match a reference. When the source and the skill disagree, the source
  wins and the skill is what gets edited — unless the source is the bug, which is a separate
  change with its own commit.
</out-of-scope>
