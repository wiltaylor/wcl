use crate::lang::ast::*;

use crate::eval::value::*;

/// The query engine executes query pipelines against a set of block references.
pub struct QueryEngine;

impl QueryEngine {
    pub fn new() -> Self {
        QueryEngine
    }

    /// Execute a query pipeline against a set of blocks.
    ///
    /// The optional evaluator and scope are provided so that filter expressions
    /// containing attribute comparisons can be evaluated at runtime.
    pub fn execute(
        &self,
        pipeline: &QueryPipeline,
        blocks: &[BlockRef],
        evaluator: &mut super::evaluator::Evaluator,
        scope_id: ScopeId,
    ) -> Result<Value, String> {
        // 1. Apply selector to get initial set
        let mut results = self.apply_selector(&pipeline.selector, blocks)?;

        // 2. Apply filters in sequence
        let mut projection = None;
        for filter in &pipeline.filters {
            match filter {
                QueryFilter::Projection(field) => {
                    projection = Some(field.name.clone());
                }
                _ => {
                    results = self.apply_filter(filter, &results, evaluator, scope_id)?;
                }
            }
        }

        // 3. Apply projection if present
        if let Some(field) = projection {
            let projected: Result<Vec<Value>, String> = results
                .iter()
                .map(|block| {
                    block.attributes.get(&field).cloned().ok_or_else(|| {
                        format!("attribute '{}' not found in block {}", field, block.kind)
                    })
                })
                .collect();
            Ok(Value::List(projected?))
        } else {
            Ok(Value::List(
                results.into_iter().map(Value::BlockRef).collect(),
            ))
        }
    }

    fn apply_selector(
        &self,
        selector: &QuerySelector,
        blocks: &[BlockRef],
    ) -> Result<Vec<BlockRef>, String> {
        match selector {
            QuerySelector::Kind(kind) => Ok(blocks
                .iter()
                .filter(|b| b.kind == kind.name)
                .cloned()
                .collect()),
            QuerySelector::KindId(kind, id) => Ok(blocks
                .iter()
                .filter(|b| b.kind == kind.name && b.id.as_deref() == Some(&id.value))
                .cloned()
                .collect()),
            QuerySelector::Recursive(kind) => {
                let mut results = Vec::new();
                self.find_recursive(blocks, &kind.name, None, &mut results);
                Ok(results)
            }
            QuerySelector::RecursiveId(kind, id) => {
                let mut results = Vec::new();
                self.find_recursive(blocks, &kind.name, Some(&id.value), &mut results);
                Ok(results)
            }
            QuerySelector::Wildcard => Ok(blocks.to_vec()),
            QuerySelector::Root => Ok(blocks.to_vec()),
            QuerySelector::Path(segments) => {
                let mut current = blocks.to_vec();
                for (i, segment) in segments.iter().enumerate() {
                    match segment {
                        PathSegment::Ident(ident) => {
                            if i == 0 {
                                // First segment: filter top-level blocks by kind
                                current = current
                                    .iter()
                                    .filter(|b| b.kind == ident.name)
                                    .cloned()
                                    .collect();
                            } else {
                                // Subsequent segments: descend into children
                                current = current
                                    .iter()
                                    .flat_map(|b| b.children.iter().cloned())
                                    .filter(|b| b.kind == ident.name)
                                    .collect();
                            }
                        }
                    }
                }
                Ok(current)
            }
            QuerySelector::TableId(id) => {
                let mut results = Vec::new();
                self.find_table_by_id(blocks, &id.value, &mut results);
                Ok(results)
            }
        }
    }

    fn find_recursive(
        &self,
        blocks: &[BlockRef],
        kind: &str,
        id: Option<&str>,
        results: &mut Vec<BlockRef>,
    ) {
        for block in blocks {
            if block.kind == kind && id.is_none_or(|i| block.id.as_deref() == Some(i)) {
                results.push(block.clone());
            }
            self.find_recursive(&block.children, kind, id, results);
        }
    }

    fn find_table_by_id(&self, blocks: &[BlockRef], id: &str, results: &mut Vec<BlockRef>) {
        for block in blocks {
            if block.kind == "table" && block.id.as_deref() == Some(id) {
                results.extend(block.children.clone());
            }
            self.find_table_by_id(&block.children, id, results);
        }
    }

    fn apply_filter(
        &self,
        filter: &QueryFilter,
        blocks: &[BlockRef],
        evaluator: &mut super::evaluator::Evaluator,
        scope_id: ScopeId,
    ) -> Result<Vec<BlockRef>, String> {
        match filter {
            QueryFilter::AttrComparison(attr, op, expr) => {
                // Evaluate the comparison expression
                let rhs_val = evaluator
                    .eval_expr(expr, scope_id)
                    .map_err(|d| d.message.clone())?;

                let mut matched = Vec::new();
                for block in blocks {
                    // `.id` is a virtual attribute that compares against the
                    // block's inline ID rather than a real attribute named "id".
                    if attr.name == "id" {
                        let id_val = block
                            .id
                            .as_ref()
                            .map(|s| Value::String(s.clone()))
                            .unwrap_or(Value::Null);
                        let matches = match op {
                            BinOp::Eq => id_val == rhs_val,
                            BinOp::Neq => id_val != rhs_val,
                            _ => false,
                        };
                        if matches {
                            matched.push(block.clone());
                        }
                        continue;
                    }
                    if let Some(attr_val) = block.attributes.get(&attr.name) {
                        let matches = match op {
                            BinOp::Eq => *attr_val == rhs_val,
                            BinOp::Neq => *attr_val != rhs_val,
                            BinOp::Lt => value_compare(attr_val, &rhs_val)
                                .is_some_and(|o| o == std::cmp::Ordering::Less),
                            BinOp::Gt => value_compare(attr_val, &rhs_val)
                                .is_some_and(|o| o == std::cmp::Ordering::Greater),
                            BinOp::Lte => value_compare(attr_val, &rhs_val)
                                .is_some_and(|o| o != std::cmp::Ordering::Greater),
                            BinOp::Gte => value_compare(attr_val, &rhs_val)
                                .is_some_and(|o| o != std::cmp::Ordering::Less),
                            BinOp::Match => {
                                if let (Value::String(s), Value::String(pattern)) =
                                    (attr_val, &rhs_val)
                                {
                                    regex::Regex::new(pattern)
                                        .map(|re| re.is_match(s))
                                        .unwrap_or(false)
                                } else {
                                    false
                                }
                            }
                            _ => false,
                        };
                        if matches {
                            matched.push(block.clone());
                        }
                    }
                }
                Ok(matched)
            }
            QueryFilter::HasAttr(attr) => Ok(blocks
                .iter()
                .filter(|b| b.attributes.contains_key(&attr.name))
                .cloned()
                .collect()),
            QueryFilter::HasDecorator(dec) => Ok(blocks
                .iter()
                .filter(|b| b.decorators.iter().any(|d| d.name == dec.name))
                .cloned()
                .collect()),
            QueryFilter::DecoratorArgFilter(dec, param, op, expr) => {
                let rhs_val = evaluator
                    .eval_expr(expr, scope_id)
                    .map_err(|d| d.message.clone())?;

                let mut matched = Vec::new();
                for block in blocks {
                    for d in &block.decorators {
                        if d.name == dec.name {
                            if let Some(arg_val) = d.args.get(&param.name) {
                                let matches = match op {
                                    BinOp::Eq => *arg_val == rhs_val,
                                    BinOp::Neq => *arg_val != rhs_val,
                                    BinOp::Lt => {
                                        value_compare(arg_val, &rhs_val)
                                            == Some(std::cmp::Ordering::Less)
                                    }
                                    BinOp::Gt => {
                                        value_compare(arg_val, &rhs_val)
                                            == Some(std::cmp::Ordering::Greater)
                                    }
                                    BinOp::Lte => matches!(
                                        value_compare(arg_val, &rhs_val),
                                        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                                    ),
                                    BinOp::Gte => matches!(
                                        value_compare(arg_val, &rhs_val),
                                        Some(
                                            std::cmp::Ordering::Greater | std::cmp::Ordering::Equal
                                        )
                                    ),
                                    _ => false,
                                };
                                if matches {
                                    matched.push(block.clone());
                                    break;
                                }
                            }
                        }
                    }
                }
                Ok(matched)
            }
            QueryFilter::Projection(_) => {
                // Projections are handled separately in execute()
                Ok(blocks.to_vec())
            }
        }
    }
}

impl Default for QueryEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Compare two Values for ordering purposes.
fn value_compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.partial_cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
        (Value::Int(x), Value::Float(y)) => (*x as f64).partial_cmp(y),
        (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::evaluator::Evaluator;
    use crate::eval::scope::ScopeKind;
    use crate::lang::span::{FileId, Span};
    use indexmap::IndexMap;

    fn ds() -> Span {
        Span::new(FileId(0), 0, 0)
    }

    fn mk_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: ds(),
        }
    }

    fn mk_block(kind: &str, id: Option<&str>, attrs: Vec<(&str, Value)>) -> BlockRef {
        let mut attributes = IndexMap::new();
        for (k, v) in attrs {
            attributes.insert(k.to_string(), v);
        }
        BlockRef {
            kind: kind.to_string(),
            id: id.map(|s| s.to_string()),
            qualified_id: None,
            attributes,
            children: Vec::new(),
            decorators: Vec::new(),
            span: ds(),
        }
    }

    #[test]
    fn query_kind_selector() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let blocks = vec![
            mk_block("service", Some("web"), vec![("port", Value::Int(8080))]),
            mk_block("service", Some("api"), vec![("port", Value::Int(9090))]),
            mk_block("database", Some("pg"), vec![("port", Value::Int(5432))]),
        ];

        let engine = QueryEngine::new();
        let pipeline = QueryPipeline {
            selector: QuerySelector::Kind(mk_ident("service")),
            filters: vec![],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                // Both should be service blocks
                for item in &items {
                    if let Value::BlockRef(br) = item {
                        assert_eq!(br.kind, "service");
                    } else {
                        panic!("expected BlockRef");
                    }
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn query_kind_id_selector() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let blocks = vec![
            mk_block("service", Some("web"), vec![("port", Value::Int(8080))]),
            mk_block("service", Some("api"), vec![("port", Value::Int(9090))]),
        ];

        let engine = QueryEngine::new();
        let pipeline = QueryPipeline {
            selector: QuerySelector::KindId(
                mk_ident("service"),
                IdentifierLit {
                    value: "web".to_string(),
                    span: ds(),
                },
            ),
            filters: vec![],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                if let Value::BlockRef(br) = &items[0] {
                    assert_eq!(br.id.as_deref(), Some("web"));
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn query_wildcard_selector() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let blocks = vec![
            mk_block("service", Some("web"), vec![]),
            mk_block("database", Some("pg"), vec![]),
        ];

        let engine = QueryEngine::new();
        let pipeline = QueryPipeline {
            selector: QuerySelector::Wildcard,
            filters: vec![],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn query_has_attr_filter() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let blocks = vec![
            mk_block("service", Some("web"), vec![("port", Value::Int(8080))]),
            mk_block("service", Some("api"), vec![]),
        ];

        let engine = QueryEngine::new();
        let pipeline = QueryPipeline {
            selector: QuerySelector::Kind(mk_ident("service")),
            filters: vec![QueryFilter::HasAttr(mk_ident("port"))],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                if let Value::BlockRef(br) = &items[0] {
                    assert_eq!(br.id.as_deref(), Some("web"));
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn query_projection() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let blocks = vec![
            mk_block("service", Some("web"), vec![("port", Value::Int(8080))]),
            mk_block("service", Some("api"), vec![("port", Value::Int(9090))]),
        ];

        let engine = QueryEngine::new();
        let pipeline = QueryPipeline {
            selector: QuerySelector::Kind(mk_ident("service")),
            filters: vec![QueryFilter::Projection(mk_ident("port"))],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(8080), Value::Int(9090)])
        );
    }

    #[test]
    fn query_attr_comparison_filter() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let blocks = vec![
            mk_block("service", Some("web"), vec![("port", Value::Int(8080))]),
            mk_block("service", Some("api"), vec![("port", Value::Int(9090))]),
        ];

        let engine = QueryEngine::new();
        let pipeline = QueryPipeline {
            selector: QuerySelector::Kind(mk_ident("service")),
            filters: vec![QueryFilter::AttrComparison(
                mk_ident("port"),
                BinOp::Gt,
                Expr::IntLit(8500, ds()),
            )],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                if let Value::BlockRef(br) = &items[0] {
                    assert_eq!(br.id.as_deref(), Some("api"));
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn query_recursive_selector() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let inner = mk_block(
            "endpoint",
            Some("health"),
            vec![("path", Value::String("/health".to_string()))],
        );
        let mut outer = mk_block("service", Some("web"), vec![("port", Value::Int(8080))]);
        outer.children.push(inner);

        let blocks = vec![
            outer,
            mk_block(
                "endpoint",
                Some("root"),
                vec![("path", Value::String("/".to_string()))],
            ),
        ];

        let engine = QueryEngine::new();
        let pipeline = QueryPipeline {
            selector: QuerySelector::Recursive(mk_ident("endpoint")),
            filters: vec![],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn query_path_selector_navigates_nested() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let firewall = mk_block(
            "firewall",
            Some("fw1"),
            vec![("enabled", Value::Bool(true))],
        );
        let mut network = mk_block("network", None, vec![]);
        network.children.push(firewall);

        let blocks = vec![network, mk_block("service", Some("web"), vec![])];

        let engine = QueryEngine::new();
        let pipeline = QueryPipeline {
            selector: QuerySelector::Path(vec![
                PathSegment::Ident(mk_ident("network")),
                PathSegment::Ident(mk_ident("firewall")),
            ]),
            filters: vec![],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                if let Value::BlockRef(br) = &items[0] {
                    assert_eq!(br.kind, "firewall");
                    assert_eq!(br.id.as_deref(), Some("fw1"));
                } else {
                    panic!("expected BlockRef");
                }
            }
            _ => panic!("expected list"),
        }
    }

    fn mk_block_with_decorators(
        kind: &str,
        id: Option<&str>,
        attrs: Vec<(&str, Value)>,
        decorators: Vec<DecoratorValue>,
    ) -> BlockRef {
        let mut b = mk_block(kind, id, attrs);
        b.decorators = decorators;
        b
    }

    fn mk_decorator(name: &str, args: Vec<(&str, Value)>) -> DecoratorValue {
        let mut arg_map = IndexMap::new();
        for (k, v) in args {
            arg_map.insert(k.to_string(), v);
        }
        DecoratorValue {
            name: name.to_string(),
            args: arg_map,
        }
    }

    #[test]
    fn decorator_arg_filter_gt() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let blocks = vec![
            mk_block_with_decorators(
                "field",
                Some("age"),
                vec![],
                vec![mk_decorator("validate", vec![("min", Value::Int(5))])],
            ),
            mk_block_with_decorators(
                "field",
                Some("score"),
                vec![],
                vec![mk_decorator("validate", vec![("min", Value::Int(0))])],
            ),
        ];

        let engine = QueryEngine::new();
        // @validate.min > 0
        let pipeline = QueryPipeline {
            selector: QuerySelector::Kind(mk_ident("field")),
            filters: vec![QueryFilter::DecoratorArgFilter(
                mk_ident("validate"),
                mk_ident("min"),
                BinOp::Gt,
                Expr::IntLit(0, ds()),
            )],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                if let Value::BlockRef(br) = &items[0] {
                    assert_eq!(br.id.as_deref(), Some("age"));
                } else {
                    panic!("expected BlockRef");
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn decorator_arg_filter_lt() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let blocks = vec![
            mk_block_with_decorators(
                "field",
                Some("name"),
                vec![],
                vec![mk_decorator("validate", vec![("max", Value::Int(50))])],
            ),
            mk_block_with_decorators(
                "field",
                Some("bio"),
                vec![],
                vec![mk_decorator("validate", vec![("max", Value::Int(200))])],
            ),
        ];

        let engine = QueryEngine::new();
        // @validate.max < 100
        let pipeline = QueryPipeline {
            selector: QuerySelector::Kind(mk_ident("field")),
            filters: vec![QueryFilter::DecoratorArgFilter(
                mk_ident("validate"),
                mk_ident("max"),
                BinOp::Lt,
                Expr::IntLit(100, ds()),
            )],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                if let Value::BlockRef(br) = &items[0] {
                    assert_eq!(br.id.as_deref(), Some("name"));
                } else {
                    panic!("expected BlockRef");
                }
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn decorator_arg_filter_gte() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let blocks = vec![
            mk_block_with_decorators(
                "field",
                Some("a"),
                vec![],
                vec![mk_decorator("validate", vec![("min", Value::Int(10))])],
            ),
            mk_block_with_decorators(
                "field",
                Some("b"),
                vec![],
                vec![mk_decorator("validate", vec![("min", Value::Int(5))])],
            ),
            mk_block_with_decorators(
                "field",
                Some("c"),
                vec![],
                vec![mk_decorator("validate", vec![("min", Value::Int(3))])],
            ),
        ];

        let engine = QueryEngine::new();
        // @validate.min >= 5
        let pipeline = QueryPipeline {
            selector: QuerySelector::Kind(mk_ident("field")),
            filters: vec![QueryFilter::DecoratorArgFilter(
                mk_ident("validate"),
                mk_ident("min"),
                BinOp::Gte,
                Expr::IntLit(5, ds()),
            )],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                let ids: Vec<_> = items
                    .iter()
                    .filter_map(|i| {
                        if let Value::BlockRef(br) = i {
                            br.id.clone()
                        } else {
                            None
                        }
                    })
                    .collect();
                assert!(ids.contains(&"a".to_string()));
                assert!(ids.contains(&"b".to_string()));
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn decorator_arg_filter_lte() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let blocks = vec![
            mk_block_with_decorators(
                "field",
                Some("x"),
                vec![],
                vec![mk_decorator("validate", vec![("max", Value::Int(100))])],
            ),
            mk_block_with_decorators(
                "field",
                Some("y"),
                vec![],
                vec![mk_decorator("validate", vec![("max", Value::Int(50))])],
            ),
            mk_block_with_decorators(
                "field",
                Some("z"),
                vec![],
                vec![mk_decorator("validate", vec![("max", Value::Int(200))])],
            ),
        ];

        let engine = QueryEngine::new();
        // @validate.max <= 100
        let pipeline = QueryPipeline {
            selector: QuerySelector::Kind(mk_ident("field")),
            filters: vec![QueryFilter::DecoratorArgFilter(
                mk_ident("validate"),
                mk_ident("max"),
                BinOp::Lte,
                Expr::IntLit(100, ds()),
            )],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                let ids: Vec<_> = items
                    .iter()
                    .filter_map(|i| {
                        if let Value::BlockRef(br) = i {
                            br.id.clone()
                        } else {
                            None
                        }
                    })
                    .collect();
                assert!(ids.contains(&"x".to_string()));
                assert!(ids.contains(&"y".to_string()));
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn query_table_id_selector() {
        let mut ev = Evaluator::new();
        let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);

        let row1 = mk_block("row", None, vec![("port", Value::Int(80))]);
        let mut table = mk_block("table", Some("ports"), vec![]);
        table.children.push(row1);

        let blocks = vec![table];

        let engine = QueryEngine::new();
        let pipeline = QueryPipeline {
            selector: QuerySelector::TableId(IdentifierLit {
                value: "ports".to_string(),
                span: ds(),
            }),
            filters: vec![],
            span: ds(),
        };

        let result = engine.execute(&pipeline, &blocks, &mut ev, scope).unwrap();
        match result {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                if let Value::BlockRef(br) = &items[0] {
                    assert_eq!(br.attributes.get("port"), Some(&Value::Int(80)));
                } else {
                    panic!("expected BlockRef");
                }
            }
            _ => panic!("expected list"),
        }
    }
}
