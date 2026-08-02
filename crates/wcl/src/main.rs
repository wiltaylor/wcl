use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use wcl_lang::{
    Document, Environment, ParseError, format as wcl_format, parse_expr, parse_for_edit,
};

mod answer;
mod answer_tui;
mod diff;
mod dump;
mod edit;
mod editor;
mod gitspec;
mod preview;
mod scaffold;
mod serve;
mod wad;
mod wskill;

const EXIT_OK: u8 = 0;
const EXIT_PARSE: u8 = 1;
const EXIT_SCHEMA: u8 = 2;
const EXIT_EVAL: u8 = 3;
const EXIT_IO: u8 = 4;

/// Loader for every CLI document open: disk imports plus the embedded
/// wdoc registry, so `wcl check` / `parse` / `eval` / `get` resolve
/// `import <wdoc.wcl>` exactly like `wcl wdoc build` (previously they
/// failed with a misleading "failed to read '<wcl-system>/wdoc.wcl'",
/// leaving `wdoc build` as the only schema checker for wdoc projects).
fn cli_loader() -> wcl_lang::FileLoader {
    wcl_wdoc::schema_registry().loader(wcl_lang::disk_loader())
}

/// Environment for every CLI document open. The registry above supplies
/// wdoc's *schemas*; this supplies its *behaviour* — the expander that
/// `@contextual` block kinds (`wdoc_repeater`, component instances)
/// expand through. Without it, projecting their generated children is a
/// hard error, so every command that evaluates a wdoc document opens
/// through here rather than through a bare `Environment::new()`.
///
/// Unconditional, on purpose: the CLI does **not** sniff a document's
/// imports to decide whether it is "wdoc's". It is already
/// unconditionally wdoc-aware for schemas ([`cli_loader`] threads the
/// registry into every open, not just wdoc documents), and behaviour
/// follows schemas. The only widening is that wdoc's own builtins
/// (`included_sites`) are in scope everywhere — which turns what used to
/// be an "unknown builtin" error in a wdoc document under `wcl check`
/// into a working call.
fn cli_environment(base_dir: Option<&Path>) -> Environment {
    wcl_wdoc::wdoc_environment(base_dir)
}

fn open_document(file: &Path, profile: bool) -> Result<Document, ParseError> {
    let mut doc =
        Document::from_file_with_loader(file, &cli_environment(file.parent()), cli_loader())?;
    if profile {
        doc.enable_profiling();
    }
    Ok(doc)
}

fn emit_profile(doc: &Document, profile: bool) {
    if profile && let Some(p) = doc.profile() {
        let json = dump::profile_to_json(&p);
        let rendered = serde_json::to_string_pretty(&json)
            .expect("serde_json::Value always serializes (string-keyed objects)");
        eprintln!("{rendered}");
    }
}

#[derive(Parser)]
#[command(name = "wcl", version = env!("WCL_VERSION"), about = "WCL command-line interface")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a WCL file and print the resulting document tree (forces evaluation).
    Parse {
        /// Path to a WCL source file.
        file: PathBuf,
        /// Record a call-tree profile of the document forcing and print
        /// it as JSON to stderr after the dump.
        #[arg(long)]
        profile: bool,
    },
    /// Parse a WCL file and report whether it is syntactically valid.
    Check {
        /// Path to a WCL source file, or `-` to read from stdin
        /// (relative imports then resolve against the current
        /// directory).
        file: PathBuf,
        /// Emit the result as a JSON object on stdout (`ok`, `file`,
        /// `errors[]` with code / message / offset / length) instead
        /// of human-readable diagnostics. Exit codes are unchanged.
        #[arg(long)]
        json: bool,
    },
    /// Resolve a dotted path inside a WCL file and print the resulting value.
    /// Aliased as `wcl get`.
    ///
    /// Examples:
    ///   wcl get site.wcl name
    ///   wcl get site.wcl service.config.region
    #[command(alias = "get")]
    Eval {
        /// Path to a WCL source file.
        file: PathBuf,
        /// Dotted path to resolve from the document root.
        path: String,
        /// Record a call-tree profile of the evaluation and print it
        /// as JSON to stderr after the value is printed.
        #[arg(long)]
        profile: bool,
        /// Emit the resolved value as JSON instead of the WCL display
        /// form. Function values can't be serialized and are
        /// represented as `null`.
        #[arg(long)]
        json: bool,
    },
    /// Update the field at a dotted path. The value is parsed as a WCL
    /// expression — quote shell-special characters as needed.
    ///
    /// When `<path>` resolves through an import, `wcl set` follows the
    /// import chain and edits the file that actually declares the field
    /// (not necessarily the file you named).
    ///
    /// Examples:
    ///   wcl set site.wcl name '"alpha"'
    ///   wcl set site.wcl service.web.port 9090u32
    ///   wcl set site.wcl color :gold
    Set {
        /// Path to a WCL source file (entry point — imports are followed).
        file: PathBuf,
        /// Dotted path to the field whose value should be replaced.
        path: String,
        /// New value, written as a WCL expression. Strings, numbers
        /// with type suffixes, symbols, lists, etc. are all accepted.
        value: String,
    },
    /// Parse a WCL file and re-emit it in canonical form. Comments and
    /// blank-line groupings survive; indentation, brace style, number
    /// radix and string-delimiter choice are normalized.
    Fmt {
        /// Path to a WCL source file, or `-` to read from stdin and
        /// write the formatted source to stdout.
        file: PathBuf,
        /// Overwrite the file in place (atomically). Without this flag,
        /// the formatted source is written to stdout and the file on
        /// disk is left untouched.
        #[arg(long = "in-place")]
        in_place: bool,
        /// Spaces per indentation level. Defaults to the canonical
        /// formatter (2). Set higher for editor-style preferences.
        #[arg(long, default_value_t = 2)]
        indent: usize,
        /// Strip the trailing comma the formatter places after every
        /// `match` arm. Parser accepts either form.
        #[arg(long = "no-trailing-comma")]
        no_trailing_comma: bool,
    },
    /// Read-eval-print loop for ad-hoc WCL expressions. With a file
    /// argument, identifiers resolve against that file's top-level
    /// fields; without one, you can still evaluate self-contained
    /// expressions (arithmetic, string ops, builtin calls).
    ///
    /// EOF (Ctrl-D) or `:quit` exits. Interactive sessions always
    /// exit 0; when stdin is not a TTY the exit code reflects any
    /// errors that occurred during the session (1 for parse errors,
    /// 3 for eval errors), so piped scripts can detect failures.
    Repl {
        /// Optional WCL file whose top-level fields the REPL should
        /// resolve identifiers against.
        file: Option<PathBuf>,
    },
    /// Run the WCL language server. Defaults to stdio (the transport
    /// editors expect); `--tcp` switches to a TCP listener that
    /// accepts any number of connections, useful for attaching debug
    /// clients.
    Lsp {
        /// Listen on `host:port` for inbound TCP connections instead
        /// of using stdio. Each connection runs as an independent LSP
        /// session. Example: `--tcp 127.0.0.1:9257`.
        #[arg(long)]
        tcp: Option<std::net::SocketAddr>,
        /// Write `tracing` log lines to this file. The server never
        /// logs to stderr (that would corrupt the stdio LSP stream),
        /// so a file sink is the only supported destination.
        #[arg(long)]
        log: Option<PathBuf>,
    },
    /// Serve a browser-based editor for the current directory: a
    /// gitignore-aware file tree, CodeMirror editing with WCL language
    /// support and LSP (completion, hover, diagnostics), and a live wdoc
    /// preview built from the root document down.
    Editor {
        /// Root `.wcl` document (defaults to `./main.wcl` when present).
        /// The preview pane and schema-validated saves need it; plain
        /// editing works without one.
        root: Option<PathBuf>,
        /// Bind address, or `auto` to pick the first free port near 8080.
        #[arg(long, default_value = "auto")]
        addr: serve::BindSpec,
    },
    /// Scaffold a new project folder from a WCL template. `<template>`
    /// is a built-in name (`wcl init --list`), a user template under
    /// `$XDG_DATA_HOME/wcl/templates/<name>/template.wcl`, or a path to a
    /// template `.wcl` file (or a folder holding `template.wcl`); the
    /// template declares `property` questions plus the `file` / `folder`
    /// blocks to generate.
    ///
    /// Property answers come from `-D key=value`, an `--answers` file
    /// (`.wcl` or `.json`), an interactive prompt, or the property's
    /// default — in that order of precedence.
    ///
    /// Examples:
    ///   wcl init minimal ./my-project
    ///   wcl init minimal ./app -D name=app --defaults
    ///   wcl init ./my-template.wcl ./out --answers answers.json
    ///   wcl init --list
    Init {
        /// Built-in template name or path to a template `.wcl` file.
        /// Optional only with `--list`.
        template: Option<String>,
        /// Destination directory. Defaults to the answered `name`
        /// property, falling back to the template name.
        dest: Option<PathBuf>,
        /// Answer file (`.wcl` or `.json`) supplying property answers.
        #[arg(long)]
        answers: Option<PathBuf>,
        /// Supply a property answer inline (repeatable). Highest
        /// precedence. Example: `-D name=acme`.
        #[arg(short = 'D', value_name = "KEY=VALUE")]
        define: Vec<String>,
        /// Non-interactive: never prompt; use defaults for unanswered
        /// properties (error if one has no default).
        #[arg(long)]
        defaults: bool,
        /// Write into the destination even if it already exists and is
        /// not empty.
        #[arg(long)]
        force: bool,
        /// List the built-in templates and exit.
        #[arg(long)]
        list: bool,
    },
    /// WCL-driven static site generator. Use `wcl wdoc build` for a
    /// one-shot render and `wcl wdoc serve` for a watch-rebuild dev
    /// server.
    Wdoc {
        #[command(subcommand)]
        cmd: WdocCommand,
    },
    /// Compare two WCL documents and print the changed entities / fields.
    /// Operates on the *evaluated* document views (imports resolved), so a
    /// formatting-only edit produces no diff. Each top-level block is an
    /// entity keyed `kind:label`; nested field edits are reported by path,
    /// recursing into lists by index. Output is a re-parseable WCL tree by
    /// default (`--format json` for the flat change array).
    ///
    /// Either side may be a `<rev>:<path>` git specifier, whose imports
    /// resolve from that same revision.
    ///
    /// Examples:
    ///   wcl diff old.wcl new.wcl
    ///   wcl diff HEAD~1:config.wcl config.wcl
    ///   wcl diff main:a.wcl feature:a.wcl --format json
    Diff {
        /// Old (base) document — a path or `<rev>:<path>` git specifier.
        old: String,
        /// New document — a path or `<rev>:<path>` git specifier.
        new: String,
        /// Output format: `wcl` (default) or `json`.
        #[arg(long, value_enum, default_value_t = DiffFormat::Wcl)]
        format: DiffFormat,
    },
    /// WAD (architecture document) helpers. Scaffold a WAD with
    /// `wcl init wad`.
    Wad {
        #[command(subcommand)]
        cmd: WadCommand,
    },
    /// wskill helpers. Scaffold a wskill with `wcl init wskill`.
    Wskill {
        #[command(subcommand)]
        cmd: WskillCommand,
    },
    /// Walk a document's pending interview questions and record the answers —
    /// the respondent-facing counterpart to editing the WCL by hand. A block
    /// type opts in with the `@answerable` decorator from `import <answer.wcl>`,
    /// which maps the prompt / response / status roles onto its own fields
    /// (see `examples/answer/plan.wcl`). Choice questions render their option
    /// child blocks as an arrow-key menu (numbered line input when stdin isn't
    /// a TTY); a free-text answer is always available. Each answer writes back
    /// immediately through the validating edit pipeline, so an interrupted
    /// session loses nothing.
    ///
    /// Examples:
    ///   wcl answer plan.wcl
    ///   wcl answer plan.wcl --list
    ///   wcl answer plan.wcl --id q_platforms --pick linux --text "and CI runners"
    ///   wcl answer plan.wcl --id q_scope --skip
    Answer {
        /// Path to the WCL document (imports are followed; each answer lands
        /// in the file that declares its question).
        file: PathBuf,
        /// List the pending questions as JSON (id, prompt, kind, options,
        /// skippable) instead of prompting.
        #[arg(long)]
        list: bool,
        /// Answer one question non-interactively: the question block's label.
        #[arg(long)]
        id: Option<String>,
        /// Free-text answer for `--id` (may combine with `--pick`).
        #[arg(long, requires = "id")]
        text: Option<String>,
        /// Pick an option by its id for `--id` (repeatable).
        #[arg(long, requires = "id")]
        pick: Vec<String>,
        /// Skip the question for `--id`: writes its declared skipped status.
        #[arg(long, requires = "id", conflicts_with_all = ["text", "pick"])]
        skip: bool,
    },
}

#[derive(Subcommand)]
enum WskillCommand {
    /// Print the wskill's model — units, index trees, `related` and pin
    /// edges, per-unit block lists, and where each is written — as JSON on
    /// stdout. No editor, no build: the same model the browser editor's
    /// graph view draws and the curator audits.
    ///
    /// Examples:
    ///   wcl wskill graph
    ///   wcl wskill graph docs/wskills/wcl
    ///   wcl wskill graph docs/wskills/wcl --rev HEAD~1
    Graph {
        /// The wskill folder (or an entry `.wcl` inside it). Defaults to
        /// the current directory.
        entry: Option<PathBuf>,
        /// Read the model at this git revision instead of the working
        /// tree — the whole tree is materialized, so imports resolve as
        /// they did at that commit.
        #[arg(long)]
        rev: Option<String>,
    },
    /// Run every wskill rule over the model and report the findings —
    /// errors, warnings and curator candidates from one pass. Reads the
    /// data model only: no build, no editor, and lint never writes.
    ///
    /// Errors are mechanically certain (a `related` id naming nothing, an id
    /// declared twice, a link to a body-less index). Warnings carry a real
    /// exception rate (over-cap links, an unpinned unit, a unit no
    /// projection renders) and never fail on their own. Candidates are
    /// nominations to the curator and fail nothing.
    ///
    /// Exit codes: 0 clean, 1 findings at or above the denied severity
    /// (errors by default), 2 the model could not be read.
    ///
    /// Examples:
    ///   wcl wskill lint docs/wskills/wcl
    ///   wcl wskill lint --severity error,warn --deny warn
    ///   wcl wskill lint --format json --severity candidate
    Lint {
        /// The wskill folder (or an entry `.wcl` inside it). Defaults to
        /// the current directory.
        entry: Option<PathBuf>,
        /// Output format: `text` (default) or `json`.
        #[arg(long, value_enum, default_value_t = ReportFormat::Text)]
        format: ReportFormat,
        /// Report only these severities: `error`, `warn`, `candidate`
        /// (comma-separated, repeatable). Default: all three.
        #[arg(long, value_delimiter = ',', value_parser = parse_severity)]
        severity: Vec<wcl_wskill::Severity>,
        /// Fail on findings this certain or more: `error` (default),
        /// `warn`, `candidate`. Only reported severities count.
        #[arg(long, value_parser = parse_severity, default_value = "error")]
        deny: wcl_wskill::Severity,
    },
    /// Diff the model across a git range: the union graph — before ∪ after,
    /// with removed units and edges marked removed — plus the findings each
    /// changed unit gained and a header of health metrics that moved.
    ///
    /// This is the one reading a live graph structurally cannot give you,
    /// because a live graph draws what exists and half of an audit is what
    /// stopped existing. It is a review, not a gate: exit 0 unless the model
    /// could not be read (2).
    ///
    /// The range is any git range and defaults to `HEAD~1` — a bare revision
    /// (or an open-ended `a..`) compares it against the working tree, `a..b`
    /// compares two commits, and `a...b` starts from where they diverged,
    /// which is what reviewing a branch means.
    ///
    /// Examples:
    ///   wcl wskill audit docs/wskills/wcl
    ///   wcl wskill audit docs/wskills/wcl --range HEAD~5..HEAD
    ///   wcl wskill audit docs/wskills/wcl --range main... --format json
    Audit {
        /// The wskill folder (or an entry `.wcl` inside it). Defaults to
        /// the current directory.
        entry: Option<PathBuf>,
        /// The git range to audit. Default: `HEAD~1` (the previous commit
        /// against the working tree).
        #[arg(long, default_value = wcl_wskill::DEFAULT_RANGE)]
        range: String,
        /// Output format: `text` (default) or `json`.
        #[arg(long, value_enum, default_value_t = ReportFormat::Text)]
        format: ReportFormat,
    },
}

/// A `--severity` / `--deny` value, in the model's own vocabulary — the
/// severities are the library's, so the CLI parses into them rather than
/// keeping a second copy that could drift.
fn parse_severity(s: &str) -> Result<wcl_wskill::Severity, String> {
    wcl_wskill::Severity::parse(s).ok_or_else(|| {
        format!(
            "expected one of {}",
            wcl_wskill::Severity::ALL.map(|s| s.as_str()).join(", ")
        )
    })
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum ReportFormat {
    /// One line per finding, most certain first.
    Text,
    /// JSON array, one object per finding.
    Json,
}

#[derive(Subcommand)]
enum WadCommand {
    /// Derive a change-spec skeleton from a WAD diff: compare the working
    /// tree against a reviewed git revision (evaluated views, imports
    /// resolved from each side) and write a schema-valid `spec` block —
    /// status `:planning`, the exact entity/field change list, TODO
    /// rationale/instructions — into `data/specs/` beside the entry
    /// document. Changes to `spec` entities themselves are filtered out
    /// unless `--include-specs`.
    ///
    /// Examples:
    ///   wcl wad spec --from v1.2 wad.wcl
    ///   wcl wad spec --from HEAD~3 --id spec_billing --title "Billing split"
    Spec {
        /// Reviewed baseline revision to diff from (any git rev).
        #[arg(long)]
        from: String,
        /// WAD root document (default: wad.wcl in the current directory).
        entry: Option<PathBuf>,
        /// Spec id — also the filename (default: spec_from_<shortsha>).
        #[arg(long)]
        id: Option<String>,
        /// Spec title (default: "Changes since <rev>").
        #[arg(long)]
        title: Option<String>,
        /// Keep changes to `spec` entities in the change list.
        #[arg(long)]
        include_specs: bool,
        /// `wcl` writes the skeleton file; `json` prints the change list
        /// to stdout instead (nothing is written).
        #[arg(long, value_enum, default_value_t = wad::SpecFormat::Wcl)]
        format: wad::SpecFormat,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum DiffFormat {
    Wcl,
    Json,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PdfPageSize {
    A4,
    Letter,
}

impl From<PdfPageSize> for wcl_wdoc::PageSize {
    fn from(p: PdfPageSize) -> Self {
        match p {
            PdfPageSize::A4 => wcl_wdoc::PageSize::A4,
            PdfPageSize::Letter => wcl_wdoc::PageSize::Letter,
        }
    }
}

#[derive(Subcommand)]
enum WdocCommand {
    /// Render every `page` block in `<file>` to `<out>/<name>.html`.
    Build {
        /// Path to a WCL source file declaring one or more `page` blocks.
        file: PathBuf,
        /// Output directory. Created if missing.
        #[arg(long)]
        out: PathBuf,
        /// Build only this named `site` (flat at `<out>`). When omitted,
        /// every site renders into its own `<out>/<name>/` subdirectory
        /// with a chooser index (a single-site document is unaffected).
        #[arg(long)]
        site: Option<String>,
        /// Record a call-tree profile of the document evaluation driving
        /// the build and print it as JSON to stderr (like `wcl parse
        /// --profile`).
        #[arg(long)]
        profile: bool,
    },
    /// Render every `page` block in `<file>` to a folder of Markdown files
    /// under `<out>` (one `.md` per page), with diagrams / terminals /
    /// wireframes written as standalone `.svg` files the Markdown
    /// references. Aimed at AI / text consumers: zoomable diagrams render
    /// as plain SVG, equations stay as LaTeX, and videos are skipped.
    #[command(alias = "md")]
    Markdown {
        /// Path to a WCL source file declaring one or more `page` blocks.
        file: PathBuf,
        /// Output directory. Created if missing.
        #[arg(long)]
        out: PathBuf,
        /// Build only this named `site` (flat at `<out>`). When omitted,
        /// every site renders into its own `<out>/<name>/` subdirectory
        /// (a single-site document is unaffected).
        #[arg(long)]
        site: Option<String>,
    },
    /// Render `<file>` to an agent / Claude **skill folder** under `<out>`:
    /// the start page becomes `SKILL.md` (its front matter from the site's
    /// `skill { }` block), every other page goes under `references/`, and
    /// `file` blocks ship into their `dir` (`scripts/`, `assets/`, …). A site
    /// opts in with `default_template = :ai_skill`.
    Skill {
        /// Path to a WCL source file declaring a `:ai_skill` site.
        file: PathBuf,
        /// Output directory (the skill folder). Created if missing.
        #[arg(long)]
        out: PathBuf,
        /// Build only this named `site`. When omitted, every skill site is
        /// built (multiple sites render into `<out>/<name>/` subfolders).
        #[arg(long)]
        site: Option<String>,
    },
    /// Render each `site` in `<file>` to `<out>/<name>.pdf` (a pure-Rust
    /// PDF, no browser or external tools). Prose, headings and more
    /// paginate onto A4 (default) or US-Letter pages.
    Pdf {
        /// Path to a WCL source file declaring one or more `page` blocks.
        file: PathBuf,
        /// Output directory. Created if missing.
        #[arg(long)]
        out: PathBuf,
        /// Render only this named `site`. When omitted, the source file
        /// stem names the output PDF.
        #[arg(long)]
        site: Option<String>,
        /// Page size.
        #[arg(long, value_enum, default_value_t = PdfPageSize::A4)]
        page_size: PdfPageSize,
    },
    /// Run a local dev server. Watches the source for `.wcl` changes but
    /// does not rebuild automatically — press Enter in the console (or
    /// `POST /__wdoc_rebuild`) to rebuild, then the browser reloads.
    /// Editing and review comments live in `wcl editor`, not here.
    Serve {
        /// Path to a WCL source file declaring one or more `page` blocks.
        file: PathBuf,
        /// Bind address, or `auto` to pick the first free port near 8080.
        /// Default `127.0.0.1:8080`.
        #[arg(long, default_value = "127.0.0.1:8080")]
        addr: serve::BindSpec,
        /// Output directory. When omitted, a temp directory is used
        /// and removed on shutdown.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Serve only this named `site` (at `/`). When omitted, every
        /// site is served under `/<name>/` with a chooser index at `/`.
        #[arg(long)]
        site: Option<String>,
    },
    /// List the review comments stored in the `comments.wcl` sidecars under
    /// `<file>`'s directory (left from the `wcl editor` preview pane), or
    /// `resolve <id>` to delete one. JSON output (`--format json`) is aimed
    /// at an AI agent acting on the notes.
    Comments {
        /// Path to the WCL source file (the doc's entry point).
        file: PathBuf,
        /// Restrict to one named `site` (reserved; currently lists all).
        #[arg(long)]
        site: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = CommentFormat::Text)]
        format: CommentFormat,
        /// `resolve <id>` deletes the comment with that id.
        #[command(subcommand)]
        cmd: Option<CommentsSub>,
    },
    /// List the course answers stored in the `training.wcl` sidecars under
    /// `<file>`'s directory (left by a training site running under
    /// `wcl wdoc serve`), or `grade <id>` to write a verdict back.
    ///
    /// Free-text (`:text`) checks arrive `pending` and are listed first —
    /// they are what an agent grades, judging the answer against the check's
    /// `rubric`. Multiple-choice answers are graded in the page and recorded
    /// here only as history. JSON output (`--format json`) is aimed at an
    /// agent working the queue.
    Training {
        /// Path to the WCL source file (the doc's entry point).
        file: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = CommentFormat::Text)]
        format: CommentFormat,
        /// Only list answers still awaiting a grader.
        #[arg(long)]
        pending: bool,
        /// `grade <id>` writes a verdict onto that answer.
        #[command(subcommand)]
        cmd: Option<TrainingSub>,
    },
    /// Wait for a reviewer to finish, then print the comments — the agent side
    /// of the review handshake. Blocks until the reviewer clicks "Send to
    /// agent" in the preview pane of a running `wcl editor`, then lists the
    /// comments (like `comments`) so the agent can act on them. Run it again
    /// after making changes: the editor shows the agent is waiting once more,
    /// so the reviewer can rebuild and keep reviewing. With no editor running
    /// it just lists the current comments without blocking.
    Review {
        /// Path to the WCL source file (the doc's entry point) — the same
        /// root document the `wcl editor` was started with.
        file: PathBuf,
        /// Output format for the comments printed once released.
        #[arg(long, value_enum, default_value_t = CommentFormat::Json)]
        format: CommentFormat,
    },
}

#[derive(Subcommand)]
enum TrainingSub {
    /// Write a grader's verdict onto the answer with the given id.
    Grade {
        /// The answer id (from the listing).
        id: String,
        /// Feedback shown to the learner in the page.
        verdict: String,
        /// Mark the answer as not meeting the rubric.
        #[arg(long)]
        fail: bool,
        /// Free-form score recorded alongside the verdict (a mark, a tally).
        #[arg(long)]
        score: Option<String>,
    },
}

#[derive(Subcommand)]
enum CommentsSub {
    /// Delete the comment with the given id from the source.
    Resolve { id: String },
    /// Replace the body of the comment with the given id.
    Edit { id: String, body: String },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum CommentFormat {
    /// Human-readable table.
    Text,
    /// JSON array, one object per comment.
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Parse { file, profile } => match open_document(&file, profile) {
            Ok(doc) => {
                print!("{}", dump::document(&doc));
                emit_profile(&doc, profile);
                EXIT_OK
            }
            Err(err) => {
                eprintln!("{:?}", miette::Report::new(err));
                EXIT_PARSE
            }
        },
        Command::Check { file, json } => run_check(&file, json),
        Command::Fmt {
            file,
            in_place,
            indent,
            no_trailing_comma,
        } => run_fmt(&file, in_place, indent, no_trailing_comma).unwrap_or_else(|msg| {
            eprintln!("{msg}");
            EXIT_IO
        }),
        Command::Repl { file } => run_repl(file.as_deref()),
        Command::Lsp { tcp, log } => {
            if let Some(log_path) = log
                && let Err(e) = wcl_lsp::install_file_logger(&log_path)
            {
                eprintln!("failed to open log file {}: {e}", log_path.display());
                return ExitCode::from(EXIT_IO);
            }
            let rt = match build_runtime() {
                Ok(rt) => rt,
                Err(code) => return ExitCode::from(code),
            };
            match tcp {
                Some(addr) => match rt.block_on(wcl_lsp::start_tcp(addr)) {
                    Ok(()) => EXIT_OK,
                    Err(e) => {
                        eprintln!("tcp listener failed: {e}");
                        EXIT_IO
                    }
                },
                None => {
                    rt.block_on(wcl_lsp::start_stdio());
                    EXIT_OK
                }
            }
        }
        Command::Editor { root, addr } => {
            let rt = match build_runtime() {
                Ok(rt) => rt,
                Err(code) => return ExitCode::from(code),
            };
            let result = rt.block_on(editor::serve(root, addr));
            // Bounded teardown so a stray in-flight blocking task can never
            // hang process exit (mirrors `wdoc serve`).
            rt.shutdown_timeout(std::time::Duration::from_millis(200));
            match result {
                Ok(()) => EXIT_OK,
                Err(e) => {
                    eprintln!("editor failed: {e}");
                    EXIT_IO
                }
            }
        }
        Command::Set { file, path, value } => run_set(&file, &path, &value).unwrap_or_else(|msg| {
            eprintln!("{msg}");
            EXIT_IO
        }),
        Command::Eval {
            file,
            path,
            profile,
            json,
        } => match open_document(&file, profile) {
            Ok(doc) => {
                let exit = match doc.get(&path) {
                    Some(dr) => match dr.value() {
                        Ok(v) => {
                            if json {
                                match serde_json::to_string_pretty(&v) {
                                    Ok(s) => println!("{s}"),
                                    Err(e) => {
                                        eprintln!("json serialization failed: {e}");
                                        return ExitCode::from(EXIT_EVAL);
                                    }
                                }
                            } else {
                                println!("{}", v);
                            }
                            EXIT_OK
                        }
                        Err(e) => {
                            eprintln!("{:?}", miette::Report::new(e));
                            EXIT_EVAL
                        }
                    },
                    None => {
                        eprintln!("no such path: {path}");
                        if let Some(hint) = suggest_path(&doc, &path) {
                            eprintln!("did you mean: {hint}?");
                        }
                        EXIT_EVAL
                    }
                };
                emit_profile(&doc, profile);
                exit
            }
            Err(err) => {
                eprintln!("{:?}", miette::Report::new(err));
                EXIT_PARSE
            }
        },
        Command::Init {
            template,
            dest,
            answers,
            define,
            defaults,
            force,
            list,
        } => scaffold::run_init(template, dest, answers, define, defaults, force, list),
        Command::Wdoc { cmd } => run_wdoc(cmd),
        Command::Diff { old, new, format } => run_diff(&old, &new, format),
        Command::Answer {
            file,
            list,
            id,
            text,
            pick,
            skip,
        } => answer::run_answer(&file, list, id.as_deref(), text.as_deref(), &pick, skip),
        Command::Wskill { cmd } => match cmd {
            WskillCommand::Graph { entry, rev } => {
                let entry = entry.unwrap_or_else(|| PathBuf::from("."));
                wskill::run_graph(&entry, rev.as_deref())
            }
            WskillCommand::Lint {
                entry,
                format,
                severity,
                deny,
            } => {
                let entry = entry.unwrap_or_else(|| PathBuf::from("."));
                wskill::run_lint(&entry, format, &severity, deny)
            }
            WskillCommand::Audit {
                entry,
                range,
                format,
            } => {
                let entry = entry.unwrap_or_else(|| PathBuf::from("."));
                wskill::run_audit(&entry, &range, format)
            }
        },
        Command::Wad { cmd } => match cmd {
            WadCommand::Spec {
                from,
                entry,
                id,
                title,
                include_specs,
                format,
            } => {
                let entry = entry.unwrap_or_else(|| PathBuf::from("wad.wcl"));
                wad::run_spec(&from, &entry, id, title, include_specs, format)
            }
        },
    };
    ExitCode::from(code)
}

/// Failure opening a diff side: a parse/eval diagnostic, or an I/O / git
/// error (bad revision, missing path, git/tar absent).
enum OpenErr {
    Parse(ParseError),
    Io(String),
}

impl OpenErr {
    /// Render the error and return the matching exit code.
    fn report(self) -> u8 {
        match self {
            OpenErr::Parse(e) => {
                eprintln!("{:?}", miette::Report::new(e));
                EXIT_PARSE
            }
            OpenErr::Io(msg) => {
                eprintln!("{msg}");
                EXIT_IO
            }
        }
    }
}

/// Open one diff side. A plain path opens directly; a `<rev>:<path>` spec is
/// materialized from git into a temp dir first. The returned `TempDir` (if
/// any) must outlive use of the `Document`, so the caller holds it.
fn open_spec(arg: &str) -> Result<(Document, Option<tempfile::TempDir>), OpenErr> {
    match gitspec::parse_spec(arg) {
        gitspec::Spec::Working(path) => {
            let doc = open_document(&path, false).map_err(OpenErr::Parse)?;
            Ok((doc, None))
        }
        gitspec::Spec::Git { rev, path } => {
            let (root, rel) = gitspec::repo_rel(&path).map_err(OpenErr::Io)?;
            let tmp = gitspec::materialize_rev(&rev, &root).map_err(OpenErr::Io)?;
            let entry = tmp.path().join(&rel);
            if !entry.exists() {
                return Err(OpenErr::Io(format!(
                    "path '{rel}' not found in revision '{rev}'"
                )));
            }
            let doc = open_document(&entry, false).map_err(OpenErr::Parse)?;
            Ok((doc, Some(tmp)))
        }
    }
}

/// Open both sides (each a path or `<rev>:<path>` git spec), compute the
/// WCL-aware entity/field diff, and print it as a WCL tree (default) or a
/// JSON array. A parse/eval/git failure on either side renders the
/// diagnostic and exits non-zero.
fn run_diff(old: &str, new: &str, format: DiffFormat) -> u8 {
    // `_old`/`_new` hold the temp dirs alive until the diff is computed.
    let (old_doc, _old) = match open_spec(old) {
        Ok(x) => x,
        Err(e) => return e.report(),
    };
    let (new_doc, _new) = match open_spec(new) {
        Ok(x) => x,
        Err(e) => return e.report(),
    };
    let changes = diff::diff_documents(&old_doc, &new_doc);
    match format {
        DiffFormat::Wcl => {
            print!("{}", diff::render_wcl(&changes, old, new));
            EXIT_OK
        }
        DiffFormat::Json => match serde_json::to_string_pretty(&diff::changes_to_json(&changes)) {
            Ok(s) => {
                println!("{s}");
                EXIT_OK
            }
            Err(e) => {
                eprintln!("json serialization failed: {e}");
                EXIT_EVAL
            }
        },
    }
}

/// Map a wdoc `BuildError` to a CLI exit code. Shared by the `build` and
/// `markdown` subcommands (both render through the same pipeline).
fn build_error_code(err: &wcl_wdoc::BuildError) -> u8 {
    match err {
        wcl_wdoc::BuildError::Io(..) => EXIT_IO,
        wcl_wdoc::BuildError::Parse(_) => EXIT_PARSE,
        wcl_wdoc::BuildError::Schema(_) => EXIT_SCHEMA,
        wcl_wdoc::BuildError::Eval(_) => EXIT_EVAL,
        wcl_wdoc::BuildError::BadPage(_) => EXIT_EVAL,
        wcl_wdoc::BuildError::DuplicateId { .. } => EXIT_SCHEMA,
        wcl_wdoc::BuildError::DuplicatePage { .. } => EXIT_SCHEMA,
        wcl_wdoc::BuildError::BadLink(_) => EXIT_SCHEMA,
        wcl_wdoc::BuildError::BadTemplate(_) => EXIT_SCHEMA,
        wcl_wdoc::BuildError::Tileset(_) => EXIT_SCHEMA,
        wcl_wdoc::BuildError::EdgeRouting(_) => EXIT_SCHEMA,
        wcl_wdoc::BuildError::IncludeCycle(_) => EXIT_EVAL,
    }
}

/// Map a wdoc `PdfError` to a CLI exit code. Companion to
/// [`build_error_code`] for the `pdf` subcommand's distinct error type.
fn pdf_error_code(err: &wcl_wdoc::PdfError) -> u8 {
    match err {
        wcl_wdoc::PdfError::Io(..) => EXIT_IO,
        wcl_wdoc::PdfError::Parse(_) => EXIT_PARSE,
        wcl_wdoc::PdfError::Schema(_) => EXIT_SCHEMA,
        wcl_wdoc::PdfError::Eval(_) => EXIT_EVAL,
        wcl_wdoc::PdfError::BadDoc(_) => EXIT_EVAL,
        wcl_wdoc::PdfError::Render(_) => EXIT_IO,
    }
}

/// Report the outcome of a wdoc page-render pipeline (`build` / `markdown`
/// / `skill`): print the page count on success, or render the error and map
/// it to an exit code on failure.
fn report_pages(result: Result<usize, wcl_wdoc::BuildError>) -> u8 {
    match result {
        Ok(n) => {
            println!("wrote {n} page{}", if n == 1 { "" } else { "s" });
            EXIT_OK
        }
        Err(err) => {
            let code = build_error_code(&err);
            err.report();
            code
        }
    }
}

/// Build the multi-thread tokio runtime shared by the `lsp` and `serve`
/// subcommands. On failure prints the error and yields `EXIT_IO` so the
/// caller can return it in its own exit-code shape.
fn build_runtime() -> Result<tokio::runtime::Runtime, u8> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            eprintln!("failed to start tokio runtime: {e}");
            EXIT_IO
        })
}

/// Drain and print the non-fatal warnings the most recent render pass
/// collected (dropped diagram edges, lowerless blocks, unsized images, …).
fn print_render_warnings() {
    for w in wcl_wdoc::take_render_warnings() {
        eprintln!("warning: {w}");
    }
}

fn run_wdoc(cmd: WdocCommand) -> u8 {
    match cmd {
        WdocCommand::Build {
            file,
            out,
            site,
            profile,
        } => {
            let opts = wcl_wdoc::BuildOptions {
                profile,
                ..Default::default()
            };
            let result = wcl_wdoc::build_with_options(&file, &out, site.as_deref(), &opts);
            if result.is_ok() {
                print_render_warnings();
            }
            let result = result.map(|(n, p)| {
                if let Some(p) = p {
                    let json = dump::profile_to_json(&p);
                    let rendered = serde_json::to_string_pretty(&json)
                        .expect("serde_json::Value always serializes (string-keyed objects)");
                    eprintln!("{rendered}");
                }
                n
            });
            report_pages(result)
        }
        WdocCommand::Markdown { file, out, site } => {
            let result = wcl_wdoc::markdown(&file, &out, site.as_deref());
            if result.is_ok() {
                print_render_warnings();
            }
            report_pages(result)
        }
        WdocCommand::Skill { file, out, site } => {
            let result = wcl_wdoc::skill(&file, &out, site.as_deref());
            if result.is_ok() {
                print_render_warnings();
            }
            report_pages(result)
        }
        WdocCommand::Pdf {
            file,
            out,
            site,
            page_size,
        } => match wcl_wdoc::pdf(&file, &out, site.as_deref(), page_size.into()) {
            Ok(n) => {
                print_render_warnings();
                println!("wrote {n} pdf{}", if n == 1 { "" } else { "s" });
                EXIT_OK
            }
            Err(err) => {
                let code = pdf_error_code(&err);
                err.report();
                code
            }
        },
        WdocCommand::Serve {
            file,
            addr,
            out,
            site,
        } => {
            let rt = match build_runtime() {
                Ok(rt) => rt,
                Err(code) => return code,
            };
            let result = rt.block_on(serve::serve(file, out, addr, site));
            // Tear the runtime down with a bound so a stray in-flight
            // `spawn_blocking` (e.g. a `tokio::fs::read` in the static
            // handler) can never hang process exit on Ctrl-C.
            rt.shutdown_timeout(std::time::Duration::from_millis(200));
            match result {
                Ok(()) => EXIT_OK,
                Err(e) => {
                    eprintln!("serve failed: {e}");
                    EXIT_IO
                }
            }
        }
        WdocCommand::Comments {
            file,
            site: _,
            format,
            cmd,
        } => run_comments(&file, format, cmd),
        WdocCommand::Training {
            file,
            format,
            pending,
            cmd,
        } => run_training(&file, format, pending, cmd),
        WdocCommand::Review { file, format } => run_review(&file, format),
    }
}

/// `wcl wdoc training` — the grader's side of the course loop. Lists the
/// answers a training site recorded (pending ones first), or writes a verdict
/// back onto one. The learner's page long-polls, so a verdict written here
/// shows up in their browser without a rebuild or a reload.
fn run_training(
    file: &Path,
    format: CommentFormat,
    pending_only: bool,
    cmd: Option<TrainingSub>,
) -> u8 {
    // A course is usually entered below its wskill root (`wdoc/training/`), so
    // resolve the owning wskill the same way the dev server does.
    let dir = file.parent().unwrap_or_else(|| Path::new("."));
    let owned = wcl_wdoc::training::sidecar_for(dir, wcl_wskill::ROOT_MARKER);
    let root = owned.parent().unwrap_or(dir);
    if let Some(TrainingSub::Grade {
        id,
        verdict,
        fail,
        score,
    }) = cmd
    {
        return match wcl_wdoc::training::grade(root, &id, &verdict, !fail, score.as_deref()) {
            Ok(true) => {
                eprintln!("graded answer {id}");
                EXIT_OK
            }
            Ok(false) => {
                eprintln!("no answer with id {id}");
                EXIT_EVAL
            }
            Err(err) => {
                err.report();
                build_error_code(&err)
            }
        };
    }

    let recs = match wcl_wdoc::training::list(root) {
        Ok(r) => r,
        Err(err) => {
            err.report();
            return build_error_code(&err);
        }
    };
    let recs: Vec<_> = recs
        .into_iter()
        .filter(|r| !pending_only || r.is_pending())
        .collect();

    match format {
        CommentFormat::Json => {
            let arr = serde_json::Value::Array(
                recs.iter()
                    .map(|r| {
                        serde_json::json!({
                            "id": r.id,
                            "file": r.file.display().to_string(),
                            "course": r.course,
                            "lesson": r.lesson,
                            "check": r.check,
                            "response": r.response,
                            "status": r.status,
                            "pending": r.is_pending(),
                            "verdict": r.verdict,
                            "score": r.score,
                        })
                    })
                    .collect(),
            );
            let s = serde_json::to_string_pretty(&arr)
                .expect("serde_json::Value always serializes (string-keyed objects)");
            println!("{s}");
        }
        CommentFormat::Text => {
            if recs.is_empty() {
                eprintln!("no answers");
            }
            for r in &recs {
                let mark = if r.is_pending() { "…" } else { "✓" };
                println!("{mark} [{}] {} → {}", r.id, r.lesson, r.check);
                println!("        {}", r.response);
                if let Some(v) = &r.verdict {
                    println!("        verdict: {v}");
                }
            }
        }
    }
    EXIT_OK
}

/// `wcl wdoc review` — the agent side of the review handshake. Blocks until
/// the reviewer clicks "Send to agent" in the preview pane of a running
/// `wcl editor`, then prints the comments. With no editor up, lists them
/// without blocking.
fn run_review(file: &Path, format: CommentFormat) -> u8 {
    let root = file.parent().unwrap_or_else(|| Path::new("."));
    let hs = wcl_wdoc::Handshake::new(file);
    if !hs.server_alive() {
        eprintln!(
            "no running `wcl editor` found for this document — \
             listing current comments without waiting."
        );
        return print_comments(root, format);
    }
    let round = match hs.begin_wait() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("could not start review wait: {e}");
            return EXIT_IO;
        }
    };
    eprintln!(
        "waiting for the reviewer — click \"Send to agent\" in the editor's preview pane… (Ctrl-C to stop)"
    );

    // Poll for release on a small runtime so Ctrl-C cleans up the marker (which
    // otherwise leaves the toolbar showing the agent as still waiting).
    let rt = match build_runtime() {
        Ok(rt) => rt,
        Err(code) => return code,
    };
    let released = rt.block_on(async {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return false,
                _ = tokio::time::sleep(std::time::Duration::from_millis(400)) => {
                    if hs.released(round) { return true; }
                    // If the server went away while we waited, stop blocking.
                    if !hs.server_alive() { return true; }
                }
            }
        }
    });
    hs.end_wait();
    if !released {
        eprintln!("review wait cancelled.");
        return EXIT_OK;
    }
    print_comments(root, format)
}

/// `wcl wdoc comments` — list comments stored in the `comments.wcl` sidecars
/// under `<file>`'s directory, or `resolve` / `edit` one by id.
fn run_comments(file: &Path, format: CommentFormat, cmd: Option<CommentsSub>) -> u8 {
    // Sidecars live beside each wskill / the root doc; scan from `<file>`'s dir.
    let root = file.parent().unwrap_or_else(|| Path::new("."));
    match cmd {
        Some(CommentsSub::Resolve { id }) => {
            return match wcl_wdoc::comments::resolve(root, &id) {
                Ok(true) => {
                    eprintln!("resolved comment {id}");
                    EXIT_OK
                }
                Ok(false) => {
                    eprintln!("no comment with id {id}");
                    EXIT_EVAL
                }
                Err(err) => {
                    err.report();
                    build_error_code(&err)
                }
            };
        }
        Some(CommentsSub::Edit { id, body }) => {
            return match wcl_wdoc::comments::edit(root, &id, &body) {
                Ok(true) => {
                    eprintln!("edited comment {id}");
                    EXIT_OK
                }
                Ok(false) => {
                    eprintln!("no comment with id {id}");
                    EXIT_EVAL
                }
                Err(err) => {
                    err.report();
                    build_error_code(&err)
                }
            };
        }
        None => {}
    }
    print_comments(root, format)
}

/// List the comments under `root` and print them in `format`. Shared by
/// `wcl wdoc comments` (the plain list) and `wcl wdoc review` (after release).
fn print_comments(root: &Path, format: CommentFormat) -> u8 {
    let recs = match wcl_wdoc::comments::list(root) {
        Ok(r) => r,
        Err(err) => {
            err.report();
            return build_error_code(&err);
        }
    };
    match format {
        CommentFormat::Json => {
            let arr = serde_json::Value::Array(recs.iter().map(comment_record_json).collect());
            let s = serde_json::to_string_pretty(&arr)
                .expect("serde_json::Value always serializes (string-keyed objects)");
            println!("{s}");
        }
        CommentFormat::Text => {
            if recs.is_empty() {
                eprintln!("no comments");
            }
            for r in &recs {
                let where_ = match r.scope {
                    wcl_wdoc::CommentScope::Block => format!(
                        "page {} → {}",
                        r.page,
                        r.target.as_deref().unwrap_or("(block)")
                    ),
                    wcl_wdoc::CommentScope::Page => format!("page {}", r.page),
                };
                println!("[{}] {} — {}", r.id, where_.trim(), r.body);
                if let Some(q) = &r.quote {
                    println!("        quote: {q}");
                }
            }
        }
    }
    EXIT_OK
}

/// Render a [`wcl_wdoc::CommentRecord`] to a JSON object — the one shape
/// shared by `--format json` and the editor's `/api/comments`.
pub(crate) fn comment_record_json(r: &wcl_wdoc::CommentRecord) -> serde_json::Value {
    serde_json::json!({
        "id": r.id,
        "scope": r.scope.as_str(),
        "file": r.file.display().to_string(),
        "page": r.page,
        "page_file": r.page_file,
        "loc": r.loc,
        "target": r.target,
        "quote": r.quote,
        "body": r.body,
        "author": r.author,
        "status": r.status,
    })
}

/// Plain-stdin REPL with multiline continuation. Reads one line at a
/// time and keeps buffering until the running input has balanced
/// `{` / `(` / `[` brackets and is not inside an unterminated string,
/// then evaluates the assembled expression. Parse errors and eval
/// errors are tagged distinctly. EOF (Ctrl-D) or `:quit` / `:q`
/// exits cleanly. No history, no readline — piping input from a
/// script works as well as interactive use; piped sessions exit
/// non-zero (`EXIT_EVAL`, else `EXIT_PARSE`) when any error occurred,
/// while interactive sessions always exit `EXIT_OK`.
fn run_repl(file: Option<&Path>) -> u8 {
    use std::io::{BufRead, Write};
    let doc = match file {
        Some(p) => match open_document(p, false) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{:?}", miette::Report::new(e));
                return EXIT_PARSE;
            }
        },
        None => match Document::open("", "<repl>") {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{:?}", miette::Report::new(e));
                return EXIT_PARSE;
            }
        },
    };
    let stdin = std::io::stdin();
    let mut buf = String::new();
    let mut line = String::new();
    let interactive = atty_stdin();
    let mut had_parse_err = false;
    let mut had_eval_err = false;
    loop {
        let continuation = !buf.is_empty();
        if interactive {
            print!("{}", if continuation { "... " } else { "wcl> " });
            let _ = std::io::stdout().flush();
        }
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("read error: {e}");
                return EXIT_IO;
            }
        }
        if !continuation {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == ":quit" || trimmed == ":q" {
                break;
            }
        }
        buf.push_str(&line);
        if !repl_input_complete(&buf) {
            continue;
        }
        let to_eval = std::mem::take(&mut buf);
        match parse_expr(to_eval.trim(), "<repl>") {
            Ok(expr) => match doc.eval_expr(&expr) {
                Ok(value) => println!("{value}"),
                Err(e) => {
                    had_eval_err = true;
                    eprintln!("eval error: {:?}", miette::Report::new(e));
                }
            },
            Err(e) => {
                had_parse_err = true;
                eprintln!("parse error: {:?}", miette::Report::new(e));
            }
        }
    }
    // Interactive sessions exit 0 — a human saw and recovered from any
    // errors. Piped sessions report them so scripts can detect failure;
    // eval outranks parse as the later pipeline stage.
    if interactive {
        EXIT_OK
    } else if had_eval_err {
        EXIT_EVAL
    } else if had_parse_err {
        EXIT_PARSE
    } else {
        EXIT_OK
    }
}

/// `true` when `src` has balanced brackets and isn't sitting inside
/// an unterminated string literal. Used by the REPL to decide whether
/// to keep reading more lines. Counts characters outside strings to
/// avoid being confused by braces inside string literals.
fn repl_input_complete(src: &str) -> bool {
    let mut depth_curly = 0i32;
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut in_string = false;
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            match c {
                '\\' => {
                    chars.next();
                } // skip the escaped char
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth_curly += 1,
            '}' => depth_curly -= 1,
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '[' => depth_brack += 1,
            ']' => depth_brack -= 1,
            '/' if chars.peek() == Some(&'/') => {
                // `//` line comment — skip to end-of-line so a brace
                // inside a comment doesn't keep the REPL reading.
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    !in_string && depth_curly <= 0 && depth_paren <= 0 && depth_brack <= 0
}

/// Lightweight TTY check that avoids pulling in the `atty` crate.
/// We only need it to suppress the prompt when stdin is piped.
fn atty_stdin() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal()
}

/// True when the path argument selects stdin input (`wcl check -`).
fn is_stdin(file: &Path) -> bool {
    file == Path::new("-")
}

fn read_stdin() -> Result<String, String> {
    use std::io::Read as _;
    let mut src = String::new();
    std::io::stdin()
        .read_to_string(&mut src)
        .map_err(|e| format!("failed to read stdin: {e}"))?;
    Ok(src)
}

/// One diagnostic as a JSON object: `code` / `message`, plus the primary
/// label's `offset` / `length` when the error carries a span.
fn diagnostic_json(diag: &dyn miette::Diagnostic) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(code) = diag.code() {
        obj.insert("code".into(), code.to_string().into());
    }
    obj.insert("message".into(), diag.to_string().into());
    if let Some(label) = diag.labels().and_then(|mut ls| ls.next()) {
        obj.insert("offset".into(), label.offset().into());
        obj.insert("length".into(), label.len().into());
    }
    serde_json::Value::Object(obj)
}

fn check_report_json(
    name: &str,
    errors: Vec<serde_json::Value>,
    warnings: Vec<serde_json::Value>,
) -> String {
    // `ok` stays errors-only: warnings are advisory and never gate.
    serde_json::to_string_pretty(&serde_json::json!({
        "ok": errors.is_empty(),
        "file": name,
        "errors": errors,
        "warnings": warnings,
    }))
    .expect("string-keyed JSON object always serializes")
}

fn run_check(file: &Path, json: bool) -> u8 {
    let name = if is_stdin(file) {
        "<stdin>".to_string()
    } else {
        file.display().to_string()
    };
    let doc = if is_stdin(file) {
        let src = match read_stdin() {
            Ok(src) => src,
            Err(msg) => {
                eprintln!("{msg}");
                return EXIT_IO;
            }
        };
        let base_dir = std::env::current_dir().ok();
        Document::open_at_with_loader(
            &src,
            &name,
            base_dir.clone(),
            &cli_environment(base_dir.as_deref()),
            cli_loader(),
        )
    } else {
        open_document(file, false)
    };
    match doc {
        Ok(doc) => {
            let errs = doc.schema_errors();
            let warns = doc.schema_warnings();
            if json {
                let errors = errs.iter().map(|e| diagnostic_json(e)).collect();
                let warnings = warns.iter().map(|w| diagnostic_json(w)).collect();
                println!("{}", check_report_json(&name, errors, warnings));
                return if errs.is_empty() {
                    EXIT_OK
                } else {
                    EXIT_SCHEMA
                };
            }
            // Warnings are advisory: printed to stderr, never fatal —
            // the exit code (and `OK`) reflect errors only.
            for w in &warns {
                eprintln!("warning: {w}");
            }
            if !warns.is_empty() {
                let count = warns.len();
                eprintln!(
                    "{name}: {count} warning{}",
                    if count == 1 { "" } else { "s" }
                );
            }
            if errs.is_empty() {
                println!("OK");
                EXIT_OK
            } else {
                let count = errs.len();
                for e in &errs {
                    eprintln!("{:?}", miette::Report::new(e.clone()));
                }
                eprintln!(
                    "{name}: {count} schema violation{}",
                    if count == 1 { "" } else { "s" }
                );
                EXIT_SCHEMA
            }
        }
        Err(err) => {
            if json {
                println!(
                    "{}",
                    check_report_json(&name, vec![diagnostic_json(&err)], Vec::new())
                );
            } else {
                eprintln!("{:?}", miette::Report::new(err));
            }
            EXIT_PARSE
        }
    }
}

/// Drive `parse_for_edit → format::to_source` and either print the
/// result to stdout or atomically overwrite the input file. Returns
/// the exit code (`EXIT_OK` on success, `EXIT_PARSE` on parse failure)
/// or an error message describing an I/O failure.
fn run_fmt(
    file: &Path,
    in_place: bool,
    indent: usize,
    no_trailing_comma: bool,
) -> Result<u8, String> {
    if is_stdin(file) && in_place {
        return Err("--in-place cannot be combined with stdin input ('-')".to_string());
    }
    let (src, name) = if is_stdin(file) {
        (read_stdin()?, "<stdin>".to_string())
    } else {
        let src = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;
        (src, file.display().to_string())
    };
    let ast = match parse_for_edit(&src, name) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{:?}", miette::Report::new(e));
            return Ok(EXIT_PARSE);
        }
    };
    let cfg = wcl_format::FormatConfig {
        indent,
        trailing_comma_in_match: !no_trailing_comma,
        ..Default::default()
    };
    let formatted = wcl_format::to_source_with(&ast, &cfg);
    // Formatting must never break a parsing file: verify our own output
    // re-parses before writing it anywhere. A failure here is a
    // formatter bug — refuse to write (or print) the broken text so it
    // can't land in the tree.
    if let Err(e) = verify_reparses(&formatted) {
        eprintln!(
            "internal error: `wcl fmt` produced output that fails to re-parse — \
             refusing to write. Please report this.\n{e}"
        );
        return Ok(EXIT_PARSE);
    }
    if in_place {
        if formatted == src {
            eprintln!("{}: unchanged", file.display());
        } else {
            write_atomic(file, &formatted)
                .map_err(|e| format!("failed to write {}: {e}", file.display()))?;
            eprintln!("formatted {}", file.display());
        }
    } else {
        print!("{formatted}");
    }
    Ok(EXIT_OK)
}

/// Guard shared by `wcl fmt` / `wcl set`: parse the formatter's output
/// and surface the diagnostic if it doesn't round-trip.
fn verify_reparses(src: &str) -> Result<(), String> {
    match parse_for_edit(src, "<formatted output>".to_string()) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("{:?}", miette::Report::new(e))),
    }
}

/// Drive the round-trip API to update one field. Reads `file` as a
/// Document to find which file actually declares `path`, parses
/// *that* file for edit, replaces the field's expression with
/// `value` (parsed as a WCL expression), and writes the file back
/// atomically. Lifecycle:
///
///   doc = Document::from_file(file)
///   field = doc.get(path).as_field()           # leaf-only
///   home  = field.source_path() ?? file        # follows imports
///   ast   = parse_for_edit(home)
///   slot  = find_field_by_span(ast, field.span)
///   slot.expr = parse_expr(value)
///   write_atomic(home, format::to_source(ast))
fn run_set(file: &Path, path: &str, value: &str) -> Result<u8, String> {
    let doc = match open_document(file, false) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{:?}", miette::Report::new(e));
            return Ok(EXIT_PARSE);
        }
    };
    let dr = match doc.get(path) {
        Some(dr) => dr,
        None => {
            eprintln!("no such path: {path}");
            if let Some(hint) = suggest_path(&doc, path) {
                eprintln!("did you mean: {hint}?");
            }
            return Ok(EXIT_EVAL);
        }
    };
    let field = match dr.as_field() {
        Some(f) => f,
        None => {
            eprintln!(
                "`set` only updates leaf field values; `{path}` resolved to a {kind}",
                kind = dr.kind()
            );
            return Ok(EXIT_EVAL);
        }
    };
    let target_span = field.span();
    // Resolve the home file. `source_path()` returns None when the
    // field lives in the document's main source — i.e. the file the
    // user named.
    let home_path: PathBuf = field
        .source_path()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| file.to_path_buf());
    let new_expr = match parse_expr(value, "<set value>") {
        Ok(e) => e,
        Err(err) => {
            eprintln!("{:?}", miette::Report::new(err));
            return Ok(EXIT_PARSE);
        }
    };
    // Drop the Document borrow before we mutate the file: re-parsing
    // the home file gives us an independent mutable AST, the new
    // expression is already detached from any borrow.
    drop(doc);

    let src = std::fs::read_to_string(&home_path)
        .map_err(|e| format!("failed to read {}: {e}", home_path.display()))?;
    let mut ast = match parse_for_edit(&src, home_path.display().to_string()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{:?}", miette::Report::new(e));
            return Ok(EXIT_PARSE);
        }
    };
    let slot =
        wcl_lang::edit::find_field_by_span(&mut ast.items, target_span).ok_or_else(|| {
            format!(
                "internal: could not relocate field at span {}..{} in {}",
                target_span.start,
                target_span.end,
                home_path.display()
            )
        })?;
    slot.expr = new_expr;
    let formatted = wcl_format::to_source(&ast);
    if let Err(e) = verify_reparses(&formatted) {
        eprintln!(
            "internal error: `wcl set` produced output that fails to re-parse — \
             refusing to write. Please report this.\n{e}"
        );
        return Ok(EXIT_PARSE);
    }
    write_atomic(&home_path, &formatted)
        .map_err(|e| format!("failed to write {}: {e}", home_path.display()))?;
    // Confirmation goes to stderr so stdout stays clean for piping.
    // Naming the home file matters: `set` follows imports, so the
    // edited file may not be the one named on the command line.
    eprintln!("updated {path} in {}", home_path.display());
    Ok(EXIT_OK)
}

/// Write `contents` to `target` via a same-directory temp file +
/// rename. Avoids leaving a partial file on disk if the host gets
/// interrupted mid-write.
fn write_atomic(target: &Path, contents: &str) -> std::io::Result<()> {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::Builder::new()
        .prefix(".wcl-fmt-")
        .tempfile_in(dir)?;
    use std::io::Write as _;
    tmp.write_all(contents.as_bytes())?;
    tmp.persist(target)
        .map_err(|e| std::io::Error::other(format!("rename to target failed: {e}")))?;
    Ok(())
}

/// Return the closest top-level name (Levenshtein ≤ 2) to `needle`.
/// Matches against the first segment of dotted paths only — sufficient
/// to surface typos in the most common case (`port` vs `ports`).
fn suggest_path(doc: &Document, needle: &str) -> Option<String> {
    let first = needle.split('.').next().unwrap_or(needle);
    let mut candidates: Vec<String> = Vec::new();
    candidates.extend(doc.fields().map(|f| f.name().to_string()));
    candidates.extend(doc.blocks().map(|b| b.kind().to_string()));
    candidates
        .into_iter()
        .filter_map(|c| {
            let d = levenshtein(first, &c);
            (d > 0 && d <= 2).then_some((d, c))
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}
