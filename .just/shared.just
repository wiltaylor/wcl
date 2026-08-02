# Recipes shared between the root justfile and the `ci` module.
#
# A just module cannot see its parent's recipes, so every recipe the merge bar
# (`.just/ci/mod.just`) depends on lives here and is imported by BOTH the root
# justfile and the module. Each recipe is still defined exactly once, and both
# `just workspace-test` and `just ci::workspace-test` reach it.
#
# Keep this file to the gate's constituents (and their private helpers).
# Everything else belongs in the root justfile.

# Defines a shell function that prints one heredoc's body from a scaffold
# template. Open a recipe line with `{{heredoc}};` then call
# `heredoc <file> <TERM>` as often as you like.
#
# The scaffold heredocs are the canonical copies of the wskill/WAD schemas and
# templates, so both the sync and the check recipes need to read one out.
# Shared as a *command* rather than as a recipe because a recipe can only be
# consumed for its stdout by shelling out to `just` — which norm 7 forbids and
# which, from inside the ci module, would silently resolve against the PARENT
# justfile. Empty output is a hard error: otherwise a renamed terminator would
# leave every drift check comparing nothing against nothing, and passing.
heredoc := "heredoc() { out=$(sed -n \"/<<'$2'/,/^$2$/p\" \"$1\" | sed '1d;$d'); [ -n \"$out\" ] || { echo \"no heredoc $2 in $1\" >&2; return 1; }; printf '%s\\n' \"$out\"; }"

# The scaffold templates whose heredocs are canonical
wskill_template := "crates/wcl/src/scaffold/templates/wskill.wcl"
wad_template := "crates/wcl/src/scaffold/templates/wad.wcl"

# Prefix for a heavy gate command, so only ONE HEAVY COMMAND runs at a time
# machine-wide: `bash "{{ci_lock}}" cargo test --workspace`. Many agents work
# this repo at once, each in its own worktree with its own target/, and every
# heavy part already fans out across every core — run eight at once and the box
# oversubscribes, making the whole fleet slower than if it had queued.
#
# One heavy command, not one whole gate: two gates DO interleave part by part
# (and command by command within a part, which is why a contended run can print
# the waiting note more than once). That is the intended trade — the parts are
# independent, and either way only one of them is burning the machine.
#
# The lock goes on the PARTS, never on `check`. Two reasons: `check` is a pure
# dependency list and just runs each dependency in its own shell, so a lock in
# its body would be taken after every part had already finished; and CLAUDE.md
# tells contributors to run `just workspace-test` / `just workspace-lint`
# directly, which is where most of the real contention is. Never do both —
# flock is not reentrant across processes, so a lock on `check` AND on its
# parts would self-deadlock on the first part.
#
# Heavy means "burns the machine": it compiles, fans out across every core, or
# walks the whole tree. `cargo fmt` (0.7s, single-core) and the pure-bash drift
# checks are left unlocked — they oversubscribe nothing, so queueing them behind
# a test run would only add latency. Every part of `check` that does real work
# is covered; the ONE thing to remember when adding a heavy recipe is to prefix
# it, because nothing enforces that (norm 7 rules out a wrapper recipe).
#
# Invoked as `bash <script>`, not as an executable: a checkout that lost the
# exec bit (Windows, an unpacked archive) would otherwise fail every heavy part
# with "Permission denied" — a hard gate failure from the very thing that is
# supposed to degrade quietly. The gate already runs under bash (`set shell`).
#
# Bash rather than norm 12's uv-python, deliberately: `uv` is currently required
# by exactly one part (wad-extract), and a python wrapper would make it a hard
# dependency of `fmt-check`, `workspace-test` and the rest. The lock also has to
# hold an fd across an `exec` so it covers the command's whole life, which is a
# shell idiom. The script stays small and flat to earn that.
#
# SIDE EFFECT, and the deliberate exception to norm 14's contract item 4: a
# lock file outside the checkout — ${XDG_RUNTIME_DIR:-/tmp}/wcl-ci.lock, plus a
# .holder record beside it (and a .noflock warn-stamp on a box without flock).
# Outside is the whole point: the contention is between separate worktrees, and
# a lock file under one of them locks nothing. It is per-user session, not
# literally machine-wide — a run under cron or `docker exec` may see no
# XDG_RUNTIME_DIR and fall back to /tmp, which is a different lock; set
# WCL_CI_LOCK_FILE to a fixed path if that ever matters here.
# WCL_CI_LOCK_DISABLE=1 opts out entirely; WCL_CI_LOCK_TIMEOUT bounds the wait.
#
# Absolute, not repo-relative: `fuzz-sweep` runs cargo from crates/wcl_lang.
ci_lock := justfile_directory() / ".just/ci-lock.sh"

# ── External tooling guards ───────────────────────────────────────────────
# So a missing tool reports as a missing tool rather than as a confusing
# build error. pnpm/node are deliberately absent: crates/wcl's build.rs
# already reports their absence itself (and offers WCL_EDITOR_UI_SKIP=1).

[private]
require-cargo:
    @command -v cargo >/dev/null 2>&1 || { echo "cargo is required — install Rust via https://rustup.rs" >&2; exit 1; }

[private]
require-git:
    @command -v git >/dev/null 2>&1 || { echo "git is required — https://git-scm.com" >&2; exit 1; }

[private]
require-uv:
    @command -v uv >/dev/null 2>&1 || { echo "uv is required to run the .wad extractor scripts — https://docs.astral.sh/uv/" >&2; exit 1; }

[private]
require-rustfmt: require-cargo
    @cargo fmt --version >/dev/null 2>&1 || { echo "the rustfmt component is required — run 'rustup component add rustfmt'" >&2; exit 1; }

[private]
require-clippy: require-cargo
    @cargo clippy --version >/dev/null 2>&1 || { echo "the clippy component is required — run 'rustup component add clippy'" >&2; exit 1; }

[private]
require-cargo-fuzz: require-cargo
    @rustc +nightly -vV >/dev/null 2>&1 || { echo "the nightly toolchain is required — run 'rustup toolchain install nightly'" >&2; exit 1; }
    @command -v cargo-fuzz >/dev/null 2>&1 || { echo "cargo-fuzz is required — run 'cargo +nightly install cargo-fuzz'" >&2; exit 1; }

# ── Quality ───────────────────────────────────────────────────────────────

# Check formatting without modifying files
[group('quality')]
fmt-check: require-rustfmt
    cargo fmt --all -- --check

# Lint with clippy (warnings are errors)
[group('quality')]
workspace-lint: require-clippy
    bash "{{ci_lock}}" cargo clippy --workspace --all-targets -- -D warnings

# Run all tests (unit + integration)
[group('test')]
workspace-test: require-cargo
    bash "{{ci_lock}}" cargo test --workspace

# Bounded fuzz sweep across every target (~15s each, ~75s total)
[group('test')]
fuzz-sweep: require-cargo-fuzz
    @host="$(rustc +nightly -vV | sed -n 's/^host: //p')"; \
    for t in parse eval format_round_trip json_round_trip set_edit_path; do \
        echo "==> fuzz $t" >&2; \
        (cd crates/wcl_lang && bash "{{ci_lock}}" cargo +nightly fuzz run --target "$host" "$t" -- -runs=2000 -max_total_time=15) || exit 1; \
    done

# Fail when a live wskill base.wcl drifts from the scaffold heredoc (runs in ci)
[group('quality')]
wskill-schema-check:
    @{{heredoc}}; for d in docs/wskills/*; do \
        [ -d "$d/schema" ] || continue; \
        diff <(heredoc {{wskill_template}} WSK_SCHEMA_BASE_WCL) "$d/schema/base.wcl" >/dev/null \
            || { echo "wskill schema drift: $d/schema/base.wcl — run 'just wskill-schema-sync'"; exit 1; }; \
        [ ! -f "$d/schema/training.wcl" ] || \
            diff <(heredoc {{wskill_template}} WSK_SCHEMA_TRAINING_WCL) "$d/schema/training.wcl" >/dev/null \
            || { echo "wskill schema drift: $d/schema/training.wcl — run 'just wskill-schema-sync'"; exit 1; }; \
    done

# The templates are topic-agnostic, so improvements to the meta wskill
# (docs/wskills/wskill) must be back-ported or new scaffolds strand on stale
# copies. wdoc/skill/main.wcl is exempt: its skill description is topic-tuned
# in the live instance.

# Fail when the wskill scaffold's wdoc templates drift from the live reference implementation (runs in ci)
[group('quality')]
wskill-template-check:
    @{{heredoc}}; for pair in \
        WSK_WDOC_COMPONENT_COMMON_WCL:component/common.wcl \
        WSK_WDOC_COMPONENT_SKILL_MD_WCL:component/skill_md.wcl \
        WSK_WDOC_COMPONENT_CONCEPT_WCL:component/concept.wcl \
        WSK_WDOC_COMPONENT_ENTITY_WCL:component/entity.wcl \
        WSK_WDOC_COMPONENT_FACT_WCL:component/fact.wcl \
        WSK_WDOC_COMPONENT_RESEARCH_WCL:component/research.wcl \
        WSK_WDOC_COMPONENT_PROCESS_WCL:component/process.wcl \
        WSK_WDOC_COMPONENT_TYPE_INDEX_WCL:component/type_index.wcl \
        WSK_WDOC_PAGES_OVERVIEW_WCL:pages/overview.wcl \
        WSK_WDOC_PAGES_CONCEPTS_WCL:pages/concepts.wcl \
        WSK_WDOC_PAGES_ENTITIES_WCL:pages/entities.wcl \
        WSK_WDOC_PAGES_FACTS_WCL:pages/facts.wcl \
        WSK_WDOC_PAGES_PROCESSES_WCL:pages/processes.wcl \
        WSK_WDOC_BOOK_MAIN_WCL:book/main.wcl \
    ; do \
        term="${pair%%:*}"; path="${pair#*:}"; \
        diff <(heredoc {{wskill_template}} "$term" | sed -z 's/\n*$/\n/') \
             <(sed -z 's/\n*$/\n/' "docs/wskills/wskill/wdoc/$path") >/dev/null \
            || { echo "wskill template drift: docs/wskills/wskill/wdoc/$path vs scaffold heredoc $term — back-port one side"; exit 1; }; \
    done

# The reference is docs/wskills/wskill/wdoc/. Per-topic exemptions cover
# verified intentional divergence: skill/main.wcl everywhere (topic-tuned
# description); wcl's common.wcl (builtins fn_signature superset), overview,
# book (builtins/CLI projections); wdoc's book (index-nav TOC architecture).
# Files a topic doesn't ship (wad: presentation/training) are skipped — a
# deleted copy breaks that topic's book imports, which docs-build catches.

# Fail when a topic's copy of a topic-agnostic wdoc template drifts from the reference (runs in ci)
[group('quality')]
wskill-crosstopic-check:
    @fail=0; \
    exempt_wcl="component/common.wcl pages/overview.wcl book/main.wcl"; \
    exempt_wdoc="book/main.wcl"; \
    exempt_wad=""; \
    ref=docs/wskills/wskill/wdoc; \
    for d in docs/wskills/*; do \
        t=$(basename "$d"); [ "$t" = wskill ] && continue; [ -d "$d/wdoc" ] || continue; \
        eval "exempt=\"\${exempt_$t:-} skill/main.wcl\""; \
        for f in $(cd "$ref" && find . -type f -name '*.wcl' | sed 's|^\./||' | sort); do \
            case " $exempt " in *" $f "*) continue;; esac; \
            [ -f "$d/wdoc/$f" ] || continue; \
            diff "$ref/$f" "$d/wdoc/$f" >/dev/null \
                || { echo "cross-topic template drift: $d/wdoc/$f vs $ref/$f — sync from the reference (or add an exemption here if newly topic-tuned)"; fail=1; }; \
        done; \
    done; \
    [ "$fail" -eq 0 ] && echo "wskill-crosstopic-check OK"; \
    exit $fail

# Entry paths are metadata (the registry landing and per-wskill justfiles
# enumerate them), so a rename or deletion would otherwise dangle silently.

# Fail when a wskill declares an `artifact` whose entry file does not exist (runs in ci)
[group('quality')]
wskill-artifact-check:
    @fail=0; \
    for d in docs/wskills/*; do \
        [ -f "$d/wskill.wcl" ] || continue; \
        for e in $(grep -oE 'entry *= *"[^"]+"' "$d/wskill.wcl" | sed 's/.*"\(.*\)"/\1/'); do \
            [ -f "$d/$e" ] || { echo "dangling artifact entry: $d/wskill.wcl declares $e but $d/$e does not exist"; fail=1; }; \
        done; \
    done; \
    [ "$fail" -eq 0 ] && echo "wskill-artifact-check OK"; \
    exit $fail

# Covers four failures: the projection breaking; the committed artifacts
# (.claude/skills/<name>/, .claude/agents/*.md) drifting from their wskill
# sources; two wskills declaring the same agent name (the flat install copy
# would silently clobber); and a committed generated skill/agent with no
# producing wskill. Discovery and name extraction match skills-install.
# Hand-authored skills (no wskill_schema_version metadata in SKILL.md) and
# .claude/agents/README.md are exempt.

# Fail when a skill projection breaks or the committed .claude/ artifacts drift from their wskill sources (runs in ci)
[group('quality')]
skills-check: require-cargo
    @rm -rf target/skills-check && mkdir -p target/skills-check
    @fail=0; agent_names=""; skill_names=""; \
    for d in docs/wskills/*; do \
        [ -f "$d/wdoc/skill/main.wcl" ] || continue; \
        topic=$(basename "$d"); \
        stage="target/skills-check/$topic"; \
        echo "==> $topic" >&2; \
        bash "{{ci_lock}}" cargo run -q -p wcl -- wdoc skill "$d/wdoc/skill/main.wcl" --out "$stage" \
            || { echo "skill projection FAILED: $d/wdoc/skill/main.wcl"; exit 1; }; \
        for md in "$stage/SKILL.md" "$stage"/*/SKILL.md; do \
            [ -f "$md" ] || continue; \
            sd=$(dirname "$md"); \
            sn=$(sed -n 's/^name: *//p' "$md" | head -1 | sed 's/^"//; s/"$//'); \
            skill_names="$skill_names $sn"; \
            diff -r "$sd" ".claude/skills/$sn" >/dev/null 2>&1 \
                || { echo "skill artifact drift: .claude/skills/$sn vs $d — run 'just skills-install' and commit"; \
                     diff -r "$sd" ".claude/skills/$sn" 2>&1 | head -20; fail=1; }; \
        done; \
        if [ -d "$stage/agents" ]; then \
            for a in "$stage/agents/"*.md; do \
                an=$(basename "$a" .md); \
                case " $agent_names " in *" $an "*) \
                    echo "agent name collision: $an declared by more than one wskill"; fail=1;; \
                esac; \
                agent_names="$agent_names $an"; \
                diff "$a" ".claude/agents/$an.md" >/dev/null 2>&1 \
                    || { echo "agent artifact drift: .claude/agents/$an.md vs $d — run 'just skills-install' and commit"; fail=1; }; \
            done; \
        fi; \
    done; \
    for smd in .claude/skills/*/SKILL.md; do \
        [ -f "$smd" ] || continue; \
        grep -q 'wskill_schema_version' "$smd" || continue; \
        sn=$(basename "$(dirname "$smd")"); \
        case " $skill_names " in *" $sn "*) ;; *) \
            echo "stale generated skill: .claude/skills/$sn — no wskill produces it; delete it or restore its source"; fail=1;; \
        esac; \
    done; \
    for a in .claude/agents/*.md; do \
        an=$(basename "$a" .md); [ "$an" = README ] && continue; \
        case " $agent_names " in *" $an "*) ;; *) \
            echo "stale agent: $a — no wskill produces it; delete it or restore its source"; fail=1;; \
        esac; \
    done; \
    [ "$fail" -eq 0 ] && echo "skills-check OK — projections build clean and match committed artifacts"; \
    exit $fail

# Two comparisons: the documented builtin names vs `builtin_names()`, and the
# documented subcommand tree vs `wcl --help` / `wcl wdoc --help` /
# `wcl wad --help`. The wskill IS the WCL language/CLI/builtin reference, so a
# new builtin or subcommand must land in it or the docs quietly go stale.
#
# `host_builtins` are the ones the CLI's *host* environment registers, not the
# language: `wcl` opens every document with `wcl_wdoc::wdoc_environment` (that
# is what supplies the expander `@contextual` block kinds need), so wdoc's own
# builtins are callable too. They're documented with wdoc, not here — a new one
# fails this check until it is either documented in the wcl wskill or added to
# this list as wdoc's. One name per line: `grep -vxF` takes a multi-line pattern
# argument as one pattern per line, so a second entry must be a real newline.
host_builtins := "included_sites"

# Fail when the `wcl` wskill's builtin/subcommand reference drifts from the binary it documents (runs in ci)
[group('quality')]
wcl-refcheck: require-cargo
    @mkdir -p target/refcheck
    @printf '@document\ntype D {\n  names: list<utf8>\n}\n\nnames = builtin_names()\n' > target/refcheck/builtins.wcl
    @bash "{{ci_lock}}" cargo run -q -p wcl -- eval target/refcheck/builtins.wcl names \
        | tr -d '[]" ' | tr ',' '\n' | sed '/^$/d' \
        | grep -vxF -e '{{host_builtins}}' | sort > target/refcheck/builtins-real.txt
    @grep -oE '^  name *= *"[^"]+"' docs/wskills/wcl/data/builtins.wcl \
        | sed 's/.*"\(.*\)"/\1/' | sort > target/refcheck/builtins-doc.txt
    @diff target/refcheck/builtins-real.txt target/refcheck/builtins-doc.txt \
        || { echo "wcl reference drift: builtins (< missing from docs/wskills/wcl/data/builtins.wcl, > documented but not a builtin)"; exit 1; }
    @subs() { bash "{{ci_lock}}" cargo run -q -p wcl -- $1 --help 2>&1 \
        | sed -n '/^Commands:/,/^$/p' | sed -n 's/^  \([a-z][a-z-]*\) .*/\1/p' | grep -v '^help$'; }; \
    { subs ""; subs wdoc | sed 's/^/wdoc /'; subs wad | sed 's/^/wad /'; \
      subs wskill | sed 's/^/wskill /'; } \
        | sort > target/refcheck/commands-real.txt
    @printf '{ let c = head(clis); join(flatten(map(c.subcommands, fn(s: Subcommand) -> list<utf8> { flatten([[s.name], map(s.subcommands, fn(n: Subcommand) -> utf8 { $"${s.name} ${n.name}" })]) })), "\\n") }\n' \
        | bash "{{ci_lock}}" cargo run -q -p wcl -- repl docs/wskills/wcl/wskill.wcl \
        | sed 's/^"//; s/"$//' | sed 's/\\n/\n/g' | sort > target/refcheck/commands-doc.txt
    @diff target/refcheck/commands-real.txt target/refcheck/commands-doc.txt \
        || { echo "wcl reference drift: CLI subcommands (< missing from docs/wskills/wcl/data/reference/cli.wcl, > documented but not a subcommand)"; exit 1; }
    @echo "wcl-refcheck OK — $(wc -l < target/refcheck/builtins-real.txt) builtins, $(wc -l < target/refcheck/commands-real.txt) subcommands match the binary"

# Fail when .wad/schema/base.wcl drifts from the scaffold heredoc (runs in ci)
[group('quality')]
wad-schema-check:
    @{{heredoc}}; diff <(heredoc {{wad_template}} WAD_SCHEMA_BASE_WCL) .wad/schema/base.wcl >/dev/null \
        || { echo "wad schema drift: .wad/schema/base.wcl — run 'just wad-schema-sync'"; exit 1; }

# Validate the WCL architecture document (.wad/) — model + book template
[group('quality')]
wad-check: require-cargo
    bash "{{ci_lock}}" cargo run -p wcl -- check .wad/wad.wcl
    bash "{{ci_lock}}" cargo run -p wcl -- check .wad/wdoc/book/main.wcl

# Needs uv and full git history. releases.wcl is exempt: it derives from git
# tags, and the release pipeline creates a tag AFTER the commit CI builds from,
# so it can never be fresh at gate time — the keep-current sweep picks new
# releases up instead.
#
# NOT side-effect-free, unlike the rest of the gate: it re-runs the extractors,
# so it REWRITES the tracked files under .wad/data/generated/ (leaving them
# rewritten when it fails), and restores releases.wcl with `git checkout`,
# discarding uncommitted edits to that one file. Freshness can't be judged
# without regenerating, and the git tree is the comparison.

# Fail when .wad/data/generated/ is stale by re-running every extractor — rewrites those tracked files (runs in ci)
[group('quality')]
wad-extract-check: require-git wad-extract
    @if [ -n "$(git status --porcelain -- .wad/data/generated ':(exclude).wad/data/generated/releases.wcl')" ]; then \
        git --no-pager diff -- .wad/data/generated ':(exclude).wad/data/generated/releases.wcl' | head -80; \
        echo "wad generated-data drift: run 'just wad-extract' and commit the result"; \
        exit 1; \
    fi
    @git checkout -q -- .wad/data/generated/releases.wcl 2>/dev/null || true
    @echo "wad-extract-check OK — .wad/data/generated is fresh (releases.wcl exempt: tag-derived)"

# Fail when the wad wskill's hand-reflected fact tables lag the WAD schema version (runs in ci)
[group('quality')]
wad-facts-check:
    @V=$(grep -m1 'schema_version = ' crates/wcl/src/scaffold/templates/wad.wcl | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'); \
    grep -q "hand-reflected from schema $V" docs/wskills/wad/data/reference/facts.wcl \
        || { echo "wad wskill facts drift: docs/wskills/wad/data/reference/facts.wcl must say 'hand-reflected from schema $V' — re-reflect the fact tables against the current WAD schema, then update the header"; exit 1; }
    @echo "wad-facts-check OK"

# Scaffold the shipped wplan template into target/ and run its structural gates (runs in ci)
[group('quality')]
wplan-template-check: require-cargo
    rm -rf target/wplan-template-check
    bash "{{ci_lock}}" cargo run -p wcl -- init wplan target/wplan-template-check --defaults
    bash "{{ci_lock}}" cargo run -p wcl -- check target/wplan-template-check/plan.wcl
    bash "{{ci_lock}}" cargo run -p wcl -- check target/wplan-template-check/gates.wcl
    @for g in $(grep -oE '^gate [a-z_0-9]+' target/wplan-template-check/gates.wcl | cut -d' ' -f2 | grep -v '^signoffs_complete$'); do \
        printf 'gate %-24s ' "$g"; \
        bash "{{ci_lock}}" cargo run -q -p wcl -- eval target/wplan-template-check/gates.wcl "gates.$g.ok" || exit 1; \
    done

# ── Builds the gate depends on ────────────────────────────────────────────

# Build the project's docs/ site into docs/_site/ (gitignored)
[group('dev')]
docs-build *ARGS: require-cargo
    bash "{{ci_lock}}" cargo run -p wcl -- wdoc build docs/main.wcl --out docs/_site {{ARGS}}

# The output dir is wiped first: a build never deletes pages that no longer
# exist, so removed entities would otherwise linger as orphaned HTML.

# Build the WCL architecture book (.wad/) into .wad/_site/ (gitignored)
[group('dev')]
wad-build *ARGS: require-cargo
    rm -rf .wad/_site
    bash "{{ci_lock}}" cargo run -p wcl -- wdoc build .wad/wdoc/book/main.wcl --out .wad/_site {{ARGS}}

# Run every WAD extractor script (rewrites .wad/data/generated/), then re-validate
[group('dev')]
wad-extract: require-uv && wad-check
    for s in .wad/scripts/extract_*.py; do echo "==> $s" >&2; bash "{{ci_lock}}" uv run "$s"; done
