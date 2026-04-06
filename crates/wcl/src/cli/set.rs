use std::path::Path;

use crate::cli::mutation::{
    block_inline_id, build_span_to_block, line_indent, parse_assignment, run_selector, split_spec,
};
use crate::cli::LibraryArgs;
use crate::lang::ast::{Block, BodyItem};

pub fn run(file: &Path, spec: &str, lib_args: &LibraryArgs) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let (selector_str, action) = split_spec(spec);
    let action =
        action.ok_or_else(|| "missing '~> .path = value' assignment in spec".to_string())?;
    let (path, rhs) = parse_assignment(action)?;

    // Parse + evaluate the document so we can run the selector against
    // BlockRefs and (later) match by id/attribute values.
    let mut options = crate::ParseOptions {
        root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        ..Default::default()
    };
    lib_args.apply(&mut options);
    let doc = crate::parse(&source, options);
    if doc.ast.items.is_empty() && doc.has_errors() {
        for d in doc.errors() {
            eprintln!("{}", super::format_diagnostic(d, &doc.source_map, file));
        }
        return Err(format!("parse errors in {}", file.display()));
    }

    // Run the selector against the evaluated document.
    let matches = run_selector(&doc, selector_str)?;
    if matches.is_empty() {
        return Err(format!(
            "selector '{}' matched no blocks in {}",
            selector_str,
            file.display()
        ));
    }

    // Build span -> &Block lookup
    let span_to_block = build_span_to_block(&doc.ast);

    // Collect (start, end, replacement) edits, then apply in reverse order.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();

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

        // Walk path through nested child blocks for all but the last segment
        let mut current = ast_block;
        for seg in &path.segments[..path.segments.len() - 1] {
            current = find_child_block(current, &seg.name, seg.id.as_deref()).ok_or_else(|| {
                format!(
                    "nested block '{}' not found in {}{}",
                    seg.name,
                    current.kind.name,
                    block_inline_id(current)
                        .map(|s| format!("#{}", s))
                        .unwrap_or_default()
                )
            })?;
        }

        let final_seg = path.last();
        // Find existing attribute
        let existing = current.body.iter().find_map(|bi| match bi {
            BodyItem::Attribute(a) if a.name.name == final_seg.name => Some(a),
            _ => None,
        });

        if let Some(attr) = existing {
            let val_span = attr.value.span();
            edits.push((val_span.start, val_span.end, rhs.clone()));
        } else {
            // Insert a new `name = rhs` line just before the block's closing `}`
            let block_end = current.span.end;
            // current.span.end is the byte just after `}`. Find the `}` itself.
            let close_byte = source[..block_end]
                .rfind('}')
                .ok_or_else(|| "could not locate closing '}' for block".to_string())?;
            let indent = derive_inner_indent(&source, current);
            let insertion = format!("{}{} = {}\n", indent, final_seg.name, rhs);
            // Find start of the line containing the closing brace, so we
            // insert above it cleanly.
            let line_start = source[..close_byte].rfind('\n').map(|n| n + 1).unwrap_or(0);
            edits.push((line_start, line_start, insertion));
        }
    }

    // Apply edits in reverse order of start position
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = source.clone();
    for (start, end, replacement) in edits {
        result.replace_range(start..end, &replacement);
    }

    std::fs::write(file, &result).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
    println!(
        "set {} block(s) in {} ({})",
        matches.len(),
        file.display(),
        action
    );
    Ok(())
}

fn find_child_block<'a>(parent: &'a Block, kind: &str, id: Option<&str>) -> Option<&'a Block> {
    parent.body.iter().find_map(|bi| match bi {
        BodyItem::Block(b) if b.kind.name == kind && match_id(b, id) => Some(b),
        _ => None,
    })
}

fn match_id(block: &Block, id: Option<&str>) -> bool {
    match id {
        Some(target) => block_inline_id(block) == Some(target),
        None => true,
    }
}

/// Compute the indentation that should be used for items inside `block`.
/// Reads the indent of an existing body item if available; otherwise indents
/// 4 spaces past the block's opening line.
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
