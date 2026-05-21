use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use miette::{NamedSource, SourceSpan};

use crate::ast::{self, Span};
use crate::error::{EvalError, ParseError};
use crate::parser::Parser;
use crate::value::Value;

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
}

impl Document {
    pub fn open(source: &str, name: &str) -> Result<Self, ParseError> {
        let ast = Parser::new(source, name).parse_source()?;
        let cells = BlockCells::build(&ast.items);
        Ok(Self {
            src: NamedSource::new(name, source.to_string()),
            ast,
            cells,
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
    })
}

fn span_to_miette(span: Span) -> SourceSpan {
    SourceSpan::new(span.start.into(), span.len().max(1))
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
    fn cycle_error_renders() {
        let err = EvalError::Cycle {
            field: "x".into(),
            span: SourceSpan::new(0.into(), 1),
        };
        let s = format!("{}", err);
        assert!(s.contains("cycle"));
    }
}
