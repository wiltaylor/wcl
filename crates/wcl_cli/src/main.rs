use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

mod add;
mod convert;
mod eval;
mod fmt;
mod inspect;
mod path;
mod query;
mod remove;
mod set;
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
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Validate {
            file,
            strict,
            schema,
            vars,
        } => validate::run(&file, strict, schema.as_deref(), &vars),
        Commands::Fmt { file, write, check } => fmt::run(&file, write, check),
        Commands::Query {
            file,
            query,
            format,
            count,
            recursive,
        } => query::run(&file, &query, &format, count, recursive),
        Commands::Inspect {
            file,
            ast,
            hir,
            scopes,
            deps,
        } => inspect::run(&file, ast, hir, scopes, deps),
        Commands::Eval { file, format, vars } => eval::run(&file, &format, &vars),
        Commands::Lsp { tcp } => {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| format!("failed to create tokio runtime: {}", e));
            match rt {
                Ok(rt) => {
                    if let Some(addr) = tcp {
                        rt.block_on(async {
                            wcl_lsp::start_tcp(&addr).await.map_err(|e| e.to_string())
                        })
                    } else {
                        rt.block_on(wcl_lsp::start_stdio());
                        Ok(())
                    }
                }
                Err(e) => Err(e),
            }
        }
        Commands::Convert { file, to, from } => convert::run(&file, to.as_deref(), from.as_deref()),
        Commands::Set { file, path, value } => set::run(&file, &path, &value),
        Commands::Add {
            file,
            block_spec,
            file_auto,
        } => add::run(&file, &block_spec, file_auto),
        Commands::Remove { file, path } => remove::run(&file, &path),
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        process::exit(1);
    }
}
