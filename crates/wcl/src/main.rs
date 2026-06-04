use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use wcl_lang::{Document, ParseError, ast, format as wcl_format, parse_expr, parse_for_edit};

mod dump;
mod serve;

const EXIT_OK: u8 = 0;
const EXIT_PARSE: u8 = 1;
const EXIT_SCHEMA: u8 = 2;
const EXIT_EVAL: u8 = 3;
const EXIT_IO: u8 = 4;

fn open_document(file: &Path, profile: bool) -> Result<Document, ParseError> {
    if profile {
        Document::from_file_profiled(file)
    } else {
        Document::from_file(file)
    }
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
#[command(name = "wcl", version, about = "WCL command-line interface")]
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
        /// Path to a WCL source file.
        file: PathBuf,
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
        /// Path to a WCL source file.
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
    /// EOF (Ctrl-D) or `:quit` exits.
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
    /// WCL-driven static site generator. Use `wcl wdoc build` for a
    /// one-shot render and `wcl wdoc serve` for a watch-rebuild dev
    /// server.
    Wdoc {
        #[command(subcommand)]
        cmd: WdocCommand,
    },
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
    /// Run a local dev server. Watches the source for `.wcl` changes
    /// and re-renders on each modification — refresh the browser to
    /// see updates.
    Serve {
        /// Path to a WCL source file declaring one or more `page` blocks.
        file: PathBuf,
        /// Bind address. Default `127.0.0.1:8080`.
        #[arg(long, default_value = "127.0.0.1:8080")]
        addr: std::net::SocketAddr,
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
        Command::Check { file } => match open_document(&file, false) {
            Ok(doc) => {
                let errs = doc.schema_errors();
                if errs.is_empty() {
                    println!("OK");
                    EXIT_OK
                } else {
                    let count = errs.len();
                    for e in &errs {
                        eprintln!("{:?}", miette::Report::new(e.clone()));
                    }
                    eprintln!(
                        "{}: {} schema violation{}",
                        file.display(),
                        count,
                        if count == 1 { "" } else { "s" }
                    );
                    EXIT_SCHEMA
                }
            }
            Err(err) => {
                eprintln!("{:?}", miette::Report::new(err));
                EXIT_PARSE
            }
        },
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
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("failed to start tokio runtime: {e}");
                    return ExitCode::from(EXIT_IO);
                }
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
        Command::Wdoc { cmd } => run_wdoc(cmd),
    };
    ExitCode::from(code)
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
        wcl_wdoc::BuildError::BadLink(_) => EXIT_SCHEMA,
        wcl_wdoc::BuildError::BadTemplate(_) => EXIT_SCHEMA,
        wcl_wdoc::BuildError::Tileset(_) => EXIT_SCHEMA,
        wcl_wdoc::BuildError::EdgeRouting(_) => EXIT_SCHEMA,
    }
}

fn run_wdoc(cmd: WdocCommand) -> u8 {
    match cmd {
        WdocCommand::Build { file, out, site } => {
            match wcl_wdoc::build(&file, &out, site.as_deref()) {
                Ok(n) => {
                    for w in wcl_wdoc::take_edge_warnings() {
                        eprintln!("warning: {w}");
                    }
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
        WdocCommand::Markdown { file, out, site } => {
            match wcl_wdoc::markdown(&file, &out, site.as_deref()) {
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
        WdocCommand::Skill { file, out, site } => {
            match wcl_wdoc::skill(&file, &out, site.as_deref()) {
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
        WdocCommand::Pdf {
            file,
            out,
            site,
            page_size,
        } => match wcl_wdoc::pdf(&file, &out, site.as_deref(), page_size.into()) {
            Ok(n) => {
                println!("wrote {n} pdf{}", if n == 1 { "" } else { "s" });
                EXIT_OK
            }
            Err(err) => {
                let code = match &err {
                    wcl_wdoc::PdfError::Io(..) => EXIT_IO,
                    wcl_wdoc::PdfError::Parse(_) => EXIT_PARSE,
                    wcl_wdoc::PdfError::Schema(_) => EXIT_SCHEMA,
                    wcl_wdoc::PdfError::Eval(_) => EXIT_EVAL,
                    wcl_wdoc::PdfError::BadDoc(_) => EXIT_EVAL,
                    wcl_wdoc::PdfError::Render(_) => EXIT_IO,
                };
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
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("failed to start tokio runtime: {e}");
                    return EXIT_IO;
                }
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

/// Drive `parse_for_edit → format::to_source` and either print the
/// result to stdout or atomically overwrite the input file. Returns
/// the exit code (`EXIT_OK` on success, `EXIT_PARSE` on parse failure)
/// or an error message describing an I/O failure.
/// Plain-stdin REPL with multiline continuation. Reads one line at a
/// time and keeps buffering until the running input has balanced
/// `{` / `(` / `[` brackets and is not inside an unterminated string,
/// then evaluates the assembled expression. Parse errors and eval
/// errors are tagged distinctly. EOF (Ctrl-D) or `:quit` / `:q`
/// exits cleanly. No history, no readline — piping input from a
/// script works as well as interactive use.
fn run_repl(file: Option<&Path>) -> u8 {
    use std::io::{BufRead, Write};
    let doc = match file {
        Some(p) => match Document::from_file(p) {
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
                Err(e) => eprintln!("eval error: {:?}", miette::Report::new(e)),
            },
            Err(e) => eprintln!("parse error: {:?}", miette::Report::new(e)),
        }
    }
    EXIT_OK
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
            '#' => {
                // Line comment — skip to end-of-line.
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

fn run_fmt(
    file: &Path,
    in_place: bool,
    indent: usize,
    no_trailing_comma: bool,
) -> Result<u8, String> {
    let src = std::fs::read_to_string(file)
        .map_err(|e| format!("failed to read {}: {e}", file.display()))?;
    let ast = match parse_for_edit(&src, file.display().to_string()) {
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
    if in_place {
        write_atomic(file, &formatted)
            .map_err(|e| format!("failed to write {}: {e}", file.display()))?;
    } else {
        print!("{formatted}");
    }
    Ok(EXIT_OK)
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
    let doc = match Document::from_file(file) {
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
    let slot = find_field_by_span(&mut ast.items, target_span).ok_or_else(|| {
        format!(
            "internal: could not relocate field at span {}..{} in {}",
            target_span.start,
            target_span.end,
            home_path.display()
        )
    })?;
    slot.expr = new_expr;
    write_atomic(&home_path, &wcl_format::to_source(&ast))
        .map_err(|e| format!("failed to write {}: {e}", home_path.display()))?;
    Ok(EXIT_OK)
}

/// Walk `items` (and recursively into `Item::Block.items`) to find
/// the [`ast::Field`] whose `span` matches `span`. Span-equality
/// works because `run_set` re-parses the same source bytes that
/// `Document` parsed, so item positions match exactly.
fn find_field_by_span(items: &mut [ast::Item], span: ast::Span) -> Option<&mut ast::Field> {
    for item in items {
        match item {
            ast::Item::Field(f) if f.span == span => return Some(f),
            ast::Item::Block(b) => {
                if let Some(found) = find_field_by_span(&mut b.items, span) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
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
