use std::path::{Path, PathBuf};

use crate::cli::vars::parse_var_args;
use crate::cli::LibraryArgs;

fn source_options(
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<wcl_wdoc::source::SourceOptions, String> {
    Ok(wcl_wdoc::source::SourceOptions {
        variables: parse_var_args(vars)?,
        lib_paths: lib_args.lib_paths.clone(),
        no_default_lib_paths: lib_args.no_default_lib_paths,
    })
}

pub fn run_build(
    files: &[PathBuf],
    output: &Path,
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let options = source_options(vars, lib_args)?;
    let result = wcl_wdoc::source::build_from_files(files, output, &options)?;
    println!(
        "wdoc: built {} page(s) to {}",
        result.pages,
        result.output.display()
    );
    Ok(())
}

pub fn run_validate(
    files: &[PathBuf],
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let options = source_options(vars, lib_args)?;
    let result = wcl_wdoc::source::validate_from_files(files, &options)?;
    println!(
        "wdoc: valid ({} section(s), {} page(s))",
        result.sections, result.pages
    );
    Ok(())
}

pub fn run_serve(
    files: &[PathBuf],
    port: u16,
    open: bool,
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let options = source_options(vars, lib_args)?;
    wcl_wdoc::source::serve_from_files(files, port, open, &options)
}

/// Install the embedded wdoc standard library (`wdoc.wcl`) into the user's
/// library directory so editors, LSP, and `wcl validate` can resolve
/// `import <wdoc.wcl>`.
pub fn run_install_library(force: bool) -> Result<(), String> {
    let target = wcl_wdoc::source::install_library(force)?;
    println!("installed wdoc library to {}", target.display());
    Ok(())
}
