//! Low-level AST-mutation helpers for the **edit path** ([`parse_for_edit`]).
//!
//! These operate on the owned, fully-public [`ast`] returned by
//! [`crate::parse_for_edit`]: locate a node by its byte [`Span`], build new
//! nodes, and splice them into an items list. They are deliberately
//! schema-agnostic — the caller (the `wcl editor` save pipeline, or
//! `wcl set`) resolves schema concerns (inline-label slots, child kinds)
//! and hands these helpers the concrete values to write. Synthesised nodes
//! carry a zero [`Span::new(0, 0)`]; the source printer re-lays them out, so
//! the result round-trips through [`crate::format::to_source`].
//!
//! Span-equality lookups work because the edit path re-parses the same source
//! bytes a [`crate::Document`] saw, so node positions match exactly.

use crate::ast::{self, Expr, Field, ImportDecl, Item, Span, Trivia};

/// Walk `items` (recursing into [`Item::Block`] bodies) to find the
/// [`ast::Field`] whose `span` matches `span`.
pub fn find_field_by_span(items: &mut [Item], span: Span) -> Option<&mut Field> {
    for item in items {
        match item {
            Item::Field(f) if f.span == span => return Some(f),
            Item::Block(b) => {
                if let Some(found) = find_field_by_span(&mut b.items, span) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// Walk `items` (recursing into nested block bodies) to find the
/// [`ast::Block`] whose `span` matches `span`.
pub fn find_block_by_span(items: &mut [Item], span: Span) -> Option<&mut ast::Block> {
    for item in items {
        if let Item::Block(b) = item {
            if b.span == span {
                return Some(b);
            }
            if let Some(found) = find_block_by_span(&mut b.items, span) {
                return Some(found);
            }
        }
    }
    None
}

/// Set an existing field's value, or append a new `name = expr` field item to
/// `block` if no field with that name exists yet. Used to edit a scalar/text
/// property (and to set an optional field for the first time).
pub fn set_or_insert_field(block: &mut ast::Block, name: &str, expr: Expr) {
    for item in &mut block.items {
        if let Item::Field(f) = item
            && f.name == name
        {
            f.expr = expr;
            return;
        }
    }
    block.items.push(Item::Field(synth_field(name, expr)));
}

/// Set the block's inline-label slot `slot` (the positional value matched by a
/// schema field's `@inline(slot)`). Returns `false` when `slot` is past the end
/// of the existing labels and not the immediate next slot — the caller should
/// build labels contiguously rather than leave gaps.
pub fn set_label(block: &mut ast::Block, slot: usize, expr: Expr) -> bool {
    if slot < block.labels.len() {
        block.labels[slot] = expr;
        true
    } else if slot == block.labels.len() {
        block.labels.push(expr);
        true
    } else {
        false
    }
}

/// A WCL string-literal expression for `text`. The source printer chooses the
/// concrete rendering: a single-line escaped string, or a heredoc when the body
/// is multi-line and round-trips — so newlines and quotes in `text` survive.
pub fn string_literal_expr(text: &str) -> Expr {
    Expr::Utf8(text.to_string())
}

/// Build a fresh [`ast::Block`] from its kind, namespace qualifier, inline
/// label values (in slot order), and named `name = value` fields. All nodes
/// carry a zero span; the printer re-lays them out.
pub fn build_block(
    kind: &str,
    kind_ns: &[String],
    labels: Vec<Expr>,
    fields: Vec<(String, Expr)>,
) -> ast::Block {
    let items = fields
        .into_iter()
        .map(|(name, expr)| Item::Field(synth_field(&name, expr)))
        .collect();
    ast::Block {
        kind: kind.to_string(),
        kind_ns: kind_ns.to_vec(),
        labels,
        items,
        decorators: Vec::new(),
        span: Span::new(0, 0),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// Append `block` as a new top-level item of `src`, separated from the
/// preceding item by a blank line (when the source isn't empty).
pub fn append_top_level_block(src: &mut ast::Source, mut block: ast::Block) {
    if !src.items.is_empty() {
        block.leading_trivia.insert(0, Trivia::BlankLine);
    }
    src.items.push(Item::Block(block));
}

/// Insert `block` into `items` so it becomes the `block_index`-th
/// [`Item::Block`] among its siblings (counting only blocks, so interleaved
/// fields / `let`s don't shift the visual position). An out-of-range index
/// appends after the last block.
pub fn insert_block_at_index(items: &mut Vec<Item>, block_index: usize, block: ast::Block) {
    let mut seen = 0;
    let mut at = items.len();
    for (i, item) in items.iter().enumerate() {
        if matches!(item, Item::Block(_)) {
            if seen == block_index {
                at = i;
                break;
            }
            seen += 1;
        }
    }
    items.insert(at, Item::Block(block));
}

/// Insert `block` immediately after the [`Item::Block`] whose span matches
/// `after` — wherever that block lives (recursing into nested bodies). Returns
/// `false` when no block with that span is found.
pub fn insert_block_after_span(items: &mut Vec<Item>, after: Span, block: ast::Block) -> bool {
    insert_after_inner(items, after, block).is_none()
}

/// Returns `None` once inserted, or `Some(block)` (handed back) when the target
/// span wasn't found at this level — so the block survives across the recursion.
fn insert_after_inner(items: &mut Vec<Item>, after: Span, block: ast::Block) -> Option<ast::Block> {
    if let Some(pos) = items
        .iter()
        .position(|it| matches!(it, Item::Block(b) if b.span == after))
    {
        items.insert(pos + 1, Item::Block(block));
        return None;
    }
    let mut carry = Some(block);
    for item in items.iter_mut() {
        if let Item::Block(b) = item {
            let blk = carry.take().expect("carry is present until inserted");
            match insert_after_inner(&mut b.items, after, blk) {
                None => return None,
                Some(returned) => carry = Some(returned),
            }
        }
    }
    carry
}

/// Remove the [`Item::Block`] whose `span` matches `span` (recursing into
/// nested block bodies). Returns whether a block was removed.
pub fn remove_block_by_span(items: &mut Vec<Item>, span: Span) -> bool {
    if let Some(idx) = items
        .iter()
        .position(|it| matches!(it, Item::Block(b) if b.span == span))
    {
        items.remove(idx);
        return true;
    }
    for item in items.iter_mut() {
        if let Item::Block(b) = item
            && remove_block_by_span(&mut b.items, span)
        {
            return true;
        }
    }
    false
}

/// Swap the block at `span` with its adjacent **block** sibling in `direction`
/// (skipping interleaved fields / `let`s, which keep their positions). Returns
/// `false` when the block isn't found or is already at the relevant edge.
pub fn move_block_by_span(items: &mut [Item], span: Span, down: bool) -> bool {
    if let Some(pos) = items
        .iter()
        .position(|it| matches!(it, Item::Block(b) if b.span == span))
    {
        let neighbour = if down {
            (pos + 1..items.len()).find(|&i| matches!(items[i], Item::Block(_)))
        } else {
            (0..pos).rev().find(|&i| matches!(items[i], Item::Block(_)))
        };
        return match neighbour {
            Some(n) => {
                items.swap(pos, n);
                true
            }
            None => false,
        };
    }
    for item in items.iter_mut() {
        if let Item::Block(b) = item
            && move_block_by_span(&mut b.items, span, down)
        {
            return true;
        }
    }
    false
}

/// Ensure `src` has a quoted disk `import "<rel_path>"`. No-op (returns
/// `false`) when an equivalent import is already present; otherwise inserts one
/// after the last existing import (or at the top) and returns `true`.
pub fn ensure_import(src: &mut ast::Source, rel_path: &str) -> bool {
    let wanted = normalize_import(rel_path);
    let present = src.items.iter().any(|it| {
        matches!(it, Item::Import(imp) if !imp.system && normalize_import(&imp.path) == wanted)
    });
    if present {
        return false;
    }
    let import = Item::Import(ImportDecl {
        path: rel_path.to_string(),
        path_span: Span::new(0, 0),
        system: false,
        span: Span::new(0, 0),
        leading_trivia: Vec::new(),
        trailing_comment: None,
    });
    match src
        .items
        .iter()
        .rposition(|it| matches!(it, Item::Import(_)))
    {
        Some(i) => src.items.insert(i + 1, import),
        None => src.items.insert(0, import),
    }
    true
}

/// Compare import paths ignoring a leading `./`, so `foo.wcl` and `./foo.wcl`
/// are treated as the same import.
fn normalize_import(p: &str) -> &str {
    p.strip_prefix("./").unwrap_or(p)
}

fn synth_field(name: &str, expr: Expr) -> Field {
    Field {
        name: name.to_string(),
        expr,
        decorators: Vec::new(),
        span: Span::new(0, 0),
        leading_trivia: Vec::new(),
        trailing_comment: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{format, parse_for_edit};

    fn reparse(src: &str) -> ast::Source {
        let printed = format::to_source(&parse_for_edit(src, "t").unwrap());
        // The whole point: edited+printed source must re-parse.
        parse_for_edit(&printed, "t2").unwrap()
    }

    #[test]
    fn set_or_insert_updates_then_inserts() {
        let mut ast = parse_for_edit("card {\n  title = \"old\"\n}\n", "t").unwrap();
        let span = match &ast.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        let block = find_block_by_span(&mut ast.items, span).unwrap();
        set_or_insert_field(block, "title", string_literal_expr("new"));
        set_or_insert_field(block, "body", string_literal_expr("added"));
        let out = format::to_source(&ast);
        assert!(out.contains("title = \"new\""), "{out}");
        assert!(out.contains("body = \"added\""), "{out}");
        reparse(&out);
    }

    #[test]
    fn build_and_append_block_with_labels() {
        let mut ast = parse_for_edit("page {\n}\n", "t").unwrap();
        let block = build_block(
            "card",
            &[],
            vec![string_literal_expr("Title")],
            vec![("body".to_string(), string_literal_expr("Hi"))],
        );
        append_top_level_block(&mut ast, block);
        let out = format::to_source(&ast);
        assert!(out.contains("card \"Title\""), "{out}");
        assert!(out.contains("body = \"Hi\""), "{out}");
        reparse(&out);
    }

    #[test]
    fn remove_and_move_blocks() {
        let src = "page {\n  card \"a\" {}\n  card \"b\" {}\n  card \"c\" {}\n}\n";
        let mut ast = parse_for_edit(src, "t").unwrap();
        // span of the page's second child (card "b")
        let page = match &ast.items[0] {
            Item::Block(b) => b,
            _ => panic!(),
        };
        let b_span = match &page.items[1] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        let page_span = match &ast.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        // move "b" down -> order a, c, b
        let page = find_block_by_span(&mut ast.items, page_span).unwrap();
        assert!(move_block_by_span(&mut page.items, b_span, true));
        // remove "a"
        let page = find_block_by_span(&mut ast.items, page_span).unwrap();
        let a_span = match &page.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        assert!(remove_block_by_span(&mut page.items, a_span));
        let out = format::to_source(&ast);
        let ai = out.find("\"a\"");
        assert!(ai.is_none(), "a should be gone: {out}");
        let bi = out.find("\"b\"").unwrap();
        let ci = out.find("\"c\"").unwrap();
        assert!(ci < bi, "c should precede b after move: {out}");
        reparse(&out);
    }

    #[test]
    fn multiline_text_round_trips() {
        let mut ast = parse_for_edit("note {\n  body = \"x\"\n}\n", "t").unwrap();
        let span = match &ast.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        let block = find_block_by_span(&mut ast.items, span).unwrap();
        set_or_insert_field(block, "body", string_literal_expr("line one\nline two"));
        let out = format::to_source(&ast);
        reparse(&out);
        // re-parsing must reproduce the exact body text
        let re = parse_for_edit(&out, "t").unwrap();
        let blk = match &re.items[0] {
            Item::Block(b) => b,
            _ => panic!(),
        };
        let body = blk.items.iter().find_map(|it| match it {
            Item::Field(f) if f.name == "body" => Some(&f.expr),
            _ => None,
        });
        assert!(
            matches!(body, Some(Expr::Utf8(s)) if s == "line one\nline two"),
            "{out}"
        );
    }

    #[test]
    fn insert_after_targets_nested_block() {
        let src = "page {\n  card \"a\" {}\n  card \"b\" {}\n}\n";
        let mut ast = parse_for_edit(src, "t").unwrap();
        let page = match &ast.items[0] {
            Item::Block(b) => b,
            _ => panic!(),
        };
        let a_span = match &page.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        let new = build_block("card", &[], vec![string_literal_expr("x")], vec![]);
        assert!(insert_block_after_span(&mut ast.items, a_span, new));
        let out = format::to_source(&ast);
        let ai = out.find("\"a\"").unwrap();
        let xi = out.find("\"x\"").unwrap();
        let bi = out.find("\"b\"").unwrap();
        assert!(ai < xi && xi < bi, "x should sit between a and b: {out}");
        reparse(&out);
    }

    #[test]
    fn ensure_import_dedupes() {
        let mut ast = parse_for_edit("import \"./a.wcl\"\n\npage {}\n", "t").unwrap();
        assert!(!ensure_import(&mut ast, "a.wcl")); // ./a.wcl already present
        assert!(ensure_import(&mut ast, "data/b.wcl"));
        let out = format::to_source(&ast);
        assert!(out.contains("import \"data/b.wcl\""), "{out}");
        reparse(&out);
    }
}
