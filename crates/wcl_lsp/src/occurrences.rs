//! Source occurrences tied to declarations and lexical bindings.

use std::collections::HashMap;

use tower_lsp::lsp_types::Url;
use wcl_lang::{DeclName, Document, Lexer, ResolvedType, Span, TokenKind, ast::*};

#[derive(Clone, Debug, PartialEq, Eq)]
/// A workspace declaration or a binding whose identity is local to one source.
pub(crate) enum Identity {
    /// A fully qualified indexed name, or a schema-name category and owner.
    Global(String),
    /// The declaring source and name span of a lexical binding.
    Local(Url, Span),
    /// A semantic name is computed, so its declaration cannot be edited safely.
    Unresolved(String),
}

/// One authored declaration or reference with its exact edit range.
pub(crate) struct Occurrence {
    /// Declaration shared by all references to this name.
    pub identity: Identity,
    /// The identifier bytes, excluding namespace qualifiers.
    pub span: Span,
    /// Whether this occurrence introduces the name.
    pub declaration: bool,
    /// Selector preserved when a shorthand binding becomes explicit.
    pub replacement_prefix: String,
    pub replacement_suffix: String,
}

#[derive(Clone, Debug, PartialEq)]
enum ContextType {
    Named(String),
    List(Box<ContextType>),
    Function(Vec<ContextType>, Box<ContextType>),
    Record(Vec<(String, ContextType)>),
    Unknown,
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
        binding_types: Vec::new(),
        initializers: Vec::new(),
        visiting: Vec::new(),
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
    collector.items(&ast.items, true, None);
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
    binding_types: Vec<(Identity, ContextType)>,
    initializers: Vec<(Identity, Expr)>,
    visiting: Vec<Identity>,
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
            replacement_prefix: String::new(),
            replacement_suffix: String::new(),
        });
    }

    fn namespace_parts(&self) -> Vec<String> {
        self.namespace
            .split('.')
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn resolved_type(&self, ty: ResolvedType<'_>, depth: usize) -> ContextType {
        if depth == 0 {
            return ContextType::Unknown;
        }
        match ty {
            ResolvedType::Named(d) => match d.alias_type() {
                Some(t) => self.resolved_type(self.doc.resolve_in(t, d.file_ns()), depth - 1),
                None => ContextType::Named(d.full_name()),
            },
            ResolvedType::Union(d) => ContextType::Named(d.full_name()),
            ResolvedType::SymbolSet(d) => ContextType::Named(d.full_name()),
            ResolvedType::Interface(d) => ContextType::Named(d.full_name()),
            ResolvedType::Reference(t) => self.resolved_type(*t, depth - 1),
            ResolvedType::List(t) => ContextType::List(Box::new(self.resolved_type(*t, depth - 1))),
            ResolvedType::Tensor { element, .. } => {
                ContextType::List(Box::new(self.resolved_type(*element, depth - 1)))
            }
            ResolvedType::Function { params, return_ty } => ContextType::Function(
                params
                    .into_iter()
                    .map(|t| self.resolved_type(t, depth - 1))
                    .collect(),
                Box::new(self.resolved_type(*return_ty, depth - 1)),
            ),
            _ => ContextType::Unknown,
        }
    }

    fn type_context(&self, ty: &TypeRef) -> ContextType {
        self.resolved_type(self.doc.resolve_in(ty, &self.namespace_parts()), 32)
    }

    fn field_context(&self, owner: &ContextType, name: &str) -> ContextType {
        match owner {
            ContextType::Named(owner) => self
                .doc
                .type_decl(owner)
                .and_then(|d| d.effective_fields().into_iter().find(|f| f.name() == name))
                .map(|f| self.resolved_type(f.resolved_type(), 32))
                .unwrap_or(ContextType::Unknown),
            ContextType::Record(fields) => fields
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, t)| t.clone())
                .unwrap_or(ContextType::Unknown),
            _ => ContextType::Unknown,
        }
    }

    fn root_field_context(&self, name: &str) -> ContextType {
        self.doc
            .type_decls()
            .filter(|d| d.decorators().any(|a| a.full_name() == "document"))
            .filter(|d| d.file_ns() == self.namespace_parts() || d.is_imported())
            .filter_map(|d| d.effective_fields().into_iter().find(|f| f.name() == name))
            .next()
            .map(|f| self.resolved_type(f.resolved_type(), 32))
            .unwrap_or(ContextType::Unknown)
    }

    fn binding_type(&self, id: &Identity) -> ContextType {
        self.binding_types
            .iter()
            .rev()
            .find(|(i, _)| i == id)
            .map(|(_, t)| t.clone())
            .unwrap_or(ContextType::Unknown)
    }

    fn infer(&self, expr: &Expr) -> ContextType {
        match expr {
            Expr::Identifier(n, _) => self
                .bindings
                .iter()
                .rev()
                .find(|(name, _)| name == n)
                .map(|(_, id)| self.binding_type(id))
                .unwrap_or_else(|| self.root_field_context(n)),
            Expr::Variant { type_path, .. } => self
                .resolve(&type_path.join("."))
                .map(ContextType::Named)
                .unwrap_or(ContextType::Unknown),
            Expr::Function(f) => ContextType::Function(
                f.params.iter().map(|p| self.type_context(&p.ty)).collect(),
                Box::new(self.type_context(&f.return_ty)),
            ),
            Expr::Call { callee, .. } => match self.infer(callee) {
                ContextType::Function(_, t) => *t,
                _ => ContextType::Unknown,
            },
            Expr::Member { recv, name, .. } => self.field_context(&self.infer(recv), name),
            Expr::Paren { inner, .. } => self.infer(inner),
            Expr::Record { fields, .. } => ContextType::Record(
                fields
                    .iter()
                    .map(|f| (f.name.clone(), self.infer(&f.value)))
                    .collect(),
            ),
            Expr::ListLit { elements, .. } => ContextType::List(Box::new(
                elements
                    .first()
                    .map(|e| self.infer(e))
                    .unwrap_or(ContextType::Unknown),
            )),
            Expr::If { then_block, .. } => self.infer(then_block),
            Expr::Block { tail, .. } => self.infer(tail),
            _ => ContextType::Unknown,
        }
    }

    fn variant_owner(&self, owner: &str, name: &str, depth: usize) -> Option<String> {
        if depth == 0 {
            return None;
        }
        let union = self.doc.union_decl(owner)?;
        if union.variant(name).is_some() {
            return Some(owner.to_string());
        }
        union.extends().iter().find_map(|path| {
            let ty = TypeRef::named(path.clone());
            let ContextType::Named(parent) =
                self.resolved_type(self.doc.resolve_in(&ty, union.file_ns()), 32)
            else {
                return None;
            };
            self.variant_owner(&parent, name, depth - 1)
        })
    }

    fn symbol(&mut self, name: &str, span: Span, expected: &ContextType) {
        if let ContextType::Named(owner) = expected
            && self
                .doc
                .symbol_set(owner)
                .is_some_and(|s| s.symbols().any(|s| s.name() == name))
        {
            self.push(
                Identity::Global(format!("{owner}.{name}")),
                Span::new(span.start + 1, span.end),
                false,
            );
        }
    }

    fn kind_strings(&mut self, expr: &Expr, span: Span, category: &str, owner: Option<&str>) {
        if let Expr::ListLit { elements, span, .. } = expr {
            let spans = self.argument_spans(*span);
            for (element, span) in elements.iter().zip(spans) {
                self.kind_strings(element, span, category, owner);
            }
            return;
        }
        if !matches!(expr, Expr::Utf8(_) | Expr::Ascii(_)) {
            self.push(Identity::Unresolved(category.into()), span, false);
            return;
        }
        let Some(text) = self.source.get(span.start..span.end) else {
            return;
        };
        let mut lexer = Lexer::new(text);
        while let Ok(token) = lexer.next_token() {
            if matches!(token.kind, TokenKind::Eof) {
                break;
            }
            let TokenKind::Str(
                wcl_lang::StringLit::Utf8(value) | wcl_lang::StringLit::Ascii(value),
            ) = token.kind
            else {
                continue;
            };
            let parts: Vec<String> = value
                .split(['.', ':'])
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            let Some((name, qualifier)) = parts.split_last() else {
                continue;
            };
            let target = owner.map(str::to_string).or_else(|| {
                let schema = if category == "block" {
                    self.doc
                        .block_schema_in(qualifier, name, &self.namespace_parts())
                } else {
                    self.doc
                        .decorator_schema_in(qualifier, name, &self.namespace_parts())
                };
                schema.map(|d| d.full_name())
            });
            if let Some(target) = target {
                let start = span.start + token.span.start;
                let end = span.start + token.span.end;
                let prefix = &value[..value.len() - name.len()];
                let authored = &self.source[start..end];
                if authored.ends_with(&format!("{value}\"")) {
                    self.push(
                        Identity::Global(format!("{category}:{target}")),
                        Span::new(end - 1 - name.len(), end - 1),
                        owner.is_some(),
                    );
                } else {
                    self.push(
                        Identity::Global(format!("{category}:{target}")),
                        Span::new(start, end),
                        owner.is_some(),
                    );
                    let occurrence = self.out.last_mut().unwrap();
                    occurrence.replacement_prefix = format!("\"{prefix}");
                    occurrence.replacement_suffix = "\"".into();
                }
            }
        }
    }

    fn shorthand(&self, span: Span) -> bool {
        let mut lexer = Lexer::new(&self.source[..span.start]);
        let mut previous = TokenKind::Eof;
        while let Ok(token) = lexer.next_token() {
            if matches!(token.kind, TokenKind::Eof) {
                break;
            }
            previous = token.kind;
        }
        matches!(previous, TokenKind::LBrace | TokenKind::Comma)
    }

    fn argument_spans(&self, span: Span) -> Vec<Span> {
        let mut spans = Vec::new();
        let mut lexer = Lexer::new(&self.source[span.start..span.end]);
        let mut depth = 0usize;
        let mut start = span.start;
        while let Ok(token) = lexer.next_token() {
            match token.kind {
                TokenKind::Eof => break,
                TokenKind::LParen | TokenKind::LBrace | TokenKind::LBracket => {
                    depth += 1;
                    if depth == 1 {
                        start = span.start + token.span.end;
                    }
                }
                TokenKind::RParen | TokenKind::RBrace | TokenKind::RBracket => {
                    if depth == 1 {
                        spans.push(Span::new(start, span.start + token.span.start));
                        break;
                    }
                    depth = depth.saturating_sub(1);
                }
                TokenKind::Comma if depth == 1 => {
                    spans.push(Span::new(start, span.start + token.span.start));
                    start = span.start + token.span.end;
                }
                _ => {}
            }
        }
        spans
    }

    fn reflective_kind_string(&mut self, expr: &Expr, span: Span, category: &str) {
        let wanted = usize::from(category == "decorator");
        let mut lexer = Lexer::new(&self.source[span.start..span.end]);
        let mut depth = 0usize;
        let mut index = 0usize;
        let mut start = None;
        while let Ok(token) = lexer.next_token() {
            if matches!(token.kind, TokenKind::Eof) {
                break;
            }
            match token.kind {
                TokenKind::LParen | TokenKind::LBrace | TokenKind::LBracket => {
                    depth += 1;
                    if depth == 1 {
                        start = Some(span.start + token.span.end);
                    }
                }
                TokenKind::RParen | TokenKind::RBrace | TokenKind::RBracket => {
                    if depth == 1 && index == wanted {
                        self.kind_strings(
                            expr,
                            Span::new(start.unwrap(), span.start + token.span.start),
                            category,
                            None,
                        );
                        break;
                    }
                    depth = depth.saturating_sub(1);
                }
                TokenKind::Comma if depth == 1 => {
                    if index == wanted {
                        self.kind_strings(
                            expr,
                            Span::new(start.unwrap(), span.start + token.span.start),
                            category,
                            None,
                        );
                        break;
                    }
                    index += 1;
                    start = Some(span.start + token.span.end);
                }
                _ => {}
            }
        }
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
            let mut schema = None;
            if let Some((name, qualifier)) = d.name.split_last() {
                let namespace: Vec<_> = self
                    .namespace
                    .split('.')
                    .filter(|n| !n.is_empty())
                    .map(str::to_string)
                    .collect();
                schema = self.doc.decorator_schema_in(qualifier, name, &namespace);
                if let Some(schema) = schema
                    && let Some((_, span)) = self.tokens(d.name_span).last()
                {
                    self.push(
                        Identity::Global(format!("decorator:{}", schema.full_name())),
                        *span,
                        false,
                    );
                }
            }
            let builtin = schema.map(|s| s.full_name()).unwrap_or_default();
            if let Some((expr, span)) = d.positional.first().zip(d.positional_spans.first()) {
                match builtin.as_str() {
                    "Block" | "Table" if owner.is_some() => {
                        self.kind_strings(expr, *span, "block", owner)
                    }
                    "Decorator" if owner.is_some() => {
                        self.kind_strings(expr, *span, "decorator", owner)
                    }
                    "Child" | "Children" if matches!(expr, Expr::Utf8(_) | Expr::Ascii(_)) => {
                        self.kind_strings(expr, *span, "block", None)
                    }
                    "RefDecorator" => self.kind_strings(expr, *span, "block", None),
                    _ => {}
                }
            }
            for (i, e) in d.positional.iter().enumerate() {
                let expected = schema
                    .and_then(|s| {
                        s.effective_fields()
                            .into_iter()
                            .find(|f| f.inline_slot() == Some(i as u64))
                    })
                    .map(|f| self.resolved_type(f.resolved_type(), 32))
                    .unwrap_or(ContextType::Unknown);
                self.expr_with(e, &expected);
            }
            for arg in &d.named {
                if (builtin == "Block" && arg.name == "required_children")
                    || (builtin == "AppliesTo" && arg.name == "kinds")
                {
                    self.kind_strings(&arg.value, arg.span, "block", None);
                }
                let expected = schema
                    .and_then(|s| s.field(&arg.name))
                    .map(|f| self.resolved_type(f.resolved_type(), 32))
                    .unwrap_or(ContextType::Unknown);
                self.expr_with(&arg.value, &expected);
            }
        }
    }

    /// Visit annotations and defaults without treating field labels as type names.
    fn fields(&mut self, fields: &[TypeField]) {
        for f in fields {
            self.decorators(&f.decorators, None);
            for d in &f.decorators {
                if d.name == ["default"]
                    && self
                        .doc
                        .decorator_schema_in(&[], "default", &self.namespace_parts())
                        .is_some_and(|d| d.full_name() == "Default")
                    && let Some(e) = d.positional.first()
                {
                    self.expr_with(e, &self.type_context(&f.ty));
                }
            }
            self.type_refs(f.ty_span);
            if let Some(e) = &f.default_expr {
                self.expr_with(e, &self.type_context(&f.ty));
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
    fn items(&mut self, items: &[Item], global: bool, owner: Option<&ContextType>) {
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
                    let id = self.bindings.last().unwrap().1.clone();
                    let expr = match item {
                        Item::Field(f) => &f.expr,
                        Item::Let(l) => &l.value,
                        _ => unreachable!(),
                    };
                    let ty = if matches!(item, Item::Field(_)) {
                        owner
                            .map(|o| self.field_context(o, name))
                            .unwrap_or_else(|| self.root_field_context(name))
                    } else {
                        self.infer(expr)
                    };
                    let ty = if ty == ContextType::Unknown {
                        self.infer(expr)
                    } else {
                        ty
                    };
                    self.binding_types.push((id.clone(), ty));
                    self.initializers.push((id, expr.clone()));
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
                    for entry in &d.symbols {
                        self.decorators(&entry.decorators, None);
                        let start = entry
                            .decorators
                            .iter()
                            .map(|d| d.span.end)
                            .max()
                            .unwrap_or(entry.span.start);
                        if let Some(span) =
                            self.name_span(Span::new(start, entry.span.end), &entry.name)
                        {
                            self.push(
                                Identity::Global(format!(
                                    "{}.{}",
                                    self.qualified(&d.name.join(".")),
                                    entry.name
                                )),
                                span,
                                true,
                            );
                        }
                    }
                }
                Item::ConnectionDecl(d) => {
                    self.declaration(&d.name, d.span, &d.decorators);
                    self.type_refs(d.source_span);
                    self.type_refs(d.destination_span);
                    self.type_refs(d.kind_set_span);
                }
                Item::Field(f) => {
                    self.decorators(&f.decorators, None);
                    let expected = self
                        .bindings
                        .iter()
                        .rev()
                        .find(|(n, _)| n == &f.name)
                        .map(|(_, id)| self.binding_type(id))
                        .unwrap_or(ContextType::Unknown);
                    self.expr_with(&f.expr, &expected);
                }
                Item::Let(l) => {
                    self.decorators(&l.decorators, None);
                    self.expr(&l.value);
                }
                Item::Block(b) => {
                    self.decorators(&b.decorators, None);
                    let schema =
                        self.doc
                            .block_schema_in(&b.kind_ns, &b.kind, &self.namespace_parts());
                    if let Some(schema) = schema {
                        use wcl_lang::DeclName;
                        let start = b
                            .decorators
                            .iter()
                            .map(|d| d.span.end)
                            .max()
                            .unwrap_or(b.span.start);
                        if let Some((_, span)) = self
                            .tokens(Span::new(start, b.span.end))
                            .get(b.kind_ns.len())
                        {
                            self.push(
                                Identity::Global(format!("block:{}", schema.full_name())),
                                *span,
                                false,
                            );
                        }
                    }
                    if let Some(slot) = &b.slot_decl {
                        self.type_refs(slot.ty_span);
                    }
                    for (i, label) in b.labels.iter().enumerate() {
                        let expected = schema
                            .and_then(|s| {
                                s.effective_fields()
                                    .into_iter()
                                    .find(|f| f.inline_slot() == Some(i as u64))
                            })
                            .map(|f| self.resolved_type(f.resolved_type(), 32))
                            .unwrap_or(ContextType::Unknown);
                        self.expr_with(label, &expected);
                    }
                    self.items(
                        &b.items,
                        false,
                        schema.map(|s| ContextType::Named(s.full_name())).as_ref(),
                    );
                }
                Item::Table(t) => {
                    let expected = owner
                        .map(|o| self.field_context(o, &t.field_name))
                        .unwrap_or_else(|| self.root_field_context(&t.field_name));
                    let columns = match expected {
                        ContextType::List(inner) => match *inner {
                            ContextType::Named(n) => self
                                .doc
                                .type_decl(&n)
                                .map(|s| s.effective_fields())
                                .unwrap_or_default(),
                            _ => Vec::new(),
                        },
                        _ => Vec::new(),
                    };
                    for row in &t.rows {
                        for (i, value) in row.values.iter().enumerate() {
                            let expected = columns
                                .get(i)
                                .map(|f| self.resolved_type(f.resolved_type(), 32))
                                .unwrap_or(ContextType::Unknown);
                            self.expr_with(value, &expected);
                        }
                    }
                }
                Item::Connection(c) => {
                    if let Some((name, span)) = c.kind.as_ref().zip(c.kind_span) {
                        let schemas: Vec<_> = match owner {
                            Some(ContextType::Named(n)) => {
                                self.doc.type_decl(n).into_iter().collect()
                            }
                            None => self
                                .doc
                                .type_decls()
                                .filter(|d| d.decorators().any(|a| a.full_name() == "document"))
                                .filter(|d| {
                                    d.file_ns() == self.namespace_parts() || d.is_imported()
                                })
                                .collect(),
                            _ => Vec::new(),
                        };
                        let endpoint_type = |id: &str| {
                            items.iter().find_map(|item| match item {
                            Item::Block(b) if matches!(b.labels.first(), Some(Expr::Identifier(n, _) | Expr::Utf8(n)) if n == id) => {
                                self.doc.block_schema_in(&b.kind_ns, &b.kind, &self.namespace_parts()).map(|s| ContextType::Named(s.full_name()))
                            }
                            _ => None,
                        })
                        };
                        let source = endpoint_type(&c.lhs);
                        let destination = endpoint_type(&c.rhs);
                        for schema in schemas {
                            for field in schema.effective_fields() {
                                if let Some(connection) = field.connection_schema() {
                                    let from = self.resolved_type(
                                        self.doc.resolve_in(
                                            connection.source_type(),
                                            connection.file_ns(),
                                        ),
                                        32,
                                    );
                                    let to = self.resolved_type(
                                        self.doc.resolve_in(
                                            connection.destination_type(),
                                            connection.file_ns(),
                                        ),
                                        32,
                                    );
                                    if source.as_ref().is_some_and(|s| s != &from)
                                        || destination.as_ref().is_some_and(|s| s != &to)
                                    {
                                        continue;
                                    }
                                    let ty = TypeRef::named(connection.kind_set_path().to_vec());
                                    let expected = self.resolved_type(
                                        self.doc.resolve_in(&ty, connection.file_ns()),
                                        32,
                                    );
                                    self.symbol(name, span, &expected);
                                }
                            }
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
                Item::NamespaceDecl(_) | Item::Import(_) => {}
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
    fn variant(
        &mut self,
        path: &[String],
        variant: &str,
        span: Span,
        expected: &ContextType,
    ) -> Option<String> {
        let target = if path.is_empty() {
            match expected {
                ContextType::Named(n) => Some(n.clone()),
                _ => None,
            }
        } else {
            self.resolve(&path.join("."))
        };
        if let Some(fqn) = target {
            let tokens = self.tokens(span);
            if !path.is_empty()
                && let Some((_, s)) = tokens.get(path.len() - 1)
                && (!self.aliases.contains_key(&path.join("."))
                    || path.last().map(String::as_str) == fqn.rsplit('.').next())
            {
                self.push(Identity::Global(fqn.clone()), *s, false);
            }
            if let Some(owner) = self.variant_owner(&fqn, variant, 32) {
                if let Some((_, s)) = tokens.get(path.len()) {
                    self.push(Identity::Global(format!("{owner}.{variant}")), *s, false);
                }
                return Some(owner);
            }
        }
        None
    }

    /// Introduce pattern bindings and visit explicit variant types.
    fn pattern(&mut self, pattern: &Pattern, expected: &ContextType) {
        match pattern {
            Pattern::Binding { name, span } | Pattern::At { name, span, .. } => {
                if let Some(s) = self.name_span(*span, name) {
                    self.bind(name, s, false);
                    self.binding_types
                        .push((self.bindings.last().unwrap().1.clone(), expected.clone()));
                }
                if let Pattern::At { inner, .. } = pattern {
                    self.pattern(inner, expected);
                }
            }
            Pattern::Variant {
                type_path,
                variant,
                args,
                span,
            } => {
                let owner = self.variant(type_path, variant, *span, expected);
                let declaration = owner
                    .as_ref()
                    .and_then(|o| self.doc.union_decl(o))
                    .and_then(|u| u.variant(variant));
                match args {
                    VariantPatArgs::Positional(p) => {
                        let expected = declaration
                            .and_then(|v| match v.body() {
                                wcl_lang::VariantBodyView::TypeRef(t) => Some(
                                    self.doc.resolve_in(
                                        t,
                                        self.doc
                                            .union_decl(owner.as_ref().unwrap())
                                            .unwrap()
                                            .file_ns(),
                                    ),
                                ),
                                _ => None,
                            })
                            .map(|t| self.resolved_type(t, 32))
                            .unwrap_or(ContextType::Unknown);
                        self.pattern(p, &expected);
                    }
                    VariantPatArgs::Record { fields, .. } => {
                        for (field, p) in fields {
                            let start = self.out.len();
                            let expected = declaration
                                .and_then(|v| v.field(field))
                                .map(|f| self.resolved_type(f.resolved_type(), 32))
                                .unwrap_or(ContextType::Unknown);
                            self.pattern(p, &expected);
                            if let Pattern::Binding { name, span } = p
                                && field == name
                                && self.shorthand(*span)
                            {
                                for occurrence in &mut self.out[start..] {
                                    occurrence.replacement_prefix = format!("{field}: ");
                                }
                            }
                        }
                    }
                    VariantPatArgs::Unit => {}
                }
            }
            Pattern::LiteralSymbol(name, span) => self.symbol(name, *span, expected),
            Pattern::Wildcard(_)
            | Pattern::LiteralBool(..)
            | Pattern::LiteralNumber { .. }
            | Pattern::LiteralUtf8(..)
            | Pattern::LiteralAscii(..)
            | Pattern::LiteralNone(_) => {}
        }
    }

    /// Visit every expression child while retaining lexical shadowing.
    fn expr(&mut self, expr: &Expr) {
        self.expr_with(expr, &ContextType::Unknown);
    }

    fn expr_with(&mut self, expr: &Expr, expected: &ContextType) {
        let depth = self.bindings.len();
        match expr {
            Expr::Identifier(name, span) => {
                self.identifier(name, *span);
                if *expected != ContextType::Unknown
                    && let Some((_, id)) =
                        self.bindings.iter().rev().find(|(n, _)| n == name).cloned()
                    && !self.visiting.contains(&id)
                    && let Some((_, initializer)) =
                        self.initializers.iter().find(|(i, _)| i == &id).cloned()
                {
                    self.visiting.push(id.clone());
                    self.binding_types.push((id, expected.clone()));
                    self.expr_with(&initializer, expected);
                    self.visiting.pop();
                }
            }
            Expr::Symbol(name, span) => self.symbol(name, *span, expected),
            Expr::Block { lets, tail, .. } => {
                for l in lets {
                    self.expr(&l.value);
                    if let Some(span) = self.name_span(l.span, &l.name) {
                        self.bind(&l.name, span, false);
                        let id = self.bindings.last().unwrap().1.clone();
                        self.binding_types.push((id.clone(), self.infer(&l.value)));
                        self.initializers.push((id, l.value.clone()));
                    }
                }
                self.expr_with(tail, expected);
            }
            Expr::Function(f) => {
                for p in &f.params {
                    self.type_refs(p.ty_span);
                    if let Some(span) = self.name_span(p.span, &p.name) {
                        self.bind(&p.name, span, false);
                        self.binding_types.push((
                            self.bindings.last().unwrap().1.clone(),
                            self.type_context(&p.ty),
                        ));
                    }
                }
                self.type_refs(f.return_ty_span);
                self.expr_with(&f.body, &self.type_context(&f.return_ty));
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
            Expr::Call {
                callee, args, span, ..
            } => {
                let params = match self.infer(callee) {
                    ContextType::Function(params, _) => params,
                    _ => Vec::new(),
                };
                if let Expr::Identifier(name, _) = callee.as_ref()
                    && !self.bindings.iter().any(|(n, _)| n == name)
                    && matches!(name.as_str(), "decorators_for_kind" | "decorator_arg")
                {
                    let index = usize::from(name == "decorator_arg");
                    if let Some(arg) = args.get(index) {
                        self.reflective_kind_string(
                            arg,
                            *span,
                            if index == 0 { "block" } else { "decorator" },
                        );
                    }
                }
                self.expr(callee);
                for (i, a) in args.iter().enumerate() {
                    self.expr_with(a, params.get(i).unwrap_or(&ContextType::Unknown));
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                let left = self.infer(lhs);
                let right = self.infer(rhs);
                self.expr_with(
                    lhs,
                    if right == ContextType::Unknown {
                        expected
                    } else {
                        &right
                    },
                );
                self.expr_with(
                    rhs,
                    if left == ContextType::Unknown {
                        expected
                    } else {
                        &left
                    },
                );
            }
            Expr::Unary { operand, .. } => self.expr(operand),
            Expr::Paren { inner, .. } => self.expr_with(inner, expected),
            Expr::ListLit { elements, .. } => {
                for e in elements {
                    let inner = match expected {
                        ContextType::List(t) => t.as_ref(),
                        _ => &ContextType::Unknown,
                    };
                    self.expr_with(e, inner);
                }
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.expr(cond);
                self.expr_with(then_block, expected);
                if let Some(e) = else_block {
                    self.expr_with(e, expected);
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
                self.pattern(pattern, &self.infer(scrut));
                self.expr_with(then_block, expected);
                self.bindings.truncate(depth);
                self.expr_with(else_block, expected);
            }
            Expr::Match { scrut, arms, .. } => {
                self.expr(scrut);
                for arm in arms {
                    for p in &arm.patterns {
                        self.pattern(p, &self.infer(scrut));
                    }
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                    }
                    self.expr_with(&arm.body, expected);
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
                self.expr_with(body, expected);
                self.bind(binder, *binder_span, false);
                self.expr_with(handler, expected);
            }
            Expr::Variant {
                type_path,
                variant,
                args,
                span,
            } => {
                let owner = self.variant(type_path, variant, *span, expected);
                let declaration = owner
                    .as_ref()
                    .and_then(|o| self.doc.union_decl(o))
                    .and_then(|u| u.variant(variant));
                match args {
                    VariantArgs::Positional(e) => {
                        let expected = declaration
                            .and_then(|v| match v.body() {
                                wcl_lang::VariantBodyView::TypeRef(t) => Some(
                                    self.doc.resolve_in(
                                        t,
                                        self.doc
                                            .union_decl(owner.as_ref().unwrap())
                                            .unwrap()
                                            .file_ns(),
                                    ),
                                ),
                                _ => None,
                            })
                            .map(|t| self.resolved_type(t, 32))
                            .unwrap_or(ContextType::Unknown);
                        self.expr_with(e, &expected);
                    }
                    VariantArgs::Record { fields, .. } => {
                        for f in fields {
                            let expected = declaration
                                .and_then(|v| v.field(&f.name))
                                .map(|f| self.resolved_type(f.resolved_type(), 32))
                                .unwrap_or(ContextType::Unknown);
                            self.expr_with(&f.value, &expected);
                        }
                    }
                    VariantArgs::Unit => {}
                }
            }
            Expr::Record { fields, .. } => {
                let union = match expected {
                    ContextType::Named(n) => self.doc.union_decl(n),
                    _ => None,
                };
                let variants: Vec<_> = union
                    .into_iter()
                    .flat_map(|u| u.variants().collect::<Vec<_>>())
                    .filter(|v| {
                        let names: Vec<_> = v.fields().map(|f| f.name()).collect();
                        names.len() == fields.len()
                            && fields.iter().all(|f| names.contains(&f.name.as_str()))
                    })
                    .collect();
                for f in fields {
                    let expected = if variants.len() == 1 {
                        variants[0]
                            .field(&f.name)
                            .map(|f| self.resolved_type(f.resolved_type(), 32))
                            .unwrap_or(ContextType::Unknown)
                    } else {
                        self.field_context(expected, &f.name)
                    };
                    self.expr_with(&f.value, &expected);
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
