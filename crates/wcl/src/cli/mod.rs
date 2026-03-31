use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process;

use crate::lang::diagnostic::{Diagnostic, Severity};
use crate::lang::span::SourceMap;

/// Shared library search path options
#[derive(clap::Args, Clone, Debug, Default)]
pub struct LibraryArgs {
    /// Extra library search path (may be repeated; searched before defaults)
    #[arg(long = "lib-path", value_name = "DIR")]
    pub lib_paths: Vec<PathBuf>,
    /// Disable default XDG/system library search paths
    #[arg(long)]
    pub no_default_lib_paths: bool,
}

impl LibraryArgs {
    pub fn apply(&self, opts: &mut crate::ParseOptions) {
        opts.lib_paths.clone_from(&self.lib_paths);
        opts.no_default_lib_paths = self.no_default_lib_paths;
    }
}

/// Format a diagnostic with file location (line:col) when available.
pub(crate) fn format_diagnostic(
    diag: &Diagnostic,
    source_map: &SourceMap,
    fallback_path: &Path,
) -> String {
    let prefix = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    };

    let code_part = match diag.code.as_deref() {
        Some(c) => format!("[{}]", c),
        None => String::new(),
    };

    let span = diag.span;
    let is_dummy = span == crate::lang::span::Span::dummy();

    if is_dummy {
        format!("{}{}: {}", prefix, code_part, diag.message)
    } else {
        let (line, col) = source_map.line_col(span.file, span.start);
        let file_path = source_map.get_file(span.file).path.as_str();
        let display_path = if file_path.is_empty() || file_path == "<input>" {
            fallback_path.display().to_string()
        } else {
            file_path.to_string()
        };
        format!(
            "{}:{}:{}: {}{}: {}",
            display_path, line, col, prefix, code_part, diag.message
        )
    }
}

mod add;
mod convert;
mod docs;
mod eval;
mod fmt;
mod inspect;
mod path;
mod query;
mod remove;
mod set;
mod table;
mod transform;
mod validate;
mod vars;

#[derive(Parser)]
#[command(
    name = "wcl",
    version,
    about = "WCL \u{2014} Wil's Configuration Language CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate a WCL document
    Validate {
        /// Input file
        file: PathBuf,
        /// Treat warnings as errors
        #[arg(long)]
        strict: bool,
        /// External schema file
        #[arg(long)]
        schema: Option<PathBuf>,
        /// Set a variable (KEY=VALUE, may repeat)
        #[arg(long = "var", value_name = "KEY=VALUE")]
        vars: Vec<String>,
        #[command(flatten)]
        lib_args: LibraryArgs,
    },
    /// Format a WCL document
    Fmt {
        /// Input file
        file: PathBuf,
        /// Write formatted output back to file
        #[arg(long)]
        write: bool,
        /// Check if file is already formatted (exit code only)
        #[arg(long)]
        check: bool,
    },
    /// Query a WCL document
    Query {
        /// Input file
        file: PathBuf,
        /// Query expression
        query: String,
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
        /// Count results only
        #[arg(long)]
        count: bool,
        /// Search recursively in directory
        #[arg(long)]
        recursive: bool,
        #[command(flatten)]
        lib_args: LibraryArgs,
    },
    /// Inspect the AST or HIR of a WCL document
    Inspect {
        /// Input file
        file: PathBuf,
        /// Show raw AST
        #[arg(long)]
        ast: bool,
        /// Show resolved HIR
        #[arg(long)]
        hir: bool,
        /// Show scope tree
        #[arg(long)]
        scopes: bool,
        /// Show dependency graph
        #[arg(long)]
        deps: bool,
    },
    /// Evaluate a WCL document and print resolved output
    Eval {
        /// Input file
        file: PathBuf,
        /// Output format (json, yaml, toml)
        #[arg(long, default_value = "json")]
        format: String,
        /// Set a variable (KEY=VALUE, may repeat)
        #[arg(long = "var", value_name = "KEY=VALUE")]
        vars: Vec<String>,
        #[command(flatten)]
        lib_args: LibraryArgs,
    },
    /// Start the WCL language server
    Lsp {
        /// Listen on a TCP address instead of stdio (e.g. 127.0.0.1:9257)
        #[arg(long)]
        tcp: Option<String>,
    },
    /// Convert between WCL and other formats
    Convert {
        /// Input file
        file: PathBuf,
        /// Output format (json, yaml, toml)
        #[arg(long)]
        to: Option<String>,
        /// Input format for conversion to WCL
        #[arg(long)]
        from: Option<String>,
        #[command(flatten)]
        lib_args: LibraryArgs,
    },
    /// Set a value by path
    Set {
        /// Input file
        file: PathBuf,
        /// Path to the value (e.g. service#svc-api.port)
        path: String,
        /// New value
        value: String,
    },
    /// Add a new block
    Add {
        /// Input file
        file: PathBuf,
        /// Block specification (e.g. "service svc-new")
        block_spec: String,
        /// Auto-determine file placement
        #[arg(long)]
        file_auto: bool,
    },
    /// Remove a block or attribute by path
    Remove {
        /// Input file
        file: PathBuf,
        /// Path to remove (e.g. service#svc-old, service#svc-api.debug)
        path: String,
    },
    /// Table row operations (insert, remove, update)
    Table {
        #[command(subcommand)]
        action: TableAction,
    },
    /// Generate schema documentation as an mdBook
    Docs {
        /// Input WCL files
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Output directory
        #[arg(long, default_value = "docs-out")]
        output: PathBuf,
        /// Book title
        #[arg(long, default_value = "WCL Schema Reference")]
        title: String,
        #[command(flatten)]
        lib_args: LibraryArgs,
    },
    /// Run data transformations
    Transform {
        #[command(subcommand)]
        action: TransformAction,
    },
}

#[derive(Subcommand)]
enum TransformAction {
    /// Execute a transform
    Run {
        /// Transform name (block ID in the WCL file)
        name: String,
        /// WCL file containing the transform definition
        #[arg(short, long)]
        file: PathBuf,
        /// Input data file (stdin if omitted)
        #[arg(long)]
        input: Option<PathBuf>,
        /// Output data file (stdout if omitted)
        #[arg(long)]
        output: Option<PathBuf>,
        /// Parameters (KEY=VALUE, may repeat)
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
        #[command(flatten)]
        lib_args: LibraryArgs,
    },
}

#[derive(Subcommand)]
enum TableAction {
    /// Insert a row into a table
    Insert {
        /// Input file
        file: PathBuf,
        /// Table name (inline ID)
        table: String,
        /// Row values as pipe-delimited: '"alice" | 25'
        values: String,
    },
    /// Remove rows matching a condition
    Remove {
        /// Input file
        file: PathBuf,
        /// Table name (inline ID)
        table: String,
        /// Condition expression: 'name == "alice"'
        #[arg(long = "where")]
        condition: String,
    },
    /// Update cells in rows matching a condition
    Update {
        /// Input file
        file: PathBuf,
        /// Table name (inline ID)
        table: String,
        /// Condition: 'name == "alice"'
        #[arg(long = "where")]
        condition: String,
        /// Assignments: 'age = 26, role = "admin"'
        #[arg(long)]
        set: String,
    },
}

pub fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Validate {
            file,
            strict,
            schema,
            vars,
            lib_args,
        } => validate::run(&file, strict, schema.as_deref(), &vars, &lib_args),
        Commands::Fmt { file, write, check } => fmt::run(&file, write, check),
        Commands::Query {
            file,
            query,
            format,
            count,
            recursive,
            lib_args,
        } => query::run(&file, &query, &format, count, recursive, &lib_args),
        Commands::Inspect {
            file,
            ast,
            hir,
            scopes,
            deps,
        } => inspect::run(&file, ast, hir, scopes, deps),
        Commands::Eval {
            file,
            format,
            vars,
            lib_args,
        } => eval::run(&file, &format, &vars, &lib_args),
        Commands::Lsp { tcp } => {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| format!("failed to create tokio runtime: {}", e));
            match rt {
                Ok(rt) => {
                    if let Some(addr) = tcp {
                        rt.block_on(async {
                            crate::lsp::start_tcp(&addr)
                                .await
                                .map_err(|e| e.to_string())
                        })
                    } else {
                        rt.block_on(crate::lsp::start_stdio());
                        Ok(())
                    }
                }
                Err(e) => Err(e),
            }
        }
        Commands::Convert {
            file,
            to,
            from,
            lib_args,
        } => convert::run(&file, to.as_deref(), from.as_deref(), &lib_args),
        Commands::Set { file, path, value } => set::run(&file, &path, &value),
        Commands::Add {
            file,
            block_spec,
            file_auto,
        } => add::run(&file, &block_spec, file_auto),
        Commands::Remove { file, path } => remove::run(&file, &path),
        Commands::Docs {
            files,
            output,
            title,
            lib_args,
        } => docs::run(&files, &output, &title, &lib_args),
        Commands::Table { action } => match action {
            TableAction::Insert {
                file,
                table: table_name,
                values,
            } => table::run_insert(&file, &table_name, &values),
            TableAction::Remove {
                file,
                table: table_name,
                condition,
            } => table::run_remove(&file, &table_name, &condition),
            TableAction::Update {
                file,
                table: table_name,
                condition,
                set,
            } => table::run_update(&file, &table_name, &condition, &set),
        },
        Commands::Transform { action } => match action {
            TransformAction::Run {
                name,
                file,
                input,
                output,
                params,
                lib_args,
            } => transform::run(
                &name,
                &file,
                input.as_deref(),
                output.as_deref(),
                &params,
                &lib_args,
            ),
        },
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        process::exit(1);
    }
}
