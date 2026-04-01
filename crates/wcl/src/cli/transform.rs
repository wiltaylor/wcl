//! CLI handler for `wcl transform run`.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use crate::cli::LibraryArgs;
use crate::lang::ast::{BodyItem, DocItem};
use crate::transform::{self, FieldMapping, MapConfig, WhereClause};

/// Run a named transform from a WCL file.
pub fn run(
    name: &str,
    file: &Path,
    input: Option<&Path>,
    output: Option<&Path>,
    _params: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    // Parse the WCL file
    let source =
        fs::read_to_string(file).map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let mut options = crate::ParseOptions {
        root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        ..Default::default()
    };
    lib_args.apply(&mut options);

    let doc = crate::parse(&source, options);

    // Filter out errors that are expected in transform files:
    // - E040: undefined reference — `in` is only available at transform runtime
    // - E060: unknown decorator — transform decorators (@where, @stream, etc.)
    //   are not registered in the decorator schema registry
    let real_errors: Vec<_> = doc
        .errors()
        .into_iter()
        .filter(|d| !matches!(d.code.as_deref(), Some("E040") | Some("E060")))
        .collect();
    if !real_errors.is_empty() {
        for diag in &real_errors {
            eprintln!("{}", super::format_diagnostic(diag, &doc.source_map, file));
        }
        return Err("document has errors".to_string());
    }

    // Find the named transform block
    let transform_block = doc
        .ast
        .items
        .iter()
        .find_map(|item| {
            if let DocItem::Body(BodyItem::Block(block)) = item {
                if block.kind.name == "transform" {
                    if let Some(ref id) = block.inline_id {
                        let id_str = match id {
                            crate::lang::ast::InlineId::Literal(lit) => lit.value.clone(),
                            crate::lang::ast::InlineId::Interpolated(_) => return None,
                        };
                        if id_str == name {
                            return Some(block);
                        }
                    }
                }
            }
            None
        })
        .ok_or_else(|| format!("transform '{}' not found in {}", name, file.display()))?;

    // Extract codec names from the transform block's AST attributes
    let (input_codec, output_codec) = extract_codec_names_from_ast(transform_block)?;

    // Build the MapConfig from the transform's map sub-blocks
    let map_config = build_map_config(transform_block)?;

    // Open input
    let input_data: Box<dyn Read> = match input {
        Some(path) => Box::new(
            fs::File::open(path)
                .map_err(|e| format!("cannot open input {}: {}", path.display(), e))?,
        ),
        None => Box::new(io::stdin()),
    };

    // Read all input
    let mut input_buf = String::new();
    let mut reader: Box<dyn Read> = input_data;
    reader
        .read_to_string(&mut input_buf)
        .map_err(|e| format!("cannot read input: {}", e))?;

    // Open output
    let mut output_buf = Vec::new();

    let stats = transform::execute(
        &input_codec,
        input_buf.as_bytes(),
        &output_codec,
        &mut output_buf,
        &map_config,
        &indexmap::IndexMap::new(),
        &indexmap::IndexMap::new(),
    )
    .map_err(|e| format!("transform error: {}", e))?;

    // Write output
    match output {
        Some(path) => {
            fs::write(path, &output_buf)
                .map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
        }
        None => {
            io::Write::write_all(&mut io::stdout(), &output_buf)
                .map_err(|e| format!("cannot write output: {}", e))?;
        }
    }

    eprintln!(
        "transform '{}': {} records read, {} written, {} filtered",
        name, stats.records_read, stats.records_written, stats.records_filtered
    );

    Ok(())
}

/// Extract input/output codec names from the transform block's AST.
fn extract_codec_names_from_ast(
    block: &crate::lang::ast::Block,
) -> Result<(String, String), String> {
    let mut input_codec = "json".to_string();
    let mut output_codec = "json".to_string();

    for item in &block.body {
        if let BodyItem::Attribute(attr) = item {
            let value_str = extract_string_value(&attr.value);
            if let Some(ref s) = value_str {
                let codec_name = s.strip_prefix("codec::").unwrap_or(s).to_string();
                match attr.name.name.as_str() {
                    "input" => input_codec = codec_name,
                    "output" => output_codec = codec_name,
                    _ => {}
                }
            }
        }
    }

    Ok((input_codec, output_codec))
}

/// Try to extract a plain string value from an AST expression.
fn extract_string_value(expr: &crate::lang::ast::Expr) -> Option<String> {
    if let crate::lang::ast::Expr::StringLit(s) = expr {
        let mut result = String::new();
        for part in &s.parts {
            if let crate::lang::ast::StringPart::Literal(lit) = part {
                result.push_str(lit);
            } else {
                return None; // interpolated strings can't be resolved statically
            }
        }
        Some(result)
    } else {
        None
    }
}

/// Build a MapConfig from the transform block's AST.
fn build_map_config(block: &crate::lang::ast::Block) -> Result<MapConfig, String> {
    let mut mappings = Vec::new();
    let mut where_clauses = Vec::new();

    // Look for map sub-blocks and direct attributes
    for item in &block.body {
        match item {
            BodyItem::Block(sub_block) if sub_block.kind.name == "map" => {
                // Extract mappings from the map block
                for sub_item in &sub_block.body {
                    if let BodyItem::Attribute(attr) = sub_item {
                        mappings.push(FieldMapping {
                            output_name: attr.name.name.clone(),
                            expr: attr.value.clone(),
                        });
                    }
                }
                // Extract @where decorators
                for dec in &sub_block.decorators {
                    if dec.name.name == "where" {
                        if let Some(crate::lang::ast::DecoratorArg::Positional(expr)) =
                            dec.args.first()
                        {
                            where_clauses.push(WhereClause { expr: expr.clone() });
                        }
                    }
                }
            }
            // Direct attributes in the transform block (simple transforms without map sub-block)
            BodyItem::Attribute(attr)
                if attr.name.name != "input"
                    && attr.name.name != "output"
                    && attr.name.name != "auto_map" =>
            {
                mappings.push(FieldMapping {
                    output_name: attr.name.name.clone(),
                    expr: attr.value.clone(),
                });
            }
            _ => {}
        }
    }

    Ok(MapConfig {
        mappings,
        where_clauses,
    })
}
