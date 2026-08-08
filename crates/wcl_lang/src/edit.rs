//! Low-level AST-mutation helpers for the **edit path** ([`parse_for_edit`]).
//!
//! These operate on the owned, fully-public [`ast`] returned by
//! [`crate::parse_for_edit`]: locate a node by its byte [`Span`], build new
//! nodes, and splice them into an items list. They are deliberately
//! schema-agnostic — the caller (`wcl set`) resolves schema concerns
//! (inline-label slots, child kinds) and hands these helpers the concrete
//! values to write. Synthesised nodes
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

/// [`find_block_by_span`] for a reader: the [`ast::Block`] at `span`, without
/// borrowing the tree mutably. Every span-addressed *read* of a parsed file
/// (what a block declares, how a field was written) wants this — only a
/// mutation wants the `&mut` form.
pub fn block_at_span(items: &[Item], span: Span) -> Option<&ast::Block> {
    for item in items {
        if let Item::Block(b) = item {
            if b.span == span {
                return Some(b);
            }
            if let Some(found) = block_at_span(&b.items, span) {
                return Some(found);
            }
        }
    }
    None
}

/// The first `kind` block whose first inline label is `label`, at any depth.
///
/// The id-addressed counterpart of [`find_block_by_span`], and what every
/// writer that must survive a reformat uses: a span read before one commit is
/// stale by the next, but a block's kind and label are how it is *named*.
pub fn find_block_by_kind_label<'a>(
    items: &'a mut [Item],
    kind: &str,
    label: &str,
) -> Option<&'a mut ast::Block> {
    for item in items {
        if let Item::Block(b) = item {
            if b.kind == kind && block_label(b).as_deref() == Some(label) {
                return Some(b);
            }
            if let Some(found) = find_block_by_kind_label(&mut b.items, kind, label) {
                return Some(found);
            }
        }
    }
    None
}

/// A block's first inline label when it is a plain identifier or string —
/// how a block is named (`concept alpha`, `chapter "Start"`). `None` when it
/// has no labels, or the first one is a computed expression.
pub fn block_label(b: &ast::Block) -> Option<String> {
    match b.labels.first()? {
        Expr::Identifier(s, _) | Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
        _ => None,
    }
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

/// Remove the `name = expr` field item from `block`, if present. Returns
/// whether a field was removed. Used to clear an optional field.
pub fn remove_field(block: &mut ast::Block, name: &str) -> bool {
    let before = block.items.len();
    block
        .items
        .retain(|item| !matches!(item, Item::Field(f) if f.name == name));
    block.items.len() != before
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
        conditional: false,
        slot_decl: None,
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
            carry = Some(insert_after_inner(&mut b.items, after, blk)?);
        }
    }
    carry
}

/// Replace the [`Item::Block`] whose `span` matches `span` with `block`
/// (recursing into nested block bodies). The replacement inherits the old
/// block's leading trivia (comments / blank lines) and its span — so callers
/// can still locate the replacement by the original span before re-printing.
/// Returns whether a block was replaced.
pub fn replace_block_by_span(items: &mut [Item], span: Span, block: ast::Block) -> bool {
    replace_inner(items, span, block).is_none()
}

/// Returns `None` once replaced, or `Some(block)` (handed back) when the
/// target span wasn't found at this level.
fn replace_inner(items: &mut [Item], span: Span, block: ast::Block) -> Option<ast::Block> {
    if let Some(pos) = items
        .iter()
        .position(|it| matches!(it, Item::Block(b) if b.span == span))
    {
        let Item::Block(old) = &items[pos] else {
            unreachable!("position matched a block item")
        };
        let mut new = block;
        new.leading_trivia = old.leading_trivia.clone();
        new.span = span;
        items[pos] = Item::Block(new);
        return None;
    }
    let mut carry = Some(block);
    for item in items.iter_mut() {
        if let Item::Block(b) = item {
            let blk = carry.take().expect("carry is present until replaced");
            carry = Some(replace_inner(&mut b.items, span, blk)?);
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

/// Append a `lhs -> rhs` connection statement to `block`, optionally with a
/// `:kind` symbol. Returns `false` when the pair is already connected (the
/// statement list is a set — a duplicate would render two identical edges).
///
/// The node is synthesised with zero spans and no trivia, like
/// [`build_block`]: the printer reads neither, so the result round-trips.
pub fn add_connection(block: &mut ast::Block, lhs: &str, rhs: &str, kind: Option<&str>) -> bool {
    if find_connection(&block.items, lhs, rhs).is_some() {
        return false;
    }
    block.items.push(Item::Connection(ast::ConnectionStmt {
        lhs: lhs.to_string(),
        lhs_span: Span::new(0, 0),
        rhs: rhs.to_string(),
        rhs_span: Span::new(0, 0),
        kind: kind.map(str::to_string),
        kind_span: None,
        span: Span::new(0, 0),
        leading_trivia: Vec::new(),
        trailing_comment: None,
    }));
    true
}

/// Drop the `lhs -> rhs` connection statement from `block`. Returns whether
/// one was removed.
pub fn remove_connection(block: &mut ast::Block, lhs: &str, rhs: &str) -> bool {
    match find_connection(&block.items, lhs, rhs) {
        Some(idx) => {
            block.items.remove(idx);
            true
        }
        None => false,
    }
}

/// Drop every connection statement naming `id` at either end, anywhere in
/// the tree — what a shape deletion needs, since an edge to a shape that no
/// longer exists renders nothing and warns at build time. The whole item
/// tree is searched because a deletion knows the id, not the container that
/// declared the edge (and an id is diagram-unique). Returns how many were
/// removed.
pub fn remove_connections_touching(items: &mut Vec<Item>, id: &str) -> usize {
    let before = items.len();
    items.retain(|it| !matches!(it, Item::Connection(c) if c.lhs == id || c.rhs == id));
    let mut removed = before - items.len();
    for item in items.iter_mut() {
        if let Item::Block(b) = item {
            removed += remove_connections_touching(&mut b.items, id);
        }
    }
    removed
}

/// Index of the `lhs -> rhs` statement among `items`, if present. Direction
/// matters: `a -> b` and `b -> a` are different edges.
fn find_connection(items: &[Item], lhs: &str, rhs: &str) -> Option<usize> {
    items
        .iter()
        .position(|it| matches!(it, Item::Connection(c) if c.lhs == lhs && c.rhs == rhs))
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

/// Replace the block's decorator with the single-segment name `name` with a
/// fresh one carrying `named` arguments, or remove it entirely when `named`
/// is empty. Existing decorators with other names keep their order; a new
/// decorator appends. Used for visibility toggles (`@except(sites = [:deck])`).
pub fn set_or_remove_decorator(block: &mut ast::Block, name: &str, named: Vec<(String, Expr)>) {
    block
        .decorators
        .retain(|d| !(d.name.len() == 1 && d.name[0] == name));
    if named.is_empty() {
        return;
    }
    block.decorators.push(ast::Decorator {
        name: vec![name.to_string()],
        name_span: Span::new(0, 0),
        positional: Vec::new(),
        positional_spans: Vec::new(),
        named: named
            .into_iter()
            .map(|(name, value)| ast::NamedArg {
                name,
                value,
                span: Span::new(0, 0),
                leading_trivia: Vec::new(),
                trailing_comment: None,
            })
            .collect(),
        span: Span::new(0, 0),
    });
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
    fn remove_field_drops_field_and_tolerates_absent() {
        let mut ast = parse_for_edit(
            "rect {\n  x = 20.0\n  y = 30.0\n  fill = \"#88c\"\n}\n",
            "t",
        )
        .unwrap();
        let span = match &ast.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        let block = find_block_by_span(&mut ast.items, span).unwrap();
        assert!(remove_field(block, "x"));
        assert!(remove_field(block, "y"));
        assert!(!remove_field(block, "y")); // already gone
        assert!(!remove_field(block, "width")); // never present
        let out = format::to_source(&ast);
        assert!(!out.contains("x ="), "{out}");
        assert!(!out.contains("y ="), "{out}");
        assert!(out.contains("fill = \"#88c\""), "{out}");
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
    fn replace_block_preserves_trivia_and_span() {
        let src =
            "page {\n  # keep me\n  card \"a\" {\n    body = \"old\"\n  }\n  card \"b\" {}\n}\n";
        let mut ast = parse_for_edit(src, "t").unwrap();
        let page = match &ast.items[0] {
            Item::Block(b) => b,
            _ => panic!(),
        };
        let a_span = match &page.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        let new = build_block(
            "note",
            &[],
            vec![string_literal_expr("x")],
            vec![("body".to_string(), string_literal_expr("new"))],
        );
        assert!(replace_block_by_span(&mut ast.items, a_span, new));
        // The replacement is findable by the original span before printing.
        assert!(find_block_by_span(&mut ast.items, a_span).is_some());
        let out = format::to_source(&ast);
        assert!(!out.contains("card \"a\""), "{out}");
        assert!(out.contains("note \"x\""), "{out}");
        assert!(out.contains("# keep me"), "leading trivia lost: {out}");
        // Order: the replacement sits where "a" was, before "b".
        let ni = out.find("note \"x\"").unwrap();
        let bi = out.find("card \"b\"").unwrap();
        assert!(ni < bi, "{out}");
        reparse(&out);
        // A missing span leaves the tree untouched.
        let stray = build_block("note", &[], vec![], vec![]);
        assert!(!replace_block_by_span(
            &mut ast.items,
            Span::new(9999, 10000),
            stray
        ));
    }

    #[test]
    fn set_or_remove_decorator_round_trips() {
        let src = "@only(sites = [:book])\ncard \"a\" {\n  body = \"x\"\n}\n";
        let mut ast = parse_for_edit(src, "t").unwrap();
        let span = match &ast.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        let block = find_block_by_span(&mut ast.items, span).unwrap();
        // Add a second decorator with symbol-list args.
        let list = Expr::ListLit {
            elements: vec![Expr::Symbol("deck".into()), Expr::Symbol("print".into())],
            elem_trivia: vec![Default::default(), Default::default()],
            trailing_trivia: Vec::new(),
            span: Span::new(0, 0),
        };
        set_or_remove_decorator(block, "except", vec![("sites".to_string(), list)]);
        let out = format::to_source(&ast);
        assert!(out.contains("@only(sites = [:book])"), "{out}");
        assert!(out.contains("@except(sites = [:deck, :print])"), "{out}");
        reparse(&out);

        // Replace it (single symbol) — the old one goes, order stable.
        let mut ast = parse_for_edit(&out, "t").unwrap();
        let span = match ast.items.iter().find_map(|it| match it {
            Item::Block(b) => Some(b.span),
            _ => None,
        }) {
            Some(s) => s,
            None => panic!(),
        };
        let block = find_block_by_span(&mut ast.items, span).unwrap();
        let list = Expr::ListLit {
            elements: vec![Expr::Symbol("deck".into())],
            elem_trivia: vec![Default::default()],
            trailing_trivia: Vec::new(),
            span: Span::new(0, 0),
        };
        set_or_remove_decorator(block, "except", vec![("sites".to_string(), list)]);
        let out = format::to_source(&ast);
        assert!(out.contains("@except(sites = [:deck])"), "{out}");
        assert!(!out.contains(":print"), "{out}");

        // Empty args ⇒ removed entirely; the other decorator survives.
        let mut ast = parse_for_edit(&out, "t").unwrap();
        let span = match ast.items.iter().find_map(|it| match it {
            Item::Block(b) => Some(b.span),
            _ => None,
        }) {
            Some(s) => s,
            None => panic!(),
        };
        let block = find_block_by_span(&mut ast.items, span).unwrap();
        set_or_remove_decorator(block, "except", Vec::new());
        let out = format::to_source(&ast);
        assert!(!out.contains("@except"), "{out}");
        assert!(out.contains("@only(sites = [:book])"), "{out}");
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

    #[test]
    fn connection_add_remove_round_trips() {
        let mut ast = parse_for_edit("diagram {\n  rect a {}\n  rect b {}\n}\n", "t").unwrap();
        let span = match &ast.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        let block = find_block_by_span(&mut ast.items, span).unwrap();
        assert!(add_connection(block, "a", "b", None));
        assert!(add_connection(block, "b", "a", Some("flow")));
        // The statement list is a set — the same pair twice is one edge.
        assert!(!add_connection(block, "a", "b", None));

        let out = format::to_source(&ast);
        assert!(out.contains("a -> b\n"), "{out}");
        assert!(out.contains("b -> a :flow"), "{out}");
        reparse(&out);

        let block = find_block_by_span(&mut ast.items, span).unwrap();
        assert!(remove_connection(block, "a", "b"));
        assert!(!remove_connection(block, "a", "b"));
        let out = format::to_source(&ast);
        assert!(!out.contains("a -> b"), "{out}");
        assert!(out.contains("b -> a :flow"), "{out}");
        reparse(&out);
    }

    #[test]
    fn removing_a_shape_drops_only_the_edges_touching_it() {
        let mut ast = parse_for_edit(
            "diagram {\n  rect a {}\n  rect b {}\n  rect c {}\n  a -> b\n  b -> c\n  c -> a\n}\n",
            "t",
        )
        .unwrap();
        let span = match &ast.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        let block = find_block_by_span(&mut ast.items, span).unwrap();
        assert_eq!(
            remove_connections_touching(&mut block.items, "b"),
            2,
            "a->b and b->c"
        );
        let out = format::to_source(&ast);
        assert!(out.contains("c -> a"), "the untouched edge survives: {out}");
        assert!(!out.contains("-> b"), "{out}");
        assert!(!out.contains("b ->"), "{out}");
        reparse(&out);
    }

    #[test]
    fn a_connection_keeps_its_direction() {
        let mut ast = parse_for_edit("diagram {\n  a -> b\n}\n", "t").unwrap();
        let span = match &ast.items[0] {
            Item::Block(b) => b.span,
            _ => panic!(),
        };
        let block = find_block_by_span(&mut ast.items, span).unwrap();
        // `b -> a` is a different edge, so it neither dedupes nor removes.
        assert!(add_connection(block, "b", "a", None));
        assert!(remove_connection(block, "b", "a"));
        assert!(!remove_connection(block, "b", "a"));
        assert!(remove_connection(block, "a", "b"));
    }
}
