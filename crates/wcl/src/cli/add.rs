use std::path::Path;

use crate::cli::mutation::{build_span_to_block, line_indent, run_selector, split_spec};
use crate::cli::LibraryArgs;
use crate::lang::ast::{Block, BodyItem};

/// `wcl add <file> '<spec>'`
///
/// Spec forms:
///   - `<wcl-fragment>` — append a top-level item (attribute, block, let) to
///     the file.
///   - `<selector> ~> <wcl-fragment>` — insert the fragment inside every
///     block matched by the query pipeline selector.
///
/// The fragment is any valid WCL body item.
pub fn run(file: &Path, spec: &str, lib_args: &LibraryArgs) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let (selector_or_fragment, action) = split_spec(spec);

    match action {
        None => append_top_level(file, &source, selector_or_fragment),
        Some(fragment) => {
            insert_into_matches(file, &source, selector_or_fragment, fragment, lib_args)
        }
    }
}

/// `wcl add file 'name = 1'` or `wcl add file 'server x { port = 8080 }'`
fn append_top_level(file: &Path, source: &str, fragment: &str) -> Result<(), String> {
    // Validate that the fragment is a single, well-formed top-level item.
    validate_fragment_as_body_item(fragment)?;

    let mut result = source.to_string();
    if !result.ends_with('\n') {
        result.push('\n');
    }
    if !result.ends_with("\n\n") {
        result.push('\n');
    }
    result.push_str(fragment.trim_end());
    result.push('\n');

    // Sanity check the rewritten file still parses
    validate_full_source_parses(file, &result)?;

    std::fs::write(file, &result).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
    println!("added top-level item to {}", file.display());
    Ok(())
}

/// `wcl add file 'selector ~> fragment'`
fn insert_into_matches(
    file: &Path,
    source: &str,
    selector_str: &str,
    fragment: &str,
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    validate_fragment_as_body_item(fragment)?;

    let mut options = crate::ParseOptions {
        root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        ..Default::default()
    };
    lib_args.apply(&mut options);
    let doc = crate::parse(source, options);
    if doc.ast.items.is_empty() && doc.has_errors() {
        for d in doc.errors() {
            eprintln!("{}", super::format_diagnostic(d, &doc.source_map, file));
        }
        return Err(format!("parse errors in {}", file.display()));
    }

    let matches = run_selector(&doc, selector_str)?;
    let span_to_block = build_span_to_block(&doc.ast);

    // Collect (insertion_offset, text) edits, apply in reverse order
    let mut edits: Vec<(usize, String)> = Vec::new();

    for matched in &matches {
        let block_ref = match matched {
            crate::Value::BlockRef(br) => br,
            _ => continue,
        };
        let ast_block = span_to_block.get(&block_ref.span).copied().ok_or_else(|| {
            format!(
                "internal: matched block at span {:?} not found in AST",
                block_ref.span
            )
        })?;

        let block_end = ast_block.span.end;
        let close_byte = source[..block_end]
            .rfind('}')
            .ok_or_else(|| "could not locate closing '}' for block".to_string())?;
        let line_start = source[..close_byte].rfind('\n').map(|n| n + 1).unwrap_or(0);

        let indent = derive_inner_indent(source, ast_block);
        let mut insertion = String::new();
        for line in fragment.lines() {
            if line.trim().is_empty() {
                insertion.push('\n');
            } else {
                insertion.push_str(&indent);
                insertion.push_str(line);
                insertion.push('\n');
            }
        }
        edits.push((line_start, insertion));
    }

    // Apply in reverse order so earlier offsets stay valid
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = source.to_string();
    for (offset, text) in edits {
        result.insert_str(offset, &text);
    }

    validate_full_source_parses(file, &result)?;

    std::fs::write(file, &result).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
    println!(
        "added fragment to {} block(s) in {}",
        matches.len(),
        file.display()
    );
    Ok(())
}

/// Verify that `fragment` parses as exactly one WCL body item (attribute,
/// block, let binding, etc.).
fn validate_fragment_as_body_item(fragment: &str) -> Result<(), String> {
    let mut sm = crate::lang::span::SourceMap::new();
    let fid = sm.add_file("<fragment>".into(), fragment.to_string());
    let (doc, diags) = crate::lang::parse(fragment, fid);
    if diags.has_errors() {
        let messages: Vec<String> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.is_error())
            .map(|d| d.message.clone())
            .collect();
        return Err(format!("invalid WCL fragment: {}", messages.join("; ")));
    }
    let body_items: Vec<_> = doc
        .items
        .iter()
        .filter(|item| matches!(item, crate::lang::ast::DocItem::Body(_)))
        .collect();
    if body_items.is_empty() {
        return Err("fragment did not produce any items".to_string());
    }
    if body_items.len() > 1 {
        return Err("fragment must contain exactly one item".to_string());
    }
    Ok(())
}

fn validate_full_source_parses(file: &Path, source: &str) -> Result<(), String> {
    let mut sm = crate::lang::span::SourceMap::new();
    let fid = sm.add_file(file.display().to_string(), source.to_string());
    let (doc, diags) = crate::lang::parse(source, fid);
    if doc.items.is_empty() && diags.has_errors() {
        eprintln!("generated source does not parse cleanly:");
        for d in diags.diagnostics() {
            if d.is_error() {
                eprintln!("{}", super::format_diagnostic(d, &sm, file));
            }
        }
        return Err("aborting".to_string());
    }
    Ok(())
}

fn derive_inner_indent(source: &str, block: &Block) -> String {
    if let Some(first) = block.body.first() {
        let span = match first {
            BodyItem::Attribute(a) => a.span,
            BodyItem::Block(b) => b.span,
            BodyItem::LetBinding(lb) => lb.span,
            BodyItem::Table(t) => t.span,
            _ => block.span,
        };
        return line_indent(source, span.start);
    }
    let outer = line_indent(source, block.span.start);
    format!("{}    ", outer)
}
