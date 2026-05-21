use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use miette::{NamedSource, SourceSpan};

use std::collections::{HashMap, HashSet};

use crate::ast::{self, Span};
use crate::error::{EvalError, ParseError};
use crate::parser::Parser;
use crate::value::{BuiltinType, TensorDim, TypeRef, Value};

#[derive(Debug)]
struct FieldCell {
    value: OnceLock<Result<Value, EvalError>>,
    evaluating: AtomicBool,
}

impl FieldCell {
    fn new() -> Self {
        Self {
            value: OnceLock::new(),
            evaluating: AtomicBool::new(false),
        }
    }
}

#[derive(Debug)]
enum ItemCells {
    Field(FieldCell),
    Block(BlockCells),
    TypeDecl,
    UnionDecl,
    NamespaceDecl,
    UseDecl,
    SymbolSetDecl,
}

#[derive(Debug)]
struct BlockCells {
    items: Vec<ItemCells>,
}

impl BlockCells {
    fn build(items: &[ast::Item]) -> Self {
        let cells = items
            .iter()
            .map(|item| match item {
                ast::Item::Field(_) => ItemCells::Field(FieldCell::new()),
                ast::Item::Block(b) => ItemCells::Block(BlockCells::build(&b.items)),
                ast::Item::TypeDecl(_) => ItemCells::TypeDecl,
                ast::Item::UnionDecl(_) => ItemCells::UnionDecl,
                ast::Item::NamespaceDecl(_) => ItemCells::NamespaceDecl,
                ast::Item::UseDecl(_) => ItemCells::UseDecl,
                ast::Item::SymbolSetDecl(_) => ItemCells::SymbolSetDecl,
            })
            .collect();
        Self { items: cells }
    }
}

#[derive(Debug)]
pub struct Document {
    src: NamedSource<String>,
    ast: ast::Source,
    cells: BlockCells,
    file_ns: Vec<String>,
    item_aliases: HashMap<String, Vec<String>>,
    ns_aliases: HashMap<String, Vec<String>>,
    wildcards: Vec<Vec<String>>,
}

impl Document {
    pub fn open(source: &str, name: &str) -> Result<Self, ParseError> {
        let ast = Parser::new(source, name).parse_source()?;
        let resolved = validate_document(&ast, source, name)?;
        let cells = BlockCells::build(&ast.items);
        Ok(Self {
            src: NamedSource::new(name, source.to_string()),
            ast,
            cells,
            file_ns: resolved.file_ns,
            item_aliases: resolved.item_aliases,
            ns_aliases: resolved.ns_aliases,
            wildcards: resolved.wildcards,
        })
    }

    pub fn from_file(path: &Path) -> Result<Self, ParseError> {
        let source = std::fs::read_to_string(path)?;
        Self::open(&source, &path.display().to_string())
    }

    pub fn source(&self) -> &NamedSource<String> {
        &self.src
    }

    pub fn field(&self, name: &str) -> Option<Field<'_>> {
        find_field(&self.ast.items, &self.cells.items, name)
    }

    pub fn block(&self, kind: &str) -> Option<Block<'_>> {
        find_block(&self.ast.items, &self.cells.items, kind)
    }

    pub fn fields(&self) -> impl Iterator<Item = Field<'_>> {
        iter_fields(&self.ast.items, &self.cells.items)
    }

    pub fn blocks(&self) -> impl Iterator<Item = Block<'_>> {
        iter_blocks(&self.ast.items, &self.cells.items)
    }

    /// Returns the file-level namespace path. Empty when the file declared none.
    pub fn namespace(&self) -> &[String] {
        &self.file_ns
    }

    pub fn uses(&self) -> impl Iterator<Item = UseDeclView<'_>> {
        self.ast.items.iter().filter_map(|item| match item {
            ast::Item::UseDecl(u) => Some(UseDeclView { ast: u }),
            _ => None,
        })
    }

    /// Look up a type by fully-qualified name (dotted).
    pub fn type_decl(&self, fqn: &str) -> Option<TypeDecl<'_>> {
        let target: Vec<&str> = fqn.split('.').collect();
        self.ast.items.iter().find_map(|item| match item {
            ast::Item::TypeDecl(t) => {
                let decl_fqn = self.compose_fqn(&t.name);
                if decl_fqn_matches(&decl_fqn, &target) {
                    Some(TypeDecl {
                        ast: t,
                        file_ns: &self.file_ns,
                    })
                } else {
                    None
                }
            }
            _ => None,
        })
    }

    pub fn type_decls(&self) -> impl Iterator<Item = TypeDecl<'_>> {
        self.ast.items.iter().filter_map(|item| match item {
            ast::Item::TypeDecl(t) => Some(TypeDecl {
                ast: t,
                file_ns: &self.file_ns,
            }),
            _ => None,
        })
    }

    /// Look up a union by fully-qualified name (dotted).
    pub fn union_decl(&self, fqn: &str) -> Option<UnionDecl<'_>> {
        let target: Vec<&str> = fqn.split('.').collect();
        self.ast.items.iter().find_map(|item| match item {
            ast::Item::UnionDecl(u) => {
                let decl_fqn = self.compose_fqn(&u.name);
                if decl_fqn_matches(&decl_fqn, &target) {
                    Some(UnionDecl {
                        ast: u,
                        file_ns: &self.file_ns,
                    })
                } else {
                    None
                }
            }
            _ => None,
        })
    }

    pub fn union_decls(&self) -> impl Iterator<Item = UnionDecl<'_>> {
        self.ast.items.iter().filter_map(|item| match item {
            ast::Item::UnionDecl(u) => Some(UnionDecl {
                ast: u,
                file_ns: &self.file_ns,
            }),
            _ => None,
        })
    }

    pub fn symbol_set(&self, fqn: &str) -> Option<SymbolSetDecl<'_>> {
        let target: Vec<&str> = fqn.split('.').collect();
        self.ast.items.iter().find_map(|item| match item {
            ast::Item::SymbolSetDecl(s) => {
                let decl_fqn = self.compose_fqn(&s.name);
                if decl_fqn_matches(&decl_fqn, &target) {
                    Some(SymbolSetDecl {
                        ast: s,
                        file_ns: &self.file_ns,
                    })
                } else {
                    None
                }
            }
            _ => None,
        })
    }

    pub fn symbol_sets(&self) -> impl Iterator<Item = SymbolSetDecl<'_>> {
        self.ast.items.iter().filter_map(|item| match item {
            ast::Item::SymbolSetDecl(s) => Some(SymbolSetDecl {
                ast: s,
                file_ns: &self.file_ns,
            }),
            _ => None,
        })
    }

    /// Resolve a [`TypeRef`] to either its built-in tag or the user-declared
    /// [`TypeDecl`] / [`UnionDecl`] it points to. `Named` refs are validated
    /// at [`Document::open`], so the lookup never fails here.
    pub fn resolve<'a>(&'a self, t: &'a TypeRef) -> ResolvedType<'a> {
        match t {
            TypeRef::Builtin(b) => ResolvedType::Builtin(*b),
            TypeRef::Named(path) => {
                let fqn = self
                    .resolve_path(path)
                    .expect("named ref validated at Document::open");
                let fqn_dotted = fqn.join(".");
                if let Some(decl) = self.type_decl(&fqn_dotted) {
                    ResolvedType::Named(decl)
                } else if let Some(union) = self.union_decl(&fqn_dotted) {
                    ResolvedType::Union(union)
                } else {
                    ResolvedType::SymbolSet(
                        self.symbol_set(&fqn_dotted)
                            .expect("named ref validated at Document::open"),
                    )
                }
            }
            TypeRef::Reference(inner) => ResolvedType::Reference(Box::new(self.resolve(inner))),
            TypeRef::List(inner) => ResolvedType::List(Box::new(self.resolve(inner))),
            TypeRef::Tensor { element, dims } => ResolvedType::Tensor {
                element: Box::new(self.resolve(element)),
                dims,
            },
        }
    }

    fn compose_fqn(&self, name: &[String]) -> Vec<String> {
        let mut v = self.file_ns.clone();
        v.extend(name.iter().cloned());
        v
    }

    /// Run the name-resolution algorithm on `path` against this document's
    /// file ns / aliases / wildcards / registry.
    fn resolve_path(&self, path: &[String]) -> Option<Vec<String>> {
        let registry: HashSet<Vec<String>> = self
            .ast
            .items
            .iter()
            .filter_map(|item| match item {
                ast::Item::TypeDecl(t) => Some(self.compose_fqn(&t.name)),
                ast::Item::UnionDecl(u) => Some(self.compose_fqn(&u.name)),
                ast::Item::SymbolSetDecl(s) => Some(self.compose_fqn(&s.name)),
                _ => None,
            })
            .collect();
        resolve_path(
            path,
            &self.file_ns,
            &self.item_aliases,
            &self.ns_aliases,
            &self.wildcards,
            &registry,
        )
    }
}

fn decl_fqn_matches(decl: &[String], target: &[&str]) -> bool {
    decl.len() == target.len() && decl.iter().zip(target.iter()).all(|(a, b)| a == b)
}

fn resolve_path(
    path: &[String],
    file_ns: &[String],
    item_aliases: &HashMap<String, Vec<String>>,
    ns_aliases: &HashMap<String, Vec<String>>,
    wildcards: &[Vec<String>],
    registry: &HashSet<Vec<String>>,
) -> Option<Vec<String>> {
    // 1. file_ns + path
    let candidate: Vec<String> = file_ns.iter().chain(path.iter()).cloned().collect();
    if registry.contains(&candidate) {
        return Some(candidate);
    }
    // 2. item alias on single-segment path
    if path.len() == 1
        && let Some(fqn) = item_aliases.get(&path[0])
        && registry.contains(fqn)
    {
        return Some(fqn.clone());
    }
    // 3. namespace alias on first segment of multi-segment path
    if path.len() > 1
        && let Some(prefix) = ns_aliases.get(&path[0])
    {
        let candidate: Vec<String> = prefix.iter().chain(path[1..].iter()).cloned().collect();
        if registry.contains(&candidate) {
            return Some(candidate);
        }
    }
    // 4. each wildcard prefix
    for w in wildcards {
        let candidate: Vec<String> = w.iter().chain(path.iter()).cloned().collect();
        if registry.contains(&candidate) {
            return Some(candidate);
        }
    }
    // 5. absolute
    if registry.contains(path) {
        return Some(path.to_vec());
    }
    None
}

#[derive(Debug)]
pub enum ResolvedType<'a> {
    Builtin(BuiltinType),
    Named(TypeDecl<'a>),
    Union(UnionDecl<'a>),
    SymbolSet(SymbolSetDecl<'a>),
    Reference(Box<ResolvedType<'a>>),
    List(Box<ResolvedType<'a>>),
    Tensor {
        element: Box<ResolvedType<'a>>,
        dims: &'a [TensorDim],
    },
}

#[derive(Debug)]
pub struct UnionDecl<'a> {
    ast: &'a ast::UnionDecl,
    file_ns: &'a [String],
}

impl<'a> UnionDecl<'a> {
    /// Last segment of the declared name.
    pub fn name(&self) -> &'a str {
        self.ast.name.last().expect("name has at least one segment")
    }

    /// Path as written in source (relative to file namespace).
    pub fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }

    /// Fully-qualified name as a dotted string.
    pub fn full_name(&self) -> String {
        self.file_ns
            .iter()
            .chain(self.ast.name.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(".")
    }

    /// Namespace path containing this declaration (file ns + decl path minus last).
    pub fn namespace(&self) -> Vec<String> {
        let mut v: Vec<String> = self.file_ns.to_vec();
        if self.ast.name.len() > 1 {
            v.extend(self.ast.name[..self.ast.name.len() - 1].iter().cloned());
        }
        v
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn variants(&self) -> impl Iterator<Item = UnionVariant<'a>> {
        self.ast.variants.iter().map(|v| UnionVariant { ast: v })
    }

    pub fn variant(&self, name: &str) -> Option<UnionVariant<'a>> {
        self.ast
            .variants
            .iter()
            .find(|v| v.name == name)
            .map(|v| UnionVariant { ast: v })
    }
}

pub struct UnionVariant<'a> {
    ast: &'a ast::UnionVariant,
}

impl<'a> UnionVariant<'a> {
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn body(&self) -> VariantBodyView<'a> {
        match &self.ast.body {
            ast::VariantBody::Record(_) => VariantBodyView::Record,
            ast::VariantBody::TypeRef { ty, .. } => VariantBodyView::TypeRef(ty),
            ast::VariantBody::Unit => VariantBodyView::Unit,
        }
    }

    pub fn fields(&self) -> Box<dyn Iterator<Item = TypeField<'a>> + 'a> {
        match &self.ast.body {
            ast::VariantBody::Record(fields) => {
                Box::new(fields.iter().map(|f| TypeField { ast: f }))
            }
            _ => Box::new(std::iter::empty()),
        }
    }

    pub fn field(&self, name: &str) -> Option<TypeField<'a>> {
        match &self.ast.body {
            ast::VariantBody::Record(fields) => fields
                .iter()
                .find(|f| f.name == name)
                .map(|f| TypeField { ast: f }),
            _ => None,
        }
    }
}

pub enum VariantBodyView<'a> {
    Record,
    TypeRef(&'a TypeRef),
    Unit,
}

#[derive(Debug)]
pub struct SymbolSetDecl<'a> {
    ast: &'a ast::SymbolSetDecl,
    file_ns: &'a [String],
}

impl<'a> SymbolSetDecl<'a> {
    pub fn name(&self) -> &'a str {
        self.ast.name.last().expect("name has at least one segment")
    }

    pub fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }

    pub fn full_name(&self) -> String {
        self.file_ns
            .iter()
            .chain(self.ast.name.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(".")
    }

    pub fn namespace(&self) -> Vec<String> {
        let mut v: Vec<String> = self.file_ns.to_vec();
        if self.ast.name.len() > 1 {
            v.extend(self.ast.name[..self.ast.name.len() - 1].iter().cloned());
        }
        v
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn symbols(&self) -> impl Iterator<Item = SymbolEntry<'a>> {
        self.ast.symbols.iter().map(|s| SymbolEntry { ast: s })
    }

    pub fn has(&self, name: &str) -> bool {
        self.ast.symbols.iter().any(|s| s.name == name)
    }
}

pub struct SymbolEntry<'a> {
    ast: &'a ast::SymbolEntry,
}

impl<'a> SymbolEntry<'a> {
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }
}

#[derive(Debug)]
pub struct TypeDecl<'a> {
    ast: &'a ast::TypeDecl,
    file_ns: &'a [String],
}

impl<'a> TypeDecl<'a> {
    pub fn name(&self) -> &'a str {
        self.ast.name.last().expect("name has at least one segment")
    }

    pub fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }

    pub fn full_name(&self) -> String {
        self.file_ns
            .iter()
            .chain(self.ast.name.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(".")
    }

    pub fn namespace(&self) -> Vec<String> {
        let mut v: Vec<String> = self.file_ns.to_vec();
        if self.ast.name.len() > 1 {
            v.extend(self.ast.name[..self.ast.name.len() - 1].iter().cloned());
        }
        v
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn fields(&self) -> impl Iterator<Item = TypeField<'a>> {
        self.ast.fields.iter().map(|f| TypeField { ast: f })
    }

    pub fn field(&self, name: &str) -> Option<TypeField<'a>> {
        self.ast
            .fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| TypeField { ast: f })
    }
}

pub struct UseDeclView<'a> {
    ast: &'a ast::UseDecl,
}

impl<'a> UseDeclView<'a> {
    pub fn path(&self) -> &'a [String] {
        &self.ast.path
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn form(&self) -> UseFormView<'a> {
        match &self.ast.form {
            ast::UseForm::Bare(alias) => UseFormView::Bare(alias.as_deref()),
            ast::UseForm::List(_) => UseFormView::List,
        }
    }

    /// If this `use` is a brace-list form, iterate its items.
    pub fn items(&self) -> Box<dyn Iterator<Item = UseItem<'a>> + 'a> {
        match &self.ast.form {
            ast::UseForm::List(items) => Box::new(items.iter().map(|i| UseItem { ast: i })),
            ast::UseForm::Bare(_) => Box::new(std::iter::empty()),
        }
    }
}

pub enum UseFormView<'a> {
    Bare(Option<&'a str>),
    List,
}

pub struct UseItem<'a> {
    ast: &'a ast::UseItem,
}

impl<'a> UseItem<'a> {
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn alias(&self) -> Option<&'a str> {
        self.ast.alias.as_deref()
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }
}

pub struct TypeField<'a> {
    ast: &'a ast::TypeField,
}

impl<'a> TypeField<'a> {
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn optional(&self) -> bool {
        self.ast.optional
    }

    pub fn type_ref(&self) -> &'a TypeRef {
        &self.ast.ty
    }
}

pub struct Field<'a> {
    ast: &'a ast::Field,
    cell: &'a FieldCell,
}

impl<'a> Field<'a> {
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn value(&self) -> Result<&'a Value, &'a EvalError> {
        if let Some(cached) = self.cell.value.get() {
            return cached.as_ref();
        }
        if self.cell.evaluating.swap(true, Ordering::Acquire) {
            let _ = self.cell.value.set(Err(EvalError::Cycle {
                field: self.ast.name.clone(),
                span: span_to_miette(self.ast.span),
            }));
            return self
                .cell
                .value
                .get()
                .expect("cycle cell was just initialised")
                .as_ref();
        }
        let result = eval_expr(&self.ast.expr);
        self.cell.evaluating.store(false, Ordering::Release);
        self.cell.value.get_or_init(|| result).as_ref()
    }
}

pub struct Block<'a> {
    ast: &'a ast::Block,
    cells: &'a BlockCells,
}

impl<'a> Block<'a> {
    pub fn kind(&self) -> &'a str {
        &self.ast.kind
    }

    pub fn labels(&self) -> &'a [String] {
        &self.ast.labels
    }

    pub fn span(&self) -> Span {
        self.ast.span
    }

    pub fn field(&self, name: &str) -> Option<Field<'a>> {
        find_field(&self.ast.items, &self.cells.items, name)
    }

    pub fn block(&self, kind: &str) -> Option<Block<'a>> {
        find_block(&self.ast.items, &self.cells.items, kind)
    }

    pub fn fields(&self) -> impl Iterator<Item = Field<'a>> {
        iter_fields(&self.ast.items, &self.cells.items)
    }

    pub fn blocks(&self) -> impl Iterator<Item = Block<'a>> {
        iter_blocks(&self.ast.items, &self.cells.items)
    }
}

fn find_field<'a>(items: &'a [ast::Item], cells: &'a [ItemCells], name: &str) -> Option<Field<'a>> {
    items
        .iter()
        .zip(cells)
        .find_map(|(item, cell)| match (item, cell) {
            (ast::Item::Field(f), ItemCells::Field(c)) if f.name == name => {
                Some(Field { ast: f, cell: c })
            }
            _ => None,
        })
}

fn find_block<'a>(items: &'a [ast::Item], cells: &'a [ItemCells], kind: &str) -> Option<Block<'a>> {
    items
        .iter()
        .zip(cells)
        .find_map(|(item, cell)| match (item, cell) {
            (ast::Item::Block(b), ItemCells::Block(c)) if b.kind == kind => {
                Some(Block { ast: b, cells: c })
            }
            _ => None,
        })
}

fn iter_fields<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
) -> impl Iterator<Item = Field<'a>> {
    items
        .iter()
        .zip(cells)
        .filter_map(|(item, cell)| match (item, cell) {
            (ast::Item::Field(f), ItemCells::Field(c)) => Some(Field { ast: f, cell: c }),
            _ => None,
        })
}

fn iter_blocks<'a>(
    items: &'a [ast::Item],
    cells: &'a [ItemCells],
) -> impl Iterator<Item = Block<'a>> {
    items
        .iter()
        .zip(cells)
        .filter_map(|(item, cell)| match (item, cell) {
            (ast::Item::Block(b), ItemCells::Block(c)) => Some(Block { ast: b, cells: c }),
            _ => None,
        })
}

fn eval_expr(e: &ast::Expr) -> Result<Value, EvalError> {
    Ok(match e {
        ast::Expr::Bool(b) => Value::Bool(*b),
        ast::Expr::I8(v) => Value::I8(*v),
        ast::Expr::I16(v) => Value::I16(*v),
        ast::Expr::I32(v) => Value::I32(*v),
        ast::Expr::I64(v) => Value::I64(*v),
        ast::Expr::I128(v) => Value::I128(*v),
        ast::Expr::Isize(v) => Value::Isize(*v),
        ast::Expr::U8(v) => Value::U8(*v),
        ast::Expr::U16(v) => Value::U16(*v),
        ast::Expr::U32(v) => Value::U32(*v),
        ast::Expr::U64(v) => Value::U64(*v),
        ast::Expr::U128(v) => Value::U128(*v),
        ast::Expr::Usize(v) => Value::Usize(*v),
        ast::Expr::F32(v) => Value::F32(*v),
        ast::Expr::F64(v) => Value::F64(*v),
        ast::Expr::Utf8(s) => Value::Utf8(s.clone()),
        ast::Expr::Ascii(s) => Value::Ascii(s.clone()),
        ast::Expr::Utf16(v) => Value::Utf16(v.clone()),
        ast::Expr::Utf32(v) => Value::Utf32(v.clone()),
        ast::Expr::Reference(s) => Value::Reference(s.clone()),
        ast::Expr::Symbol(s) => Value::Symbol(s.clone()),
        ast::Expr::None => Value::None,
    })
}

fn span_to_miette(span: Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.len().max(1))
}

struct Resolved {
    file_ns: Vec<String>,
    item_aliases: HashMap<String, Vec<String>>,
    ns_aliases: HashMap<String, Vec<String>>,
    wildcards: Vec<Vec<String>>,
}

fn validate_document(ast: &ast::Source, source: &str, file: &str) -> Result<Resolved, ParseError> {
    // 1. Namespace must be first if present; at most one.
    let mut file_ns: Vec<String> = Vec::new();
    let mut saw_ns = false;
    for (idx, item) in ast.items.iter().enumerate() {
        if let ast::Item::NamespaceDecl(n) = item {
            if saw_ns {
                return Err(open_error(
                    source,
                    file,
                    "duplicate namespace declaration".to_string(),
                    n.span,
                    "duplicate namespace",
                ));
            }
            if idx != 0 {
                return Err(open_error(
                    source,
                    file,
                    "namespace declaration must be the first item in the file".to_string(),
                    n.span,
                    "must be first item",
                ));
            }
            file_ns = n.path.clone();
            saw_ns = true;
        }
    }

    // 2. FQN registry and prefix set.
    let mut declared: HashSet<Vec<String>> = HashSet::new();
    let mut prefixes: HashSet<Vec<String>> = HashSet::new();
    for item in &ast.items {
        let fqn = match item {
            ast::Item::TypeDecl(t) => compose(&file_ns, &t.name),
            ast::Item::UnionDecl(u) => compose(&file_ns, &u.name),
            ast::Item::SymbolSetDecl(s) => compose(&file_ns, &s.name),
            _ => continue,
        };
        if !declared.insert(fqn.clone()) {
            let span = match item {
                ast::Item::TypeDecl(t) => t.span,
                ast::Item::UnionDecl(u) => u.span,
                ast::Item::SymbolSetDecl(s) => s.span,
                _ => unreachable!(),
            };
            return Err(open_error(
                source,
                file,
                format!("duplicate declaration '{}'", fqn.join(".")),
                span,
                "duplicate declaration",
            ));
        }
        for n in 1..fqn.len() {
            prefixes.insert(fqn[..n].to_vec());
        }
    }

    // 3. Use declarations.
    let mut item_aliases: HashMap<String, Vec<String>> = HashMap::new();
    let mut ns_aliases: HashMap<String, Vec<String>> = HashMap::new();
    let mut wildcards: Vec<Vec<String>> = Vec::new();
    let mut alias_taken: HashSet<String> = HashSet::new();

    let record_alias =
        |alias: String, span: Span, taken: &mut HashSet<String>| -> Result<(), ParseError> {
            if !taken.insert(alias.clone()) {
                return Err(open_error(
                    source,
                    file,
                    format!("duplicate use alias '{alias}'"),
                    span,
                    "duplicate alias",
                ));
            }
            Ok(())
        };

    for item in &ast.items {
        let ast::Item::UseDecl(u) = item else {
            continue;
        };
        match &u.form {
            ast::UseForm::Bare(alias) => {
                let path_is_leaf = declared.contains(&u.path);
                let path_is_prefix = prefixes.contains(&u.path);
                match alias {
                    None => {
                        if path_is_leaf {
                            let local = u.path.last().expect("non-empty path").clone();
                            record_alias(local.clone(), u.span, &mut alias_taken)?;
                            item_aliases.insert(local, u.path.clone());
                        } else if path_is_prefix {
                            wildcards.push(u.path.clone());
                        } else {
                            return Err(open_error(
                                source,
                                file,
                                format!("unknown use target '{}'", u.path.join(".")),
                                u.span,
                                "not declared",
                            ));
                        }
                    }
                    Some(alias_name) => {
                        if path_is_leaf {
                            record_alias(alias_name.clone(), u.span, &mut alias_taken)?;
                            item_aliases.insert(alias_name.clone(), u.path.clone());
                        } else if path_is_prefix {
                            record_alias(alias_name.clone(), u.span, &mut alias_taken)?;
                            ns_aliases.insert(alias_name.clone(), u.path.clone());
                        } else {
                            return Err(open_error(
                                source,
                                file,
                                format!("unknown use target '{}'", u.path.join(".")),
                                u.span,
                                "not declared",
                            ));
                        }
                    }
                }
            }
            ast::UseForm::List(items) => {
                if declared.contains(&u.path) {
                    return Err(open_error(
                        source,
                        file,
                        format!(
                            "expected namespace, but '{}' names a type",
                            u.path.join(".")
                        ),
                        u.span,
                        "not a namespace",
                    ));
                }
                if !u.path.is_empty() && !prefixes.contains(&u.path) {
                    return Err(open_error(
                        source,
                        file,
                        format!("unknown use target '{}'", u.path.join(".")),
                        u.span,
                        "not declared",
                    ));
                }
                for it in items {
                    let mut full = u.path.clone();
                    full.push(it.name.clone());
                    if !declared.contains(&full) {
                        return Err(open_error(
                            source,
                            file,
                            format!("unknown use target '{}'", full.join(".")),
                            it.span,
                            "not declared",
                        ));
                    }
                    let local = it.alias.clone().unwrap_or_else(|| it.name.clone());
                    record_alias(local.clone(), it.span, &mut alias_taken)?;
                    item_aliases.insert(local, full);
                }
            }
        }
    }

    // 4. TypeRef resolution + variant-name uniqueness.
    for item in &ast.items {
        match item {
            ast::Item::TypeDecl(t) => {
                for f in &t.fields {
                    check_type_ref(
                        &f.ty,
                        f.ty_span,
                        &declared,
                        &file_ns,
                        &item_aliases,
                        &ns_aliases,
                        &wildcards,
                        source,
                        file,
                    )?;
                }
            }
            ast::Item::UnionDecl(u) => {
                let mut seen: HashSet<String> = HashSet::new();
                for v in &u.variants {
                    if !seen.insert(v.name.clone()) {
                        return Err(open_error(
                            source,
                            file,
                            format!(
                                "duplicate variant '{}' in union '{}'",
                                v.name,
                                u.name.join(".")
                            ),
                            v.span,
                            "duplicate variant",
                        ));
                    }
                    match &v.body {
                        ast::VariantBody::Record(fields) => {
                            for f in fields {
                                check_type_ref(
                                    &f.ty,
                                    f.ty_span,
                                    &declared,
                                    &file_ns,
                                    &item_aliases,
                                    &ns_aliases,
                                    &wildcards,
                                    source,
                                    file,
                                )?;
                            }
                        }
                        ast::VariantBody::TypeRef { ty, ty_span } => {
                            check_type_ref(
                                ty,
                                *ty_span,
                                &declared,
                                &file_ns,
                                &item_aliases,
                                &ns_aliases,
                                &wildcards,
                                source,
                                file,
                            )?;
                        }
                        ast::VariantBody::Unit => {}
                    }
                }
            }
            ast::Item::SymbolSetDecl(s) => {
                let mut seen: HashSet<String> = HashSet::new();
                for entry in &s.symbols {
                    if !seen.insert(entry.name.clone()) {
                        return Err(open_error(
                            source,
                            file,
                            format!(
                                "duplicate symbol '{}' in symbol_set '{}'",
                                entry.name,
                                s.name.join(".")
                            ),
                            entry.span,
                            "duplicate symbol",
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(Resolved {
        file_ns,
        item_aliases,
        ns_aliases,
        wildcards,
    })
}

fn compose(file_ns: &[String], name: &[String]) -> Vec<String> {
    file_ns.iter().chain(name.iter()).cloned().collect()
}

#[allow(clippy::too_many_arguments)]
fn check_type_ref(
    t: &TypeRef,
    ty_span: Span,
    declared: &HashSet<Vec<String>>,
    file_ns: &[String],
    item_aliases: &HashMap<String, Vec<String>>,
    ns_aliases: &HashMap<String, Vec<String>>,
    wildcards: &[Vec<String>],
    source: &str,
    file: &str,
) -> Result<(), ParseError> {
    match t {
        TypeRef::Builtin(_) => Ok(()),
        TypeRef::Named(path) => {
            if resolve_path(path, file_ns, item_aliases, ns_aliases, wildcards, declared).is_some()
            {
                Ok(())
            } else {
                Err(open_error(
                    source,
                    file,
                    format!("unknown type '{}'", path.join(".")),
                    ty_span,
                    "type not declared",
                ))
            }
        }
        TypeRef::Reference(inner) => check_type_ref(
            inner,
            ty_span,
            declared,
            file_ns,
            item_aliases,
            ns_aliases,
            wildcards,
            source,
            file,
        ),
        TypeRef::List(inner) => check_type_ref(
            inner,
            ty_span,
            declared,
            file_ns,
            item_aliases,
            ns_aliases,
            wildcards,
            source,
            file,
        ),
        TypeRef::Tensor { element, .. } => check_type_ref(
            element,
            ty_span,
            declared,
            file_ns,
            item_aliases,
            ns_aliases,
            wildcards,
            source,
            file,
        ),
    }
}

fn open_error(source: &str, file: &str, message: String, span: Span, label: &str) -> ParseError {
    ParseError::syntax(
        message,
        NamedSource::new(file, source.to_string()),
        span_to_miette(span),
        label.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(src: &str) -> Document {
        Document::open(src, "test").expect("open")
    }

    #[test]
    fn document_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Document>();
    }

    #[test]
    fn default_scalars_resolve_to_default_types() {
        let doc = open(
            r#"
            s = "alpha"
            i = 3
            f = 2.5
            b = true
            "#,
        );
        assert_eq!(
            doc.field("s").unwrap().value().unwrap(),
            &Value::Utf8("alpha".into())
        );
        assert_eq!(doc.field("i").unwrap().value().unwrap(), &Value::I64(3));
        assert_eq!(doc.field("f").unwrap().value().unwrap(), &Value::F64(2.5));
        assert_eq!(doc.field("b").unwrap().value().unwrap(), &Value::Bool(true));
    }

    #[test]
    fn every_typed_literal_evaluates() {
        let doc = open(
            r#"
            a = 1i8
            b = 2i16
            c = 3i32
            d = 4i64
            e = 5i128
            f = 6isize
            g = 7u8
            h = 8u16
            i = 9u32
            j = 10u64
            k = 11u128
            l = 12usize
            m = 1.5f32
            n = 2.5f64
            s_utf8  = utf8"alpha"
            s_ascii = ascii"beta"
            s_utf16 = utf16"gamma"
            s_utf32 = utf32"delta"
            "#,
        );
        assert_eq!(doc.field("a").unwrap().value().unwrap(), &Value::I8(1));
        assert_eq!(doc.field("b").unwrap().value().unwrap(), &Value::I16(2));
        assert_eq!(doc.field("c").unwrap().value().unwrap(), &Value::I32(3));
        assert_eq!(doc.field("d").unwrap().value().unwrap(), &Value::I64(4));
        assert_eq!(doc.field("e").unwrap().value().unwrap(), &Value::I128(5));
        assert_eq!(doc.field("f").unwrap().value().unwrap(), &Value::Isize(6));
        assert_eq!(doc.field("g").unwrap().value().unwrap(), &Value::U8(7));
        assert_eq!(doc.field("h").unwrap().value().unwrap(), &Value::U16(8));
        assert_eq!(doc.field("i").unwrap().value().unwrap(), &Value::U32(9));
        assert_eq!(doc.field("j").unwrap().value().unwrap(), &Value::U64(10));
        assert_eq!(doc.field("k").unwrap().value().unwrap(), &Value::U128(11));
        assert_eq!(doc.field("l").unwrap().value().unwrap(), &Value::Usize(12));
        assert_eq!(doc.field("m").unwrap().value().unwrap(), &Value::F32(1.5));
        assert_eq!(doc.field("n").unwrap().value().unwrap(), &Value::F64(2.5));
        assert_eq!(
            doc.field("s_utf8").unwrap().value().unwrap(),
            &Value::Utf8("alpha".into())
        );
        assert_eq!(
            doc.field("s_ascii").unwrap().value().unwrap(),
            &Value::Ascii("beta".into())
        );
        assert_eq!(
            doc.field("s_utf16").unwrap().value().unwrap(),
            &Value::Utf16("gamma".encode_utf16().collect())
        );
        assert_eq!(
            doc.field("s_utf32").unwrap().value().unwrap(),
            &Value::Utf32("delta".chars().collect())
        );
    }

    #[test]
    fn strict_typing_distinguishes_widths() {
        let doc = open("a = 42i32\nb = 42i64\n");
        let a = doc.field("a").unwrap().value().unwrap();
        let b = doc.field("b").unwrap().value().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn field_value_caches_on_first_access() {
        let doc = open(r#"name = "alpha""#);
        let f = doc.field("name").unwrap();
        let first = f.value().unwrap() as *const Value;
        let second = f.value().unwrap() as *const Value;
        assert_eq!(first, second);
    }

    #[test]
    fn span_is_available_without_forcing_eval() {
        let doc = open(r#"name = "alpha""#);
        let f = doc.field("name").unwrap();
        assert!(!f.span().is_empty());
        // value() not called; the OnceLock should still be empty.
        assert!(f.cell.value.get().is_none());
    }

    #[test]
    fn block_field_resolves() {
        let doc = open(r#"service "web" { port = 8080 }"#);
        let b = doc.block("service").unwrap();
        assert_eq!(b.labels(), &["web".to_string()]);
        assert_eq!(b.field("port").unwrap().value().unwrap(), &Value::I64(8080));
    }

    #[test]
    fn nested_block_field_resolves() {
        let doc = open(
            r#"
            service "web" {
              metadata {
                region = "us-east-1"
              }
            }
            "#,
        );
        let svc = doc.block("service").unwrap();
        let meta = svc.block("metadata").unwrap();
        assert_eq!(
            meta.field("region").unwrap().value().unwrap(),
            &Value::Utf8("us-east-1".into())
        );
    }

    #[test]
    fn unknown_field_returns_none() {
        let doc = open(r#"name = "alpha""#);
        assert!(doc.field("missing").is_none());
    }

    #[test]
    fn unknown_block_returns_none() {
        let doc = open(r#"name = "alpha""#);
        assert!(doc.block("missing").is_none());
    }

    #[test]
    fn fields_iter_yields_only_root_fields() {
        let doc = open(
            r#"a = 1
                          b "label" { c = 2 }
                          d = 3"#,
        );
        let names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(names, vec!["a".to_string(), "d".to_string()]);
    }

    #[test]
    fn blocks_iter_yields_only_root_blocks() {
        let doc = open(
            r#"a = 1
                          b "label" { c = 2 }
                          d {}"#,
        );
        let kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
        assert_eq!(kinds, vec!["b".to_string(), "d".to_string()]);
    }

    #[test]
    fn reference_value_resolves() {
        let doc = open("owner = wil_taylor");
        assert_eq!(
            doc.field("owner").unwrap().value().unwrap(),
            &Value::Reference("wil_taylor".into())
        );
    }

    #[test]
    fn none_value_resolves() {
        let doc = open("maybe = none");
        assert_eq!(doc.field("maybe").unwrap().value().unwrap(), &Value::None);
    }

    #[test]
    fn type_decls_are_queryable() {
        use crate::value::BuiltinType;
        let doc = open(
            r#"
            type User {
              name: utf8
              bio:  utf8?
              link: &User?
            }
            type Empty {}
            "#,
        );
        assert_eq!(doc.type_decls().count(), 2);
        let user = doc.type_decl("User").expect("User type");
        let fields: Vec<_> = user.fields().collect();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].name(), "name");
        assert_eq!(fields[0].type_ref(), &TypeRef::Builtin(BuiltinType::Utf8));
        assert!(!fields[0].optional());
        assert_eq!(fields[1].name(), "bio");
        assert!(fields[1].optional());
        assert_eq!(fields[2].name(), "link");
        assert_eq!(
            fields[2].type_ref(),
            &TypeRef::Reference(Box::new(TypeRef::Named(vec!["User".into()])))
        );
        assert!(fields[2].optional());
        assert!(doc.type_decl("Empty").unwrap().fields().count() == 0);
    }

    #[test]
    fn type_decl_named_field_lookup() {
        let doc = open("type Point { x: i32 y: i32 }");
        let t = doc.type_decl("Point").unwrap();
        assert!(t.field("x").is_some());
        assert!(t.field("y").is_some());
        assert!(t.field("z").is_none());
    }

    #[test]
    fn type_decls_dont_appear_in_field_or_block_iteration() {
        let doc = open(
            r#"
            type User { name: utf8 }
            count = 1
            svc {}
            "#,
        );
        let field_names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(field_names, vec!["count".to_string()]);
        let block_kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
        assert_eq!(block_kinds, vec!["svc".to_string()]);
    }

    #[test]
    fn forward_ref_resolves_at_open() {
        let doc = open("type A { b: B }\ntype B { x: i32 }");
        let a = doc.type_decl("A").unwrap();
        let b_field = a.field("b").unwrap();
        match doc.resolve(b_field.type_ref()) {
            ResolvedType::Named(decl) => assert_eq!(decl.name(), "B"),
            _ => panic!("expected named ResolvedType::Named"),
        }
    }

    #[test]
    fn self_ref_resolves_at_open() {
        let doc = open("type Tree { parent: Tree? }");
        let t = doc.type_decl("Tree").unwrap();
        let parent = t.field("parent").unwrap();
        match doc.resolve(parent.type_ref()) {
            ResolvedType::Named(decl) => assert_eq!(decl.name(), "Tree"),
            _ => panic!("expected ResolvedType::Named"),
        }
    }

    #[test]
    fn unknown_type_ref_errors_at_open() {
        let err = Document::open("type X { y: NotDecl }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("unknown type 'NotDecl'"),
                    "message: {}",
                    e.message
                );
                // Span covers exactly the type ident `NotDecl`.
                assert_eq!(e.span.offset(), "type X { y: ".len());
                assert_eq!(e.span.len(), "NotDecl".len());
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn duplicate_type_decl_errors_at_open() {
        let err = Document::open("type Foo { x: i32 }\ntype Foo { y: i32 }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("duplicate declaration 'Foo'"),
                    "message: {}",
                    e.message
                );
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn type_and_union_with_same_name_errors() {
        let err = Document::open("type Foo {}\nunion Foo {}", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("duplicate declaration 'Foo'"),
                    "message: {}",
                    e.message
                );
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn duplicate_variant_name_errors() {
        let err = Document::open("union X { A none A none }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("duplicate variant 'A' in union 'X'"),
                    "message: {}",
                    e.message
                );
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn union_decls_are_queryable() {
        let doc = open(
            r#"
            type Point { x: f64 y: f64 }
            union Shape {
              Circle { radius: f64 }
              Polygon Point
              Empty none
            }
            union Maybe { Some { v: i32 } Nothing none }
            "#,
        );
        assert_eq!(doc.union_decls().count(), 2);
        let shape = doc.union_decl("Shape").expect("Shape union");
        assert_eq!(shape.variants().count(), 3);
        assert!(shape.variant("Circle").is_some());
        assert!(shape.variant("missing").is_none());
    }

    #[test]
    fn variant_record_fields_iterate() {
        let doc = open(
            "type Point { x: f64 y: f64 }\nunion Shape { Circle { radius: f64 center: Point } }",
        );
        let shape = doc.union_decl("Shape").unwrap();
        let circle = shape.variant("Circle").unwrap();
        assert!(matches!(circle.body(), VariantBodyView::Record));
        let names: Vec<_> = circle.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(names, vec!["radius".to_string(), "center".to_string()]);
    }

    #[test]
    fn variant_type_ref_body_resolves() {
        let doc = open("type Point { x: f64 y: f64 }\nunion Shape { Polygon Point }");
        let shape = doc.union_decl("Shape").unwrap();
        let v = shape.variant("Polygon").unwrap();
        match v.body() {
            VariantBodyView::TypeRef(t) => assert_eq!(*t, TypeRef::Named(vec!["Point".into()])),
            _ => panic!("expected TypeRef body"),
        }
    }

    #[test]
    fn variant_unit_body() {
        let doc = open("union Maybe { Nothing none }");
        let m = doc.union_decl("Maybe").unwrap();
        let n = m.variant("Nothing").unwrap();
        assert!(matches!(n.body(), VariantBodyView::Unit));
        assert_eq!(n.fields().count(), 0);
    }

    #[test]
    fn resolve_union_returns_union() {
        let doc = open("union Shape { Empty none }\ntype Box { contents: Shape }");
        let b = doc.type_decl("Box").unwrap();
        let contents = b.field("contents").unwrap();
        match doc.resolve(contents.type_ref()) {
            ResolvedType::Union(u) => assert_eq!(u.name(), "Shape"),
            _ => panic!("expected union"),
        }
    }

    #[test]
    fn unknown_variant_body_type_errors() {
        let err = Document::open("union X { V NotDecl }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("unknown type 'NotDecl'"),
                    "message: {}",
                    e.message
                );
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn namespace_sets_file_ns_and_qualifies_decls() {
        let doc = open("namespace foo.bar\ntype X {}");
        assert_eq!(doc.namespace(), &["foo".to_string(), "bar".to_string()]);
        let x = doc.type_decl("foo.bar.X").expect("found");
        assert_eq!(x.full_name(), "foo.bar.X");
        assert_eq!(x.name(), "X");
    }

    #[test]
    fn dotted_decl_name_extends_file_ns() {
        let doc = open("namespace foo\ntype a.b.X {}");
        assert!(doc.type_decl("foo.a.b.X").is_some());
        let x = doc.type_decl("foo.a.b.X").unwrap();
        assert_eq!(x.name(), "X");
        assert_eq!(x.full_name(), "foo.a.b.X");
        assert_eq!(
            x.namespace(),
            vec!["foo".to_string(), "a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn local_name_resolves_within_file_ns() {
        let doc = open(
            r#"
            namespace app
            type X { name: utf8 }
            type Y { f: X }
            "#,
        );
        let y = doc.type_decl("app.Y").unwrap();
        match doc.resolve(y.field("f").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "app.X"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn item_alias_with_as_resolves() {
        let doc = open(
            r#"
            type a.b.Baz {}
            use a.b.Baz as MB
            type Q { f: MB }
            "#,
        );
        let q = doc.type_decl("Q").unwrap();
        match doc.resolve(q.field("f").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Baz"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn wildcard_import_resolves_bare_name() {
        let doc = open(
            r#"
            type a.b.Baz {}
            use a.b
            type Q { f: Baz }
            "#,
        );
        let q = doc.type_decl("Q").unwrap();
        match doc.resolve(q.field("f").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Baz"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn namespace_alias_with_as_resolves() {
        let doc = open(
            r#"
            type a.b.Baz {}
            use a.b as B
            type Q { f: B.Baz }
            "#,
        );
        let q = doc.type_decl("Q").unwrap();
        match doc.resolve(q.field("f").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Baz"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn brace_list_imports_each_item() {
        let doc = open(
            r#"
            type a.b.X {}
            type a.b.Y {}
            use a.b.{X, Y as Z}
            type Q { f1: X f2: Z }
            "#,
        );
        let q = doc.type_decl("Q").unwrap();
        match doc.resolve(q.field("f1").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.X"),
            _ => panic!("f1"),
        }
        match doc.resolve(q.field("f2").unwrap().type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.full_name(), "a.b.Y"),
            _ => panic!("f2"),
        }
    }

    #[test]
    fn namespace_must_be_first_item() {
        let err = Document::open("type X {}\nnamespace foo", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("must be the first item"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn duplicate_namespace_errors() {
        let err = Document::open("namespace foo\nnamespace bar", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("duplicate namespace"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn unknown_use_target_errors() {
        let err = Document::open("use foo.bar.Nope", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("unknown use target"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn brace_list_unknown_item_errors() {
        let err = Document::open("type a.b.X {}\nuse a.b.{Nope}", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("unknown use target"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn brace_list_on_leaf_errors() {
        // foo.bar.X is a leaf; can't do use foo.bar.X.{Y}
        let err = Document::open("type foo.bar.X {}\nuse foo.bar.X.{Y}", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("expected namespace"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn duplicate_alias_errors() {
        let err = Document::open(
            r#"
            type a.X {}
            type b.X {}
            use a.X
            use b.X
            "#,
            "test",
        )
        .unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("duplicate use alias"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn unions_dont_appear_in_field_or_block_iteration() {
        let doc = open(
            r#"
            union Shape { Empty none }
            count = 1
            svc {}
            "#,
        );
        let field_names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(field_names, vec!["count".to_string()]);
        let block_kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
        assert_eq!(block_kinds, vec!["svc".to_string()]);
    }

    #[test]
    fn resolve_reference_unwraps_to_named() {
        let doc = open("type User { name: utf8 }\ntype Post { author: &User }");
        let post = doc.type_decl("Post").unwrap();
        let author = post.field("author").unwrap();
        let resolved = doc.resolve(author.type_ref());
        let ResolvedType::Reference(inner) = resolved else {
            panic!("expected reference");
        };
        let ResolvedType::Named(decl) = *inner else {
            panic!("expected inner Named");
        };
        assert_eq!(decl.name(), "User");
    }

    #[test]
    fn resolve_reference_to_builtin() {
        let doc = open("type Score { value: &i32 }");
        let s = doc.type_decl("Score").unwrap();
        let v = s.field("value").unwrap();
        let resolved = doc.resolve(v.type_ref());
        let ResolvedType::Reference(inner) = resolved else {
            panic!("expected reference");
        };
        let ResolvedType::Builtin(b) = *inner else {
            panic!("expected inner builtin");
        };
        assert_eq!(b, BuiltinType::I32);
    }

    #[test]
    fn unknown_reference_target_errors() {
        let err = Document::open("type X { y: &NotDecl }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("unknown type 'NotDecl'"),
                    "message: {}",
                    e.message
                );
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn resolve_builtin_returns_builtin() {
        let doc = open("");
        match doc.resolve(&TypeRef::Builtin(BuiltinType::I32)) {
            ResolvedType::Builtin(b) => assert_eq!(b, BuiltinType::I32),
            _ => panic!("expected builtin"),
        }
    }

    #[test]
    fn resolve_lets_caller_walk_named_decl_fields() {
        let doc = open(
            r#"
            type Inner { a: i32 b: utf8 }
            type Outer { inner: Inner }
            "#,
        );
        let outer = doc.type_decl("Outer").unwrap();
        let inner_field = outer.field("inner").unwrap();
        let ResolvedType::Named(inner_decl) = doc.resolve(inner_field.type_ref()) else {
            panic!("expected named");
        };
        let sub_names: Vec<_> = inner_decl.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(sub_names, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn resolve_list_of_named() {
        let doc = open("type User { name: utf8 }\ntype Q { items: list<User> }");
        let q = doc.type_decl("Q").unwrap();
        let items = q.field("items").unwrap();
        let resolved = doc.resolve(items.type_ref());
        let ResolvedType::List(inner) = resolved else {
            panic!("expected List");
        };
        let ResolvedType::Named(decl) = *inner else {
            panic!("expected Named inner");
        };
        assert_eq!(decl.name(), "User");
    }

    #[test]
    fn resolve_tensor_keeps_dims() {
        let doc = open("type Q { w: tensor<f32, [3, N, 5]> }");
        let q = doc.type_decl("Q").unwrap();
        let w = q.field("w").unwrap();
        let resolved = doc.resolve(w.type_ref());
        let ResolvedType::Tensor { element, dims } = resolved else {
            panic!("expected Tensor");
        };
        let ResolvedType::Builtin(b) = *element else {
            panic!("expected Builtin element");
        };
        assert_eq!(b, BuiltinType::F32);
        assert_eq!(
            dims,
            &[
                TensorDim::Fixed(3),
                TensorDim::Symbolic("N".into()),
                TensorDim::Fixed(5),
            ]
        );
    }

    #[test]
    fn unknown_list_element_type_errors() {
        let err = Document::open("type Q { items: list<NotDecl> }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("unknown type 'NotDecl'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn unknown_tensor_element_type_errors() {
        let err = Document::open("type Q { w: tensor<NotDecl, [4]> }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("unknown type 'NotDecl'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn list_of_reference_resolves() {
        let doc = open("type T { x: i32 }\ntype Q { items: list<&T> }");
        let q = doc.type_decl("Q").unwrap();
        let resolved = doc.resolve(q.field("items").unwrap().type_ref());
        let ResolvedType::List(inner) = resolved else {
            panic!("expected List");
        };
        let ResolvedType::Reference(inner2) = *inner else {
            panic!("expected Reference inner");
        };
        let ResolvedType::Named(d) = *inner2 else {
            panic!("expected Named");
        };
        assert_eq!(d.name(), "T");
    }

    #[test]
    fn symbol_sets_are_queryable() {
        let doc = open(
            r#"
            symbol_set Color { red green blue }
            symbol_set Mood { warm cool }
            "#,
        );
        assert_eq!(doc.symbol_sets().count(), 2);
        let color = doc.symbol_set("Color").expect("Color");
        assert_eq!(color.symbols().count(), 3);
        assert!(color.has("red"));
        assert!(!color.has("missing"));
    }

    #[test]
    fn resolve_named_symbol_set() {
        let doc = open("symbol_set Color { red green }\ntype Q { f: Color }");
        let q = doc.type_decl("Q").unwrap();
        let f = q.field("f").unwrap();
        match doc.resolve(f.type_ref()) {
            ResolvedType::SymbolSet(s) => assert_eq!(s.name(), "Color"),
            _ => panic!("expected SymbolSet"),
        }
    }

    #[test]
    fn duplicate_symbol_in_set_errors() {
        let err = Document::open("symbol_set X { a a }", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("duplicate symbol 'a' in symbol_set 'X'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn symbol_set_collides_with_type_name() {
        let err = Document::open("type Foo {}\nsymbol_set Foo {}", "test").unwrap_err();
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("duplicate declaration 'Foo'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn symbol_value_evaluates() {
        let doc = open("tag = :wide");
        assert_eq!(
            doc.field("tag").unwrap().value().unwrap(),
            &Value::Symbol("wide".into())
        );
    }

    #[test]
    fn symbol_sets_dont_appear_in_field_or_block_iteration() {
        let doc = open(
            r#"
            symbol_set Color { red green }
            count = 1
            svc {}
            "#,
        );
        let field_names: Vec<_> = doc.fields().map(|f| f.name().to_string()).collect();
        assert_eq!(field_names, vec!["count".to_string()]);
        let block_kinds: Vec<_> = doc.blocks().map(|b| b.kind().to_string()).collect();
        assert_eq!(block_kinds, vec!["svc".to_string()]);
    }

    #[test]
    fn cycle_error_renders() {
        let err = EvalError::Cycle {
            field: "x".into(),
            span: SourceSpan::new(0.into(), 1),
        };
        let s = format!("{}", err);
        assert!(s.contains("cycle"));
    }
}
