//! The **cell**: one field value as the editor's forms see it.
//!
//! A cell is a fact about a *value*, the way a [`super::blocks::kind_entry`]
//! field is a fact about a *schema*. They meet in the client's form — a
//! field's declared type picks the control, the cell fills it — but share no
//! logic, which is why this is its own module: a schema consumer must not
//! have to drag expression classification along with it.
//!
//! Every endpoint that serves a block's values ([`super::blocks`]'s
//! `/api/block/source`, [`super::systems`]'s nodes and detail,
//! [`super::data`]'s rows) answers with the SAME two containers — a
//! positional array of label cells and a map of named field cells (see
//! [`block_cells`]) — so a control written against one endpoint works
//! against all of them.
//!
//! ## The states
//!
//! ```text
//! text | identifier | symbol | bool | number | list | rows | computed
//! ```
//!
//! - `text` — a plain string literal. The only state written back as a
//!   string; every other state round-trips as parsed WCL, which is why
//!   there is no separate "write it as an expression" flag to disagree with
//!   the state.
//! - `identifier` — a bare name (`repo = wcl_repo`), editable as such.
//! - `symbol` — the **bare** member name, without the leading colon: the
//!   colon is syntax, and a picker's options are names.
//! - `bool` / `number` — scalar literals; `number` covers every numeric
//!   form the language has, including type-suffixed (`5u8`) and
//!   unit-suffixed (`5MiB`) literals.
//! - `list` — a list literal whose elements are all cells themselves
//!   (`items`), `rows` — a list of such lists (`rows`). Both carry
//!   classified cells, never bare strings.
//! - `computed` — an interpolation, a call, a reference: no form control
//!   can express it, and the client says so rather than rendering a dead
//!   input.
//!
//! Adding a state means adding a variant here and handling it in the
//! client's control mapping — the two documented places.

use wcl_lang::ast::{self, Expr, Item};

/// A field value as a form sees it — see the module docs for the states.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Cell {
    Text(String),
    Identifier(String),
    Symbol(String),
    Bool(bool),
    /// The literal's source form (`12`, `1.5`, `5MiB`), so it round-trips.
    Number(String),
    List(Vec<Cell>),
    Rows(Vec<Vec<Cell>>),
    Computed,
}

impl Cell {
    /// The state name on the wire.
    pub(super) fn state(&self) -> &'static str {
        match self {
            Cell::Text(_) => "text",
            Cell::Identifier(_) => "identifier",
            Cell::Symbol(_) => "symbol",
            Cell::Bool(_) => "bool",
            Cell::Number(_) => "number",
            Cell::List(_) => "list",
            Cell::Rows(_) => "rows",
            Cell::Computed => "computed",
        }
    }

    /// The scalar text a form control edits — `None` for the container and
    /// computed states, which have no single value.
    pub(super) fn text(&self) -> Option<String> {
        match self {
            Cell::Text(s) | Cell::Identifier(s) | Cell::Symbol(s) | Cell::Number(s) => {
                Some(s.clone())
            }
            Cell::Bool(b) => Some(b.to_string()),
            Cell::List(_) | Cell::Rows(_) | Cell::Computed => None,
        }
    }

    /// The wire form: always `state` + `text` (null where there is none),
    /// plus `items` / `rows` for the container states.
    pub(super) fn json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({ "state": self.state(), "text": self.text() });
        match self {
            Cell::List(items) => {
                v["items"] = serde_json::Value::Array(items.iter().map(Cell::json).collect());
            }
            Cell::Rows(rows) => {
                v["rows"] = serde_json::Value::Array(
                    rows.iter()
                        .map(|r| serde_json::Value::Array(r.iter().map(Cell::json).collect()))
                        .collect(),
                );
            }
            _ => {}
        }
        v
    }
}

/// Classify one expression. The single place a newly supported expression
/// form has to be taught about.
pub(super) fn classify(e: &Expr) -> Cell {
    match e {
        Expr::Utf8(s) | Expr::Ascii(s) => Cell::Text(s.clone()),
        Expr::Identifier(s, _) => Cell::Identifier(s.clone()),
        // Bare on purpose: `:` is syntax, the picker offers names.
        Expr::Symbol(s) => Cell::Symbol(s.clone()),
        Expr::Bool(b) => Cell::Bool(*b),
        Expr::ListLit { elements, .. } => classify_list(elements),
        _ => match number_text(e) {
            Some(text) => Cell::Number(text),
            None => Cell::Computed,
        },
    }
}

/// A list literal: `list` when every element is a cell in its own right,
/// `rows` when every element is such a list, `computed` otherwise. An
/// empty literal is an empty `list`.
fn classify_list(elements: &[Expr]) -> Cell {
    let cells: Vec<Cell> = elements.iter().map(classify).collect();
    let scalar = |c: &Cell| !matches!(c, Cell::Computed | Cell::List(_) | Cell::Rows(_));
    if cells.iter().all(scalar) {
        return Cell::List(cells);
    }
    let rows: Option<Vec<Vec<Cell>>> = cells
        .into_iter()
        .map(|c| match c {
            Cell::List(items) => Some(items),
            _ => None,
        })
        .collect();
    match rows {
        Some(rows) => Cell::Rows(rows),
        None => Cell::Computed,
    }
}

/// The source text of a numeric literal, for both enums that hold one:
/// `Expr`'s numeric variants and the [`wcl_lang::NumberLit`] a unit literal
/// carries. The two use the same variant names, so one body serves both
/// (the idiom `wcl_lang`'s `numeric_as_u64!` already follows) and a new
/// numeric type is added to one list. Callers append their own tail arms.
macro_rules! numeric_source_text {
    ($val:expr, $ty:ident, $($tail:tt)*) => {
        match $val {
            $ty::I8(n) => n.to_string(),
            $ty::I16(n) => n.to_string(),
            $ty::I32(n) => n.to_string(),
            $ty::I64(n) => n.to_string(),
            $ty::I128(n) => n.to_string(),
            $ty::Isize(n) => n.to_string(),
            $ty::U8(n) => n.to_string(),
            $ty::U16(n) => n.to_string(),
            $ty::U32(n) => n.to_string(),
            $ty::U64(n) => n.to_string(),
            $ty::U128(n) => n.to_string(),
            $ty::Usize(n) => n.to_string(),
            $ty::F32(n) => n.to_string(),
            $ty::F64(n) => n.to_string(),
            $($tail)*
        }
    };
}

/// The source form of any numeric literal, including the type-suffixed
/// (`5u8`) and unit-suffixed (`5MiB`) forms — `None` when the expression
/// isn't a number at all.
///
/// The tail arms are inside a macro invocation, so rustfmt leaves them
/// alone: keep them hand-wrapped to match the rest of the file.
fn number_text(e: &Expr) -> Option<String> {
    Some(numeric_source_text!(e, Expr,
        Expr::UnitLiteral { value, unit, .. } => {
            format!("{}{unit}", number_lit_text(value))
        }
        // The parser keeps `-7` as a negation over a literal; a form must
        // still see one editable number.
        Expr::Unary {
            op: ast::UnaryOp::Neg,
            operand,
            ..
        } => format!("-{}", number_text(operand)?),
        _ => return None,
    ))
}

fn number_lit_text(n: &wcl_lang::NumberLit) -> String {
    use wcl_lang::NumberLit as N;
    numeric_source_text!(n, N,)
}

/// A block's inline labels, positionally.
pub(super) fn labels_json(blk: &ast::Block) -> Vec<serde_json::Value> {
    blk.labels.iter().map(|e| classify(e).json()).collect()
}

/// A block's named fields.
pub(super) fn fields_json(blk: &ast::Block) -> serde_json::Map<String, serde_json::Value> {
    blk.items
        .iter()
        .filter_map(|it| match it {
            Item::Field(f) => Some((f.name.clone(), classify(&f.expr).json())),
            _ => None,
        })
        .collect()
}

/// The one container encoding: `{ labels: [cell], fields: { name: cell } }`.
/// Labels are positional, fields are named — no synthetic slot keys to build
/// and parse back apart.
pub(super) fn block_cells(blk: &ast::Block) -> serde_json::Value {
    serde_json::json!({ "labels": labels_json(blk), "fields": fields_json(blk) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_lang::parse_for_edit;

    /// Classify the fields (and labels) of `thing … { … }` in a source
    /// string — the in-memory seam, no disk and no server.
    fn cells_of(src: &str) -> serde_json::Value {
        let parsed = parse_for_edit(src, "test.wcl".to_string()).expect("parses");
        let blk = parsed
            .items
            .iter()
            .find_map(|it| match it {
                Item::Block(b) => Some(b),
                _ => None,
            })
            .expect("a block");
        block_cells(blk)
    }

    fn state(v: &serde_json::Value, field: &str) -> String {
        v["fields"][field]["state"].as_str().unwrap().to_string()
    }
    fn text(v: &serde_json::Value, field: &str) -> String {
        v["fields"][field]["text"].as_str().unwrap().to_string()
    }

    #[test]
    fn scalar_states_split_by_type() {
        let v = cells_of(
            r#"thing t {
              name = "Hero"
              repo = wcl_repo
              kind = :module
              done = true
              hp = 10
              ratio = 1.5
            }"#,
        );
        assert_eq!(state(&v, "name"), "text");
        assert_eq!(text(&v, "name"), "Hero");
        assert_eq!(state(&v, "repo"), "identifier");
        assert_eq!(text(&v, "repo"), "wcl_repo");
        assert_eq!(state(&v, "kind"), "symbol");
        // Bare: the colon is syntax, not part of the value.
        assert_eq!(text(&v, "kind"), "module");
        assert_eq!(state(&v, "done"), "bool");
        assert_eq!(text(&v, "done"), "true");
        assert_eq!(state(&v, "hp"), "number");
        assert_eq!(text(&v, "hp"), "10");
        assert_eq!(state(&v, "ratio"), "number");
    }

    /// Every numeric form is a number — the type-suffixed and unit-suffixed
    /// literals used to fall through to `computed`, leaving a silently
    /// read-only control.
    #[test]
    fn every_numeric_form_is_a_number() {
        let v = cells_of(
            r#"thing t {
              a = 1u8
              b = 2i16
              c = 3.5f32
              d = 4usize
              e = -7
            }"#,
        );
        for f in ["a", "b", "c", "d", "e"] {
            assert_eq!(state(&v, f), "number", "field {f}");
        }
        assert_eq!(text(&v, "a"), "1");
        assert_eq!(text(&v, "b"), "2");
        assert_eq!(text(&v, "d"), "4");
    }

    #[test]
    fn unit_suffixed_literals_keep_their_suffix() {
        let v = cells_of("thing t { size = 5MiB }");
        assert_eq!(state(&v, "size"), "number");
        assert_eq!(text(&v, "size"), "5MiB");
    }

    /// The colon is syntax: it is in the SOURCE and never in the cell, so a
    /// picker's options are names. The bare form of the same word is an
    /// identifier — the two must not collapse into one state.
    #[test]
    fn symbols_drop_the_colon_identifiers_keep_the_name() {
        let v = cells_of(
            r#"thing t {
              kind = :module
              same = module
              tags = [:one, :two]
            }"#,
        );
        assert_eq!(state(&v, "kind"), "symbol");
        assert_eq!(text(&v, "kind"), "module");
        assert_eq!(state(&v, "same"), "identifier");
        assert_eq!(text(&v, "same"), "module");
        // A list of symbols carries bare names too, not `:one`.
        assert_eq!(v["fields"]["tags"]["items"][0]["state"], "symbol");
        assert_eq!(v["fields"]["tags"]["items"][0]["text"], "one");
        assert_eq!(v["fields"]["tags"]["items"][1]["text"], "two");
    }

    #[test]
    fn labels_are_positional() {
        let v = cells_of(r#"thing t "Title" { name = "n" }"#);
        assert_eq!(v["labels"][0]["state"], "identifier");
        assert_eq!(v["labels"][0]["text"], "t");
        assert_eq!(v["labels"][1]["state"], "text");
        assert_eq!(v["labels"][1]["text"], "Title");
        // No synthetic `@0` key leaks into the named fields.
        assert!(v["fields"]["@0"].is_null());
    }

    #[test]
    fn lists_carry_classified_cells() {
        let v = cells_of(
            r#"thing t {
              tags = ["a", "b"]
              repos = [one, two]
              sizes = [1, 2.5]
              empty = []
            }"#,
        );
        assert_eq!(state(&v, "tags"), "list");
        assert_eq!(v["fields"]["tags"]["items"][1]["state"], "text");
        assert_eq!(v["fields"]["tags"]["items"][1]["text"], "b");
        assert_eq!(state(&v, "repos"), "list");
        assert_eq!(v["fields"]["repos"]["items"][0]["state"], "identifier");
        assert_eq!(state(&v, "sizes"), "list");
        assert_eq!(v["fields"]["sizes"]["items"][0]["state"], "number");
        assert_eq!(state(&v, "empty"), "list");
        assert_eq!(v["fields"]["empty"]["items"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn nested_lists_are_rows() {
        let v = cells_of(r#"table t { rows = [["a", "b"], ["c", "d"]] }"#);
        assert_eq!(state(&v, "rows"), "rows");
        assert_eq!(v["fields"]["rows"]["rows"][1][0]["text"], "c");
        assert!(v["fields"]["rows"]["text"].is_null());
    }

    #[test]
    fn expressions_stay_computed() {
        let v = cells_of(
            r#"thing t {
              greeting = $"hi ${name}"
              total = 1 + 2
              from_call = upper("x")
              linked = other.name
              mixed = ["a", 1 + 2]
              nothing = none
            }"#,
        );
        for f in [
            "greeting",
            "total",
            "from_call",
            "linked",
            "mixed",
            "nothing",
        ] {
            assert_eq!(state(&v, f), "computed", "field {f}");
            assert!(v["fields"][f]["text"].is_null(), "field {f}");
        }
    }
}
