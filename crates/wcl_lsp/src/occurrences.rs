//! Source occurrences tied to declarations and lexical bindings.

use std::collections::HashMap;

use tower_lsp::lsp_types::Url;
use wcl_lang::{Document, Lexer, Span, TokenKind, ast::*};

#[derive(Clone, Debug, PartialEq, Eq)]
/// A workspace declaration or a binding whose identity is local to one source.
pub(crate) enum Identity {
    /// A fully qualified indexed name, or a schema-name category and owner.
    Global(String),
    /// The declaring source and name span of a lexical binding.
    Local(Url, Span),
}

/// One authored declaration or reference with its exact edit range.
pub(crate) struct Occurrence {
    /// Declaration shared by all references to this name.
    pub identity: Identity,
    /// The identifier bytes, excluding namespace qualifiers.
    pub span: Span,
    /// Whether this occurrence introduces the name.
    pub declaration: bool,
    /// False when replacing the name would also change a selector.
    pub rename_supported: bool,
}

/// Walk a parsed source using the workspace index and local lexical scopes.
pub(crate) fn collect(source: &str, uri: &Url, doc: &Document) -> Option<Vec<Occurrence>> {
    let ast = wcl_lang::parse_for_edit(source, uri.as_str()).ok()?;
    let namespace = ast
        .items
        .iter()
        .find_map(|item| match item {
            Item::NamespaceDecl(n) => Some(n.path.join(".")),
            _ => None,
        })
        .unwrap_or_default();
    let mut collector = Collector {
        source,
        uri,
        doc,
        namespace,
        aliases: HashMap::new(),
        bindings: Vec::new(),
        item_scopes: Vec::new(),
        out: Vec::new(),
    };
    for item in &ast.items {
        if let Item::UseDecl(u) = item {
            match &u.form {
                UseForm::Bare(alias) => {
                    if let Some(name) = alias.as_ref().or(u.path.last()) {
                        collector.aliases.insert(name.clone(), u.path.join("."));
                    }
                }
                UseForm::List(items) => {
                    for item in items {
                        collector.aliases.insert(
                            item.alias.as_ref().unwrap_or(&item.name).clone(),
                            format!("{}.{}", u.path.join("."), item.name),
                        );
                    }
                }
            }
        }
    }
    collector.items(&ast.items, true);
    Some(collector.out)
}

/// State for a depth-first traversal of one source snapshot.
struct Collector<'a> {
    /// Original text used to locate names without dedicated AST spans.
    source: &'a str,
    /// Source identity for lexical declarations.
    uri: &'a Url,
    /// Workspace declaration index.
    doc: &'a Document,
    /// Namespace declared by this source, rather than by the workspace root.
    namespace: String,
    /// Local import names mapped to their source paths.
    aliases: HashMap<String, String>,
    /// Visible value bindings, with the innermost entries last.
    bindings: Vec<(String, Identity)>,
    /// Item scopes used by explicit self and parent member accesses.
    item_scopes: Vec<HashMap<String, Identity>>,
    /// Collected authored occurrences.
    out: Vec<Occurrence>,
}

impl Collector<'_> {
    /// Record a name with its declaration identity.
    fn push(&mut self, identity: Identity, span: Span, declaration: bool) {
        self.out.push(Occurrence {
            identity,
            span,
            declaration,
            rename_supported: true,
        });
    }

    /// Prefix a declaration with this source's namespace.
    fn qualified(&self, name: &str) -> String {
        if self.namespace.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", self.namespace, name)
        }
    }

    /// Resolve an indexed path through use aliases and the source namespace.
    fn resolve(&self, path: &str) -> Option<String> {
        let (head, tail) = path.split_once('.').unwrap_or((path, ""));
        if let Some(alias) = self.aliases.get(head) {
            let expanded = if tail.is_empty() {
                alias.clone()
            } else {
                format!("{alias}.{tail}")
            };
            if self.doc.find_symbol(&expanded).is_some() {
                return Some(expanded);
            }
        }
        [self.qualified(path), path.to_string()]
            .into_iter()
            .find(|candidate| self.doc.find_symbol(candidate).is_some())
    }

    /// Lex identifiers within an AST span, excluding comments and strings.
    fn tokens(&self, span: Span) -> Vec<(String, Span)> {
        let mut out = Vec::new();
        let Some(text) = self.source.get(span.start..span.end) else {
            return out;
        };
        let mut lexer = Lexer::new(text);
        while let Ok(token) = lexer.next_token() {
            if matches!(token.kind, TokenKind::Eof) {
                break;
            }
            if let TokenKind::Ident(name) = token.kind {
                out.push((
                    name,
                    Span::new(span.start + token.span.start, span.start + token.span.end),
                ));
            }
        }
        out
    }

    /// Locate an authored declaration name within its node.
    fn name_span(&self, span: Span, name: &str) -> Option<Span> {
        self.tokens(span)
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s)
    }

    /// Resolve the named paths in a type annotation.
    fn type_refs(&mut self, span: Span) {
        let tokens = self.tokens(span);
        let mut i = 0;
        while i < tokens.len() {
            let mut path = tokens[i].0.clone();
            let mut end = i;
            while end + 1 < tokens.len()
                && self.source[tokens[end].1.end..tokens[end + 1].1.start].trim() == "."
            {
                end += 1;
                path.push('.');
                path.push_str(&tokens[end].0);
            }
            if let Some(fqn) = self.resolve(&path) {
                // An explicit alias retains its local spelling when its target is renamed.
                if !self.aliases.contains_key(&path)
                    || path == fqn.rsplit('.').next().unwrap_or(&fqn)
                {
                    self.push(Identity::Global(fqn), tokens[end].1, false);
                }
            }
            i = end + 1;
        }
    }

    /// Visit a schema declaration's name, decorators, and header types.
    fn declaration(&mut self, name: &[String], span: Span, decorators: &[Decorator]) {
        self.decorators(decorators, Some(&self.qualified(&name.join("."))));
        let start = decorators
            .iter()
            .map(|d| d.span.end)
            .max()
            .unwrap_or(span.start);
        let tokens = self.tokens(Span::new(start, span.end));
        // The first identifier is the declaration keyword; the following path names it.
        if let Some((_, tail)) = tokens.get(name.len()) {
            self.push(
                Identity::Global(self.qualified(&name.join("."))),
                *tail,
                true,
            );
            let header_end = self.source[tail.end..span.end]
                .find('{')
                .map_or(span.end, |n| tail.end + n);
            self.type_refs(Span::new(tail.end, header_end));
        }
    }

    /// Visit decorator names, literal registrations, and argument expressions.
    fn decorators(&mut self, decorators: &[Decorator], owner: Option<&str>) {
        for d in decorators {
            if let Some((name, qualifier)) = d.name.split_last() {
                let namespace: Vec<_> = self
                    .namespace
                    .split('.')
                    .filter(|n| !n.is_empty())
                    .map(str::to_string)
                    .collect();
                if let Some(schema) = self.doc.decorator_schema_in(qualifier, name, &namespace) {
                    use wcl_lang::DeclName;
                    if let Some(span) = self.name_span(d.name_span, name) {
                        self.push(
                            Identity::Global(format!("decorator:{}", schema.full_name())),
                            span,
                            false,
                        );
                    }
                }
                if let Some(owner) = owner
                    && qualifier.is_empty()
                    && matches!(name.as_str(), "block" | "decorator")
                    && let Some(Expr::Utf8(value)) = d.positional.first()
                    && let Some(span) = d.positional_spans.first()
                    && self.source.get(span.start..span.end)
                        == Some(format!("\"{value}\"").as_str())
                {
                    self.push(
                        Identity::Global(format!("{name}:{owner}")),
                        Span::new(span.start + 1, span.end - 1),
                        true,
                    );
                }
            }
            for e in &d.positional {
                self.expr(e);
            }
            for arg in &d.named {
                self.expr(&arg.value);
            }
        }
    }

    /// Visit annotations and defaults without treating field labels as type names.
    fn fields(&mut self, fields: &[TypeField]) {
        for f in fields {
            self.decorators(&f.decorators, None);
            self.type_refs(f.ty_span);
            if let Some(e) = &f.default_expr {
                self.expr(e);
            }
        }
    }

    /// Introduce a value binding into the current lexical scope.
    fn bind(&mut self, name: &str, span: Span, global: bool) {
        let identity = if global {
            Identity::Global(self.qualified(name))
        } else {
            Identity::Local(self.uri.clone(), span)
        };
        self.push(identity.clone(), span, true);
        self.bindings.push((name.to_string(), identity));
    }

    /// Walk an item scope with all sibling bindings visible.
    fn items(&mut self, items: &[Item], global: bool) {
        let depth = self.bindings.len();
        // Item bindings are visible to sibling and descendant expressions.
        for item in items {
            let binding = match item {
                Item::Field(f) => Some((&f.name, f.span, &f.decorators)),
                Item::Let(l) => Some((&l.name, l.span, &l.decorators)),
                _ => None,
            };
            if let Some((name, span, decorators)) = binding {
                let start = decorators
                    .iter()
                    .map(|d| d.span.end)
                    .max()
                    .unwrap_or(span.start);
                if let Some(s) = self.name_span(Span::new(start, span.end), name) {
                    self.bind(name, s, global);
                }
            }
        }
        self.item_scopes
            .push(self.bindings[depth..].iter().cloned().collect());
        for item in items {
            match item {
                Item::TypeDecl(d) => {
                    self.declaration(&d.name, d.span, &d.decorators);
                    self.fields(&d.fields);
                }
                Item::InterfaceDecl(d) => {
                    self.declaration(&d.name, d.span, &d.decorators);
                    self.fields(&d.fields);
                }
                Item::UnionDecl(d) => {
                    self.declaration(&d.name, d.span, &d.decorators);
                    for v in &d.variants {
                        self.decorators(&v.decorators, None);
                        if let Some(span) = self.name_span(v.span, &v.name) {
                            self.push(
                                Identity::Global(format!(
                                    "{}.{}",
                                    self.qualified(&d.name.join(".")),
                                    v.name
                                )),
                                span,
                                true,
                            );
                        }
                        match &v.body {
                            VariantBody::Record { fields, .. } => self.fields(fields),
                            VariantBody::TypeRef { ty_span, .. } => self.type_refs(*ty_span),
                            VariantBody::InterfaceRef { iface_span, .. } => {
                                self.type_refs(*iface_span)
                            }
                            VariantBody::Unit => {}
                        }
                    }
                }
                Item::SymbolSetDecl(d) => {
                    self.declaration(&d.name, d.span, &d.decorators);
                }
                Item::ConnectionDecl(d) => {
                    self.declaration(&d.name, d.span, &d.decorators);
                    self.type_refs(d.source_span);
                    self.type_refs(d.destination_span);
                    self.type_refs(d.kind_set_span);
                }
                Item::Field(f) => {
                    self.decorators(&f.decorators, None);
                    self.expr(&f.expr);
                }
                Item::Let(l) => {
                    self.decorators(&l.decorators, None);
                    self.expr(&l.value);
                }
                Item::Block(b) => {
                    self.decorators(&b.decorators, None);
                    if let Some(schema) = self.doc.block_schema(&b.kind) {
                        use wcl_lang::DeclName;
                        let start = b
                            .decorators
                            .iter()
                            .map(|d| d.span.end)
                            .max()
                            .unwrap_or(b.span.start);
                        if let Some(span) = self.name_span(Span::new(start, b.span.end), &b.kind) {
                            self.push(
                                Identity::Global(format!("block:{}", schema.full_name())),
                                span,
                                false,
                            );
                        }
                    }
                    if let Some(slot) = &b.slot_decl {
                        self.type_refs(slot.ty_span);
                    }
                    for label in &b.labels {
                        self.expr(label);
                    }
                    self.items(&b.items, false);
                }
                Item::Table(t) => {
                    for row in &t.rows {
                        for value in &row.values {
                            self.expr(value);
                        }
                    }
                }
                Item::UseDecl(u) => match &u.form {
                    UseForm::Bare(_) => {
                        let tokens = self.tokens(u.span);
                        if let Some((_, span)) = tokens.get(u.path.len())
                            && let Some(fqn) = self.resolve(&u.path.join("."))
                        {
                            self.push(Identity::Global(fqn), *span, false);
                        }
                    }
                    UseForm::List(items) => {
                        for item in items {
                            let fqn = format!("{}.{}", u.path.join("."), item.name);
                            if self.doc.find_symbol(&fqn).is_some()
                                && let Some(span) = self.name_span(item.span, &item.name)
                            {
                                self.push(Identity::Global(fqn), span, false);
                            }
                        }
                    }
                },
                Item::NamespaceDecl(_) | Item::Import(_) | Item::Connection(_) => {}
            }
        }
        self.item_scopes.pop();
        self.bindings.truncate(depth);
    }

    /// Resolve a value name, preferring the innermost lexical binding.
    fn identifier(&mut self, name: &str, span: Span) {
        if let Some((_, id)) = self.bindings.iter().rev().find(|(n, _)| n == name) {
            self.push(id.clone(), span, false);
        } else if let Some(fqn) = self.resolve(name)
            && (!self.aliases.contains_key(name) || fqn.rsplit('.').next() == Some(name))
        {
            self.push(Identity::Global(fqn), span, false);
        }
    }

    /// Record the explicit type and variant parts of a constructor or pattern.
    fn variant(&mut self, path: &[String], variant: &str, span: Span) {
        if let Some(fqn) = self.resolve(&path.join(".")) {
            let tokens = self.tokens(span);
            if let Some((_, s)) = tokens.get(path.len().saturating_sub(1))
                && (!self.aliases.contains_key(&path.join("."))
                    || path.last().map(String::as_str) == fqn.rsplit('.').next())
            {
                self.push(Identity::Global(fqn.clone()), *s, false);
            }
            if let Some((_, s)) = tokens.get(path.len()) {
                self.push(Identity::Global(format!("{fqn}.{variant}")), *s, false);
            }
        }
    }

    /// Introduce pattern bindings and visit explicit variant types.
    fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Binding { name, span } | Pattern::At { name, span, .. } => {
                if let Some(s) = self.name_span(*span, name) {
                    self.bind(name, s, false);
                }
                if let Pattern::At { inner, .. } = pattern {
                    self.pattern(inner);
                }
            }
            Pattern::Variant {
                type_path,
                variant,
                args,
                span,
            } => {
                self.variant(type_path, variant, *span);
                match args {
                    VariantPatArgs::Positional(p) => self.pattern(p),
                    VariantPatArgs::Record { fields, .. } => {
                        for (field, p) in fields {
                            let start = self.out.len();
                            self.pattern(p);
                            if let Pattern::Binding { name, span } = p
                                && field == name
                                && self.source[..span.start].trim_end().ends_with(['{', ','])
                            {
                                // Renaming shorthand also changes its field selector.
                                for occurrence in &mut self.out[start..] {
                                    occurrence.rename_supported = false;
                                }
                            }
                        }
                    }
                    VariantPatArgs::Unit => {}
                }
            }
            Pattern::Wildcard(_)
            | Pattern::LiteralBool(..)
            | Pattern::LiteralNumber { .. }
            | Pattern::LiteralUtf8(..)
            | Pattern::LiteralAscii(..)
            | Pattern::LiteralSymbol(..)
            | Pattern::LiteralNone(_) => {}
        }
    }

    /// Visit every expression child while retaining lexical shadowing.
    fn expr(&mut self, expr: &Expr) {
        let depth = self.bindings.len();
        match expr {
            Expr::Identifier(name, span) => self.identifier(name, *span),
            Expr::Block { lets, tail, .. } => {
                for l in lets {
                    self.expr(&l.value);
                    if let Some(span) = self.name_span(l.span, &l.name) {
                        self.bind(&l.name, span, false);
                    }
                }
                self.expr(tail);
            }
            Expr::Function(f) => {
                for p in &f.params {
                    self.type_refs(p.ty_span);
                    if let Some(span) = self.name_span(p.span, &p.name) {
                        self.bind(&p.name, span, false);
                    }
                }
                self.type_refs(f.return_ty_span);
                self.expr(&f.body);
            }
            Expr::Member { recv, name, span } => {
                let scope = match recv.as_ref() {
                    Expr::SelfKw(_) => self.item_scopes.last(),
                    Expr::ParentKw(_) => self
                        .item_scopes
                        .len()
                        .checked_sub(2)
                        .and_then(|i| self.item_scopes.get(i)),
                    _ => None,
                };
                if let Some(identity) = scope.and_then(|s| s.get(name)).cloned() {
                    if let Some((_, member_span)) = self.tokens(*span).last() {
                        self.push(identity, *member_span, false);
                    }
                    return;
                }
                // Only namespace-qualified paths resolve globally. Object members
                // are not references to an unrelated lexical binding of that name.
                /// Recover a dotted identifier path without evaluating a receiver.
                fn path(e: &Expr) -> Option<String> {
                    match e {
                        Expr::Identifier(n, _) => Some(n.clone()),
                        Expr::Member { recv, name, .. } => Some(format!("{}.{name}", path(recv)?)),
                        _ => None,
                    }
                }
                let qualified = path(expr).filter(|p| {
                    !self
                        .bindings
                        .iter()
                        .any(|(n, _)| Some(n.as_str()) == p.split('.').next())
                });
                if let Some(fqn) = qualified.as_deref().and_then(|p| self.resolve(p)) {
                    if let Some((_, s)) = self.tokens(*span).last() {
                        self.push(Identity::Global(fqn), *s, false);
                    }
                } else {
                    let _ = name;
                    self.expr(recv);
                }
            }
            Expr::Call { callee, args, .. } => {
                self.expr(callee);
                for a in args {
                    self.expr(a);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            Expr::Unary { operand, .. } => self.expr(operand),
            Expr::Paren { inner, .. } => self.expr(inner),
            Expr::ListLit { elements, .. } => {
                for e in elements {
                    self.expr(e);
                }
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.expr(cond);
                self.expr(then_block);
                if let Some(e) = else_block {
                    self.expr(e);
                }
            }
            Expr::IfLet {
                pattern,
                scrut,
                then_block,
                else_block,
                ..
            } => {
                self.expr(scrut);
                self.pattern(pattern);
                self.expr(then_block);
                self.bindings.truncate(depth);
                self.expr(else_block);
            }
            Expr::Match { scrut, arms, .. } => {
                self.expr(scrut);
                for arm in arms {
                    for p in &arm.patterns {
                        self.pattern(p);
                    }
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                    }
                    self.expr(&arm.body);
                    self.bindings.truncate(depth);
                }
            }
            Expr::Try {
                body,
                binder,
                binder_span,
                handler,
                ..
            } => {
                self.expr(body);
                self.bind(binder, *binder_span, false);
                self.expr(handler);
            }
            Expr::Variant {
                type_path,
                variant,
                args,
                span,
            } => {
                self.variant(type_path, variant, *span);
                match args {
                    VariantArgs::Positional(e) => self.expr(e),
                    VariantArgs::Record { fields, .. } => {
                        for f in fields {
                            self.expr(&f.value);
                        }
                    }
                    VariantArgs::Unit => {}
                }
            }
            Expr::Record { fields, .. } => {
                for f in fields {
                    self.expr(&f.value);
                }
            }
            Expr::InterpolatedString { parts, .. } => {
                for p in parts {
                    if let TemplatePart::Expr(e) = p {
                        self.expr(e);
                    }
                }
            }
            Expr::Bool(_)
            | Expr::I8(_)
            | Expr::I16(_)
            | Expr::I32(_)
            | Expr::I64(_)
            | Expr::I128(_)
            | Expr::Isize(_)
            | Expr::U8(_)
            | Expr::U16(_)
            | Expr::U32(_)
            | Expr::U64(_)
            | Expr::U128(_)
            | Expr::Usize(_)
            | Expr::F32(_)
            | Expr::F64(_)
            | Expr::UnitLiteral { .. }
            | Expr::Utf8(_)
            | Expr::Ascii(_)
            | Expr::Utf16(_)
            | Expr::Utf32(_)
            | Expr::Symbol(_)
            | Expr::None
            | Expr::SelfKw(_)
            | Expr::ParentKw(_) => {}
        }
        self.bindings.truncate(depth);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Return the spellings and declaration flags for the selected binding.
    fn matching(source: &str, needle: &str) -> Vec<(String, bool)> {
        let doc = Document::open(source, "test.wcl").expect("document");
        let uri = Url::parse("file:///test.wcl").unwrap();
        let occurrences = collect(source, &uri, &doc).expect("occurrences");
        let offset = source.find(needle).expect("cursor");
        let selected = occurrences
            .iter()
            .find(|o| o.span.start == offset)
            .expect("selected symbol");
        occurrences
            .iter()
            .filter(|o| o.identity == selected.identity)
            .map(|o| (source[o.span.start..o.span.end].to_string(), o.declaration))
            .collect()
    }

    #[test]
    fn type_references_exclude_comments_strings_and_field_names() {
        let source = "type Foo { x: utf8 }\n// Foo is unrelated\ntype Other { Foo: utf8 value: Foo }\n@schemaless text = \"Foo\"\n";
        assert_eq!(
            matching(source, "Foo {"),
            vec![("Foo".into(), true), ("Foo".into(), false)]
        );
    }

    #[test]
    fn local_bindings_distinguish_shadowed_parameters_and_members() {
        let source =
            "@schemaless value = fn(x: i64) -> i64 { let y = fn(x: i64) -> i64 { x }; x + y(2) }\n";
        assert_eq!(
            matching(source, "x: i64"),
            vec![("x".into(), true), ("x".into(), false)]
        );
        let source =
            "@schemaless x = 1\n@schemaless object = { x: 2 }\n@schemaless value = x + object.x\n";
        assert_eq!(
            matching(source, "x = 1"),
            vec![("x".into(), true), ("x".into(), false)]
        );
    }

    #[test]
    fn nested_let_shadows_parameter_after_initializer() {
        let source = "@schemaless value = fn(x: i64) -> i64 { let x = x + 1; x }\n";
        assert_eq!(
            matching(source, "x: i64"),
            vec![("x".into(), true), ("x".into(), false)]
        );
        assert_eq!(
            matching(source, "x = x"),
            vec![("x".into(), true), ("x".into(), false)]
        );
    }

    #[test]
    fn interpolation_visits_expressions_only() {
        let source = "@schemaless name = \"a\"\n@schemaless text = $\"name: ${name}\"\n";
        assert_eq!(
            matching(source, "name ="),
            vec![("name".into(), true), ("name".into(), false)]
        );
    }

    #[test]
    fn self_member_resolves_the_item_field_instead_of_a_shadowing_parameter() {
        let source = "@schemaless x = 2\n@schemaless value = fn(x: i64) -> i64 { self.x + x }\n";
        assert_eq!(
            matching(source, "x = 2"),
            vec![("x".into(), true), ("x".into(), false)]
        );
        assert_eq!(
            matching(source, "x: i64"),
            vec![("x".into(), true), ("x".into(), false)]
        );
    }
}
