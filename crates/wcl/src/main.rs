use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use wcl_lang::{
    Document, Environment, ParseError, format as wcl_format, parse_expr, parse_for_edit,
};

mod diff;
mod dump;
mod edit;
mod gitspec;
mod scaffold;
mod serve;

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

/// Report the outcome of a wdoc page-render pipeline (`build` / `markdown`):
/// print the page count on success, or render the error and map it to an exit
/// code on failure.
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
            let opts = wcl_wdoc::BuildOptions { profile };
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
    }
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
            let diagnostics = doc.schema_diagnostics();
            let warns = doc.schema_warnings();
            if json {
                let errors = diagnostics
                    .iter()
                    .map(|(error, _)| diagnostic_json(error))
                    .collect();
                let warnings = warns.iter().map(|w| diagnostic_json(w)).collect();
                println!("{}", check_report_json(&name, errors, warnings));
                return if diagnostics.is_empty() {
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
            if diagnostics.is_empty() {
                println!("OK");
                EXIT_OK
            } else {
                let count = diagnostics.len();
                for (error, source) in diagnostics {
                    let report = miette::Report::new(error);
                    match source {
                        Some(source) => eprintln!("{:?}", report.with_source_code(source)),
                        None => eprintln!("{report:?}"),
                    }
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
