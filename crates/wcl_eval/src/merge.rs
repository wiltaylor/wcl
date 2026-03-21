use std::collections::HashMap;
use wcl_core::ast::*;
use wcl_core::diagnostic::DiagnosticBag;
// Span is used transitively via wcl_core types but not directly in this module.

/// How to resolve attribute conflicts when merging partial blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictMode {
    /// Duplicate attributes across fragments are an error (default).
    Strict,
    /// Later fragments silently override earlier ones for duplicate attributes.
    LastWins,
}

/// Merges partial block declarations in a WCL document.
///
/// Partial blocks with the same `(kind, inline_id)` are merged into a single
/// block at the position of the first fragment. All subsequent fragments are
/// removed from the document body.
pub struct PartialMerger {
    conflict_mode: ConflictMode,
    diagnostics: DiagnosticBag,
}

impl PartialMerger {
    pub fn new(conflict_mode: ConflictMode) -> Self {
        PartialMerger {
            conflict_mode,
            diagnostics: DiagnosticBag::new(),
        }
    }

    /// Merge all partial declarations in the document.
    ///
    /// Groups top-level blocks by `(kind, inline_id)` where `partial=true`,
    /// merges each group into a single block, and replaces all fragments with
    /// the merged block at the position of the first fragment.
    pub fn merge(&mut self, doc: &mut Document) {
        // Phase 1: Identify groups of partial blocks by (kind, inline_id)
        // We need the inline_id as a string key for grouping.
        let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
        let mut order: Vec<(String, String)> = Vec::new();

        for (idx, item) in doc.items.iter().enumerate() {
            if let DocItem::Body(BodyItem::Block(block)) = item {
                if block.partial {
                    if let Some(id_str) = inline_id_to_string(&block.inline_id) {
                        let key = (block.kind.name.clone(), id_str);
                        let entry = groups.entry(key.clone()).or_insert_with(|| {
                            order.push(key.clone());
                            Vec::new()
                        });
                        entry.push(idx);
                    } else {
                        // Partial blocks without inline IDs: warn but don't merge
                        self.diagnostics.add(wcl_core::Diagnostic::warning(
                            "partial block without inline ID cannot be merged",
                            block.span,
                        ));
                    }
                }
            }
        }

        // E033: Check for mixed partial/non-partial with same ID
        for item in doc.items.iter() {
            if let DocItem::Body(BodyItem::Block(block)) = item {
                if !block.partial {
                    if let Some(id_str) = inline_id_to_string(&block.inline_id) {
                        let key = (block.kind.name.clone(), id_str);
                        if groups.contains_key(&key) {
                            self.diagnostics.error_with_code(
                                format!(
                                    "block {}#{} is declared both as partial and non-partial",
                                    key.0, key.1
                                ),
                                block.span,
                                "E033",
                            );
                        }
                    }
                }
            }
        }

        // Collect @partial_requires from all fragments in each group
        let mut group_requires: HashMap<(String, String), Vec<String>> = HashMap::new();
        for key in &order {
            let indices = &groups[key];
            let mut required_fields: Vec<String> = Vec::new();
            for &idx in indices {
                if let DocItem::Body(BodyItem::Block(block)) = &doc.items[idx] {
                    for decorator in &block.decorators {
                        if decorator.name.name == "partial_requires" {
                            if let Some(DecoratorArg::Positional(Expr::List(items, _))) =
                                decorator.args.first()
                            {
                                for item in items {
                                    if let Expr::StringLit(s) = item {
                                        for part in &s.parts {
                                            if let StringPart::Literal(field_name) = part {
                                                required_fields.push(field_name.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !required_fields.is_empty() {
                group_requires.insert(key.clone(), required_fields);
            }
        }

        // Phase 2: For each group with more than one fragment, merge them
        // We must process groups and remove indices carefully.
        // Collect all indices to remove (all fragments except the first in each group).
        let mut indices_to_remove: Vec<usize> = Vec::new();
        let mut merge_replacements: Vec<(usize, Block)> = Vec::new();

        for key in &order {
            let indices = &groups[key];
            if indices.len() < 2 {
                // Single partial block: just clear the partial flag
                if let Some(&idx) = indices.first() {
                    if let DocItem::Body(BodyItem::Block(block)) = &doc.items[idx] {
                        let mut merged = block.clone();
                        merged.partial = false;
                        merge_replacements.push((idx, merged));
                    }
                }
                continue;
            }

            // Collect all blocks in this group
            let blocks: Vec<Block> = indices
                .iter()
                .filter_map(|&idx| {
                    if let DocItem::Body(BodyItem::Block(block)) = &doc.items[idx] {
                        Some(block.clone())
                    } else {
                        None
                    }
                })
                .collect();

            if blocks.is_empty() {
                continue;
            }

            // Sort blocks by @merge_order if present
            let mut sorted_blocks = blocks;
            sorted_blocks.sort_by(|a, b| {
                let order_a = get_merge_order(a);
                let order_b = get_merge_order(b);
                order_a.cmp(&order_b)
            });

            // Merge the sorted blocks
            let merged = self.merge_blocks(&sorted_blocks);

            // First index gets the merged block; rest are removed
            let first_idx = indices[0];
            merge_replacements.push((first_idx, merged));
            for &idx in &indices[1..] {
                indices_to_remove.push(idx);
            }
        }

        // Phase 3: Apply replacements and removals
        // Sort removal indices in descending order for safe removal
        indices_to_remove.sort_unstable();
        indices_to_remove.dedup();

        // Apply replacements first (they don't change indices)
        for (idx, merged_block) in &merge_replacements {
            doc.items[*idx] = DocItem::Body(BodyItem::Block(merged_block.clone()));
        }

        // Remove in reverse order
        for idx in indices_to_remove.into_iter().rev() {
            doc.items.remove(idx);
        }

        // Phase 4: Validate @partial_requires
        for (key, required_fields) in &group_requires {
            let merged_block = doc.items.iter().find_map(|item| {
                if let DocItem::Body(BodyItem::Block(block)) = item {
                    if block.kind.name == key.0 {
                        if let Some(id_str) = inline_id_to_string(&block.inline_id) {
                            if id_str == key.1 {
                                return Some(block);
                            }
                        }
                    }
                }
                None
            });

            if let Some(block) = merged_block {
                for field in required_fields {
                    let has_attr = block.body.iter().any(|item| {
                        matches!(item, BodyItem::Attribute(attr) if attr.name.name == *field)
                    });
                    let has_child = block
                        .body
                        .iter()
                        .any(|item| matches!(item, BodyItem::Block(b) if b.kind.name == *field));
                    if !has_attr && !has_child {
                        self.diagnostics.error(
                            format!(
                                "@partial_requires: field '{}' is missing after merge of {}#{}",
                                field, key.0, key.1
                            ),
                            block.span,
                        );
                    }
                }
            }
        }
    }

    /// Merge multiple partial blocks into a single block.
    fn merge_blocks(&mut self, blocks: &[Block]) -> Block {
        assert!(!blocks.is_empty(), "merge_blocks called with empty slice");

        let first = &blocks[0];
        let mut merged = Block {
            decorators: Vec::new(),
            partial: false,
            kind: first.kind.clone(),
            inline_id: first.inline_id.clone(),
            labels: first.labels.clone(),
            body: Vec::new(),
            text_content: first.text_content.clone(),
            trivia: first.trivia.clone(),
            span: first.span,
        };

        // Merge decorators (deduplicate by name)
        let mut seen_decorators: HashMap<String, Decorator> = HashMap::new();
        for block in blocks {
            for decorator in &block.decorators {
                let name = &decorator.name.name;
                if name == "merge_order" || name == "partial_requires" {
                    // Skip merge-related decorators, they don't carry forward
                    continue;
                }
                if seen_decorators.contains_key(name) {
                    match self.conflict_mode {
                        ConflictMode::Strict => {
                            self.diagnostics.error_with_code(
                                format!("duplicate decorator '@{}' in partial merge", name),
                                decorator.span,
                                "E031",
                            );
                        }
                        ConflictMode::LastWins => {
                            seen_decorators.insert(name.clone(), decorator.clone());
                        }
                    }
                } else {
                    seen_decorators.insert(name.clone(), decorator.clone());
                }
            }
        }
        merged.decorators = seen_decorators.into_values().collect();

        // Merge body items
        // Track attributes by name and child blocks by (kind, inline_id)
        let mut seen_attrs: HashMap<String, usize> = HashMap::new(); // name -> index in merged.body
        let mut child_block_groups: HashMap<(String, String), Vec<Block>> = HashMap::new();
        let mut child_block_order: Vec<(String, String)> = Vec::new();

        for block in blocks {
            // Warn about label mismatches
            if !block.labels.is_empty()
                && !first.labels.is_empty()
                && !labels_match(&first.labels, &block.labels)
            {
                self.diagnostics.add(
                    wcl_core::Diagnostic::warning(
                        "mismatched labels in partial block fragments",
                        block.span,
                    )
                    .with_code("W003"),
                );
            }

            for item in &block.body {
                match item {
                    BodyItem::Attribute(attr) => {
                        let name = &attr.name.name;
                        if let Some(&existing_idx) = seen_attrs.get(name) {
                            match self.conflict_mode {
                                ConflictMode::Strict => {
                                    self.diagnostics.error_with_code(
                                        format!("duplicate attribute '{}' in partial merge", name),
                                        attr.span,
                                        "E031",
                                    );
                                }
                                ConflictMode::LastWins => {
                                    merged.body[existing_idx] = item.clone();
                                }
                            }
                        } else {
                            seen_attrs.insert(name.clone(), merged.body.len());
                            merged.body.push(item.clone());
                        }
                    }
                    BodyItem::Block(child_block) => {
                        if let Some(child_id) = inline_id_to_string(&child_block.inline_id) {
                            let key = (child_block.kind.name.clone(), child_id);
                            let entry =
                                child_block_groups.entry(key.clone()).or_insert_with(|| {
                                    child_block_order.push(key.clone());
                                    Vec::new()
                                });
                            entry.push(child_block.clone());
                        } else {
                            // Child blocks without inline IDs are just appended
                            merged.body.push(item.clone());
                        }
                    }
                    // Other body items (let bindings, tables, etc.) are appended
                    other => {
                        merged.body.push(other.clone());
                    }
                }
            }
        }

        // Recursively merge grouped child blocks
        for key in &child_block_order {
            let children = &child_block_groups[key];
            if children.len() == 1 {
                merged.body.push(BodyItem::Block(children[0].clone()));
            } else {
                let merged_child = self.merge_blocks(children);
                merged.body.push(BodyItem::Block(merged_child));
            }
        }

        merged
    }

    /// Consume the merger and return accumulated diagnostics.
    pub fn into_diagnostics(self) -> DiagnosticBag {
        self.diagnostics
    }
}

/// Extract a string representation from an `InlineId` for grouping purposes.
fn inline_id_to_string(id: &Option<InlineId>) -> Option<String> {
    match id {
        Some(InlineId::Literal(lit)) => Some(lit.value.clone()),
        Some(InlineId::Interpolated(parts)) => {
            // For interpolated IDs, we can only group by the literal parts.
            // If there are interpolations, the ID is dynamic and cannot be
            // reliably grouped at this stage.
            let mut result = String::new();
            for part in parts {
                match part {
                    StringPart::Literal(s) => result.push_str(s),
                    StringPart::Interpolation(_) => {
                        // Dynamic ID — cannot merge at this stage
                        return None;
                    }
                }
            }
            Some(result)
        }
        None => None,
    }
}

/// Extract the `@merge_order(n)` value from a block's decorators.
/// Returns `i64::MAX` if not present (so unordered blocks sort last).
fn get_merge_order(block: &Block) -> i64 {
    for decorator in &block.decorators {
        if decorator.name.name == "merge_order" {
            if let Some(DecoratorArg::Positional(Expr::IntLit(n, _))) = decorator.args.first() {
                return *n;
            }
        }
    }
    i64::MAX
}

/// Check whether two label lists match (by comparing their literal content).
fn labels_match(a: &[StringLit], b: &[StringLit]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (la, lb) in a.iter().zip(b.iter()) {
        let sa = string_lit_to_plain(la);
        let sb = string_lit_to_plain(lb);
        if sa != sb {
            return false;
        }
    }
    true
}

/// Extract plain text from a StringLit (ignoring interpolations).
fn string_lit_to_plain(lit: &StringLit) -> String {
    let mut result = String::new();
    for part in &lit.parts {
        match part {
            StringPart::Literal(s) => result.push_str(s),
            StringPart::Interpolation(_) => result.push_str("<interp>"),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_core::span::{FileId, Span};
    use wcl_core::trivia::Trivia;

    fn dummy_span() -> Span {
        Span::new(FileId(0), 0, 0)
    }

    fn make_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: dummy_span(),
        }
    }

    fn make_id_lit(value: &str) -> IdentifierLit {
        IdentifierLit {
            value: value.to_string(),
            span: dummy_span(),
        }
    }

    fn make_partial_block(kind: &str, id: &str, attrs: Vec<(&str, Expr)>) -> Block {
        Block {
            decorators: vec![],
            partial: true,
            kind: make_ident(kind),
            inline_id: Some(InlineId::Literal(make_id_lit(id))),
            labels: vec![],
            body: attrs
                .into_iter()
                .map(|(name, value)| {
                    BodyItem::Attribute(Attribute {
                        decorators: vec![],
                        name: make_ident(name),
                        value,
                        trivia: Trivia::empty(),
                        span: dummy_span(),
                    })
                })
                .collect(),
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        }
    }

    fn make_doc(blocks: Vec<Block>) -> Document {
        Document {
            items: blocks
                .into_iter()
                .map(|b| DocItem::Body(BodyItem::Block(b)))
                .collect(),
            trivia: Trivia::empty(),
            span: dummy_span(),
        }
    }

    #[test]
    fn merge_two_partial_blocks_strict() {
        let block1 = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );
        let block2 = make_partial_block(
            "service",
            "svc-api",
            vec![("replicas", Expr::IntLit(3, dummy_span()))],
        );

        let mut doc = make_doc(vec![block1, block2]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        assert!(!merger.diagnostics.has_errors());
        // Should have one merged block
        assert_eq!(doc.items.len(), 1);

        if let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] {
            assert!(!block.partial);
            assert_eq!(block.kind.name, "service");
            // Should have both attributes
            assert_eq!(block.body.len(), 2);
            let attr_names: Vec<&str> = block
                .body
                .iter()
                .filter_map(|item| {
                    if let BodyItem::Attribute(attr) = item {
                        Some(attr.name.name.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            assert!(attr_names.contains(&"port"));
            assert!(attr_names.contains(&"replicas"));
        } else {
            panic!("expected a Block");
        }
    }

    #[test]
    fn merge_strict_duplicate_attribute_error() {
        let block1 = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );
        let block2 = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(9090, dummy_span()))],
        );

        let mut doc = make_doc(vec![block1, block2]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        assert!(merger.diagnostics.has_errors());
        assert_eq!(merger.diagnostics.error_count(), 1);
    }

    #[test]
    fn merge_last_wins_duplicate_attribute() {
        let block1 = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );
        let block2 = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(9090, dummy_span()))],
        );

        let mut doc = make_doc(vec![block1, block2]);
        let mut merger = PartialMerger::new(ConflictMode::LastWins);
        merger.merge(&mut doc);

        assert!(!merger.diagnostics.has_errors());
        assert_eq!(doc.items.len(), 1);

        if let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] {
            assert_eq!(block.body.len(), 1);
            if let BodyItem::Attribute(attr) = &block.body[0] {
                match &attr.value {
                    Expr::IntLit(9090, _) => {} // Last wins
                    other => panic!("expected 9090, got {:?}", other),
                }
            }
        } else {
            panic!("expected a Block");
        }
    }

    #[test]
    fn merge_different_ids_not_merged() {
        let block1 = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );
        let block2 = make_partial_block(
            "service",
            "svc-db",
            vec![("port", Expr::IntLit(5432, dummy_span()))],
        );

        let mut doc = make_doc(vec![block1, block2]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        assert!(!merger.diagnostics.has_errors());
        // Two separate blocks (each single partial cleared)
        assert_eq!(doc.items.len(), 2);
    }

    #[test]
    fn merge_three_fragments() {
        let block1 = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );
        let block2 = make_partial_block(
            "service",
            "svc-api",
            vec![("replicas", Expr::IntLit(3, dummy_span()))],
        );
        let block3 = make_partial_block(
            "service",
            "svc-api",
            vec![(
                "env",
                Expr::StringLit(StringLit {
                    parts: vec![StringPart::Literal("prod".to_string())],
                    span: dummy_span(),
                }),
            )],
        );

        let mut doc = make_doc(vec![block1, block2, block3]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        assert!(!merger.diagnostics.has_errors());
        assert_eq!(doc.items.len(), 1);

        if let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] {
            assert_eq!(block.body.len(), 3);
        } else {
            panic!("expected a Block");
        }
    }

    #[test]
    fn single_partial_block_clears_partial_flag() {
        let block = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );

        let mut doc = make_doc(vec![block]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        assert!(!merger.diagnostics.has_errors());
        assert_eq!(doc.items.len(), 1);

        if let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] {
            assert!(!block.partial);
        } else {
            panic!("expected a Block");
        }
    }

    #[test]
    fn conflict_mode_equality() {
        assert_eq!(ConflictMode::Strict, ConflictMode::Strict);
        assert_eq!(ConflictMode::LastWins, ConflictMode::LastWins);
        assert_ne!(ConflictMode::Strict, ConflictMode::LastWins);
    }

    #[test]
    fn merge_child_blocks_with_same_inline_id() {
        // Create two partial blocks each with a child block "monitoring" with the same ID
        let child1 = Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("monitoring"),
            inline_id: Some(InlineId::Literal(make_id_lit("mon-1"))),
            labels: vec![],
            body: vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("interval"),
                value: Expr::IntLit(15, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            })],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let child2 = Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("monitoring"),
            inline_id: Some(InlineId::Literal(make_id_lit("mon-1"))),
            labels: vec![],
            body: vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("threshold"),
                value: Expr::FloatLit(0.99, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            })],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let block1 = Block {
            decorators: vec![],
            partial: true,
            kind: make_ident("service"),
            inline_id: Some(InlineId::Literal(make_id_lit("svc-api"))),
            labels: vec![],
            body: vec![
                BodyItem::Attribute(Attribute {
                    decorators: vec![],
                    name: make_ident("port"),
                    value: Expr::IntLit(8080, dummy_span()),
                    trivia: Trivia::empty(),
                    span: dummy_span(),
                }),
                BodyItem::Block(child1),
            ],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let block2 = Block {
            decorators: vec![],
            partial: true,
            kind: make_ident("service"),
            inline_id: Some(InlineId::Literal(make_id_lit("svc-api"))),
            labels: vec![],
            body: vec![BodyItem::Block(child2)],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let mut doc = make_doc(vec![block1, block2]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        assert!(!merger.diagnostics.has_errors());
        assert_eq!(doc.items.len(), 1);

        if let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] {
            // Should have port attribute + merged monitoring child
            let attr_count = block
                .body
                .iter()
                .filter(|i| matches!(i, BodyItem::Attribute(_)))
                .count();
            let block_count = block
                .body
                .iter()
                .filter(|i| matches!(i, BodyItem::Block(_)))
                .count();

            assert_eq!(attr_count, 1); // port
            assert_eq!(block_count, 1); // merged monitoring

            // Check the merged child has both attributes
            if let Some(BodyItem::Block(child)) =
                block.body.iter().find(|i| matches!(i, BodyItem::Block(_)))
            {
                assert_eq!(child.body.len(), 2); // interval + threshold
            }
        } else {
            panic!("expected a Block");
        }
    }

    #[test]
    fn partial_requires_satisfied() {
        let mut block1 = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );
        block1.decorators.push(Decorator {
            name: make_ident("partial_requires"),
            args: vec![DecoratorArg::Positional(Expr::List(
                vec![Expr::StringLit(StringLit {
                    parts: vec![StringPart::Literal("tls".to_string())],
                    span: dummy_span(),
                })],
                dummy_span(),
            ))],
            span: dummy_span(),
        });

        let block2 = make_partial_block(
            "service",
            "svc-api",
            vec![("tls", Expr::BoolLit(true, dummy_span()))],
        );

        let mut doc = make_doc(vec![block1, block2]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        assert!(!merger.diagnostics.has_errors());
    }

    #[test]
    fn partial_requires_missing_field() {
        let mut block1 = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );
        block1.decorators.push(Decorator {
            name: make_ident("partial_requires"),
            args: vec![DecoratorArg::Positional(Expr::List(
                vec![
                    Expr::StringLit(StringLit {
                        parts: vec![StringPart::Literal("tls".to_string())],
                        span: dummy_span(),
                    }),
                    Expr::StringLit(StringLit {
                        parts: vec![StringPart::Literal("monitoring".to_string())],
                        span: dummy_span(),
                    }),
                ],
                dummy_span(),
            ))],
            span: dummy_span(),
        });

        let block2 = make_partial_block(
            "service",
            "svc-api",
            vec![("tls", Expr::BoolLit(true, dummy_span()))],
        );

        let mut doc = make_doc(vec![block1, block2]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        // "monitoring" is missing, so there should be an error
        assert!(merger.diagnostics.has_errors());
        assert_eq!(merger.diagnostics.error_count(), 1);
    }

    #[test]
    fn partial_requires_child_block_satisfies() {
        let mut block1 = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );
        block1.decorators.push(Decorator {
            name: make_ident("partial_requires"),
            args: vec![DecoratorArg::Positional(Expr::List(
                vec![Expr::StringLit(StringLit {
                    parts: vec![StringPart::Literal("monitoring".to_string())],
                    span: dummy_span(),
                })],
                dummy_span(),
            ))],
            span: dummy_span(),
        });

        let mut block2 = make_partial_block("service", "svc-api", vec![]);
        // Add a child block of kind "monitoring"
        block2.body.push(BodyItem::Block(Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("monitoring"),
            inline_id: None,
            labels: vec![],
            body: vec![],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        }));

        let mut doc = make_doc(vec![block1, block2]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        assert!(!merger.diagnostics.has_errors());
    }

    #[test]
    fn e033_mixed_partial_and_non_partial_same_id() {
        let partial_block = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );
        let non_partial_block = Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("service"),
            inline_id: Some(InlineId::Literal(make_id_lit("svc-api"))),
            labels: vec![],
            body: vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("replicas"),
                value: Expr::IntLit(3, dummy_span()),
                trivia: Trivia::empty(),
                span: dummy_span(),
            })],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let mut doc = make_doc(vec![partial_block, non_partial_block]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        assert!(merger.diagnostics.has_errors());
        let errors: Vec<_> = merger
            .diagnostics
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E033"))
            .collect();
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .message
            .contains("declared both as partial and non-partial"));
    }

    #[test]
    fn e033_no_error_when_ids_differ() {
        let partial_block = make_partial_block(
            "service",
            "svc-api",
            vec![("port", Expr::IntLit(8080, dummy_span()))],
        );
        let non_partial_block = Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("service"),
            inline_id: Some(InlineId::Literal(make_id_lit("svc-db"))),
            labels: vec![],
            body: vec![],
            text_content: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let mut doc = make_doc(vec![partial_block, non_partial_block]);
        let mut merger = PartialMerger::new(ConflictMode::Strict);
        merger.merge(&mut doc);

        let e033_errors: Vec<_> = merger
            .diagnostics
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E033"))
            .collect();
        assert_eq!(e033_errors.len(), 0);
    }
}
