//! The semantic content IR — the target-neutral document vocabulary.
//!
//! [`Content`] is a closed union: one variant per document concept, no
//! generic container, and no escape into markup. Every variant, record and
//! symbol vocabulary in this module is **generated** by the build script
//! from `lib/content.wcl`, which is the source of truth — edit the WCL
//! declaration, not the Rust. What lives here by hand is the reading half:
//! the error type and the [`Value`] readers the generated conversions are
//! written in terms of.
//!
//! The conversion is `TryFrom<&Value>` (and `TryFrom<Value>`) on every
//! generated type, so a lowered WCL value becomes a typed node — or a
//! precise [`ContentError`] naming the variant and field that failed.
//!
//! Nothing lowers to this IR yet: the blocks still lower to
//! `HtmlFundamental`, and the four backends still walk that. This module is
//! the declaration those backends will be moved onto.

use wcl_lang::Value;

/// Where a conversion failed: the owning type (`Content::Heading`,
/// `ContentTocEntry`, …) and the field being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    /// The variant, record or symbol set the field belongs to.
    pub owner: &'static str,
    /// The field name as declared in WCL.
    pub field: &'static str,
}

/// Why a [`Value`] could not be read as a content node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentError {
    /// The value wasn't a record-payload variant of the expected union.
    NotAVariant {
        /// The union that was expected.
        owner: &'static str,
    },
    /// The value wasn't a record, so its fields can't be read by name.
    NotARecord {
        /// The record type that was expected.
        owner: &'static str,
    },
    /// The value wasn't a symbol.
    NotASymbol {
        /// The symbol set that was expected.
        owner: &'static str,
    },
    /// The union declares no such variant.
    UnknownVariant {
        /// The union that was read.
        owner: &'static str,
        /// The variant name carried by the value.
        variant: String,
    },
    /// The symbol set declares no such member.
    UnknownSymbol {
        /// The symbol set that was read.
        owner: &'static str,
        /// The symbol carried by the value.
        symbol: String,
    },
    /// A required field was absent (or `none`).
    MissingField {
        /// The field that was missing.
        at: At,
    },
    /// A field held a value of the wrong shape for its declared type.
    FieldType {
        /// The field that was read.
        at: At,
        /// The declared type the reader wanted.
        expected: &'static str,
    },
}

impl std::fmt::Display for ContentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentError::NotAVariant { owner } => {
                write!(f, "expected a `{owner}` variant value")
            }
            ContentError::NotARecord { owner } => write!(f, "expected a `{owner}` record value"),
            ContentError::NotASymbol { owner } => write!(f, "expected a `{owner}` symbol"),
            ContentError::UnknownVariant { owner, variant } => {
                write!(f, "`{owner}` declares no variant `{variant}`")
            }
            ContentError::UnknownSymbol { owner, symbol } => {
                write!(f, "`{owner}` declares no symbol `:{symbol}`")
            }
            ContentError::MissingField { at } => {
                write!(f, "`{}` is missing required field `{}`", at.owner, at.field)
            }
            ContentError::FieldType { at, expected } => write!(
                f,
                "`{}`'s field `{}` is not a {expected}",
                at.owner, at.field
            ),
        }
    }
}

impl std::error::Error for ContentError {}

/// The [`Value`] readers the generated conversions are written in terms
/// of. Which ones a build uses follows from the declaration: the emitter
/// pairs every WCL builtin with the reader that carries it (its
/// `Crossing` table), so a reader the current union happens not to reach
/// is the one the next field to declare that builtin needs. Those carry
/// their own `dead_code` waiver; a reader nothing at all pairs with is a
/// genuine leftover and still warns.
mod read {
    use std::collections::BTreeMap;

    use wcl_lang::{Value, VariantPayload};

    use super::{At, ContentError};

    /// The variant name and payload map of a union value, checked to
    /// belong to `owner` — so handing a `SvgFundamental` to `Content`
    /// is an error rather than a silent miss.
    pub(super) fn variant_payload<'a>(
        value: &'a Value,
        owner: &'static str,
    ) -> Result<(&'a str, &'a BTreeMap<String, Value>), ContentError> {
        let Value::Variant {
            union,
            variant,
            payload: VariantPayload::Record(map),
        } = value
        else {
            return Err(ContentError::NotAVariant { owner });
        };
        if union.last().map(String::as_str) != Some(owner) {
            return Err(ContentError::NotAVariant { owner });
        }
        Ok((variant.as_str(), map))
    }

    /// The field map of a record value.
    pub(super) fn record_fields<'a>(
        value: &'a Value,
        owner: &'static str,
    ) -> Result<&'a BTreeMap<String, Value>, ContentError> {
        match value {
            Value::Record { fields, .. } => Ok(fields),
            _ => Err(ContentError::NotARecord { owner }),
        }
    }

    /// The bare name of a symbol value (the `:` is syntax).
    pub(super) fn symbol_name<'a>(
        value: &'a Value,
        owner: &'static str,
    ) -> Result<&'a str, ContentError> {
        match value {
            Value::Symbol(s) => Ok(s.as_str()),
            _ => Err(ContentError::NotASymbol { owner }),
        }
    }

    /// A field that carries a value: absent and explicit `none` are the
    /// same thing to a reader.
    pub(super) fn present<'a>(map: &'a BTreeMap<String, Value>, name: &str) -> Option<&'a Value> {
        match map.get(name) {
            Some(Value::None) | None => None,
            Some(v) => Some(v),
        }
    }

    /// A required field's value.
    pub(super) fn required(map: &BTreeMap<String, Value>, at: At) -> Result<&Value, ContentError> {
        present(map, at.field).ok_or(ContentError::MissingField { at })
    }

    pub(super) fn as_string(value: &Value, at: At) -> Result<String, ContentError> {
        match value {
            Value::Utf8(s) | Value::Ascii(s) => Ok(s.clone()),
            _ => Err(ContentError::FieldType {
                at,
                expected: "utf8",
            }),
        }
    }

    /// An identifier slot: a quoted string coerces to the identifier it
    /// names, exactly as it does in the language.
    pub(super) fn as_identifier(value: &Value, at: At) -> Result<String, ContentError> {
        match value {
            Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => Ok(s.clone()),
            _ => Err(ContentError::FieldType {
                at,
                expected: "identifier",
            }),
        }
    }

    /// The reader for a bare `symbol` field. Today's union types its one
    /// symbol field as a declared vocabulary (`CalloutKind`) instead, so
    /// nothing reaches this — the `Crossing` table still pairs `symbol`
    /// with it.
    #[allow(dead_code)]
    pub(super) fn as_symbol(value: &Value, at: At) -> Result<String, ContentError> {
        match value {
            Value::Symbol(s) => Ok(s.clone()),
            _ => Err(ContentError::FieldType {
                at,
                expected: "symbol",
            }),
        }
    }

    pub(super) fn as_bool(value: &Value, at: At) -> Result<bool, ContentError> {
        match value {
            Value::Bool(b) => Ok(*b),
            _ => Err(ContentError::FieldType {
                at,
                expected: "bool",
            }),
        }
    }

    pub(super) fn as_f64(value: &Value, at: At) -> Result<f64, ContentError> {
        float(value).ok_or(ContentError::FieldType {
            at,
            expected: "f64",
        })
    }

    /// The reader for an `f32` field. The IR's measurements are all
    /// `f64`; the `Crossing` table still pairs `f32` with it.
    #[allow(dead_code)]
    pub(super) fn as_f32(value: &Value, at: At) -> Result<f32, ContentError> {
        float(value)
            .map(|f| f as f32)
            .ok_or(ContentError::FieldType {
                at,
                expected: "f32",
            })
    }

    /// An integer field, range-checked into its declared width — a
    /// `level: u8` that arrives as `9000` is a type error, not a wrap.
    pub(super) fn as_int<T: TryFrom<i128>>(value: &Value, at: At) -> Result<T, ContentError> {
        integer(value)
            .and_then(|i| T::try_from(i).ok())
            .ok_or(ContentError::FieldType {
                at,
                expected: std::any::type_name::<T>(),
            })
    }

    /// A list field, each element read by `f`.
    pub(super) fn as_seq<T>(
        value: &Value,
        at: At,
        mut f: impl FnMut(&Value) -> Result<T, ContentError>,
    ) -> Result<Vec<T>, ContentError> {
        let Value::List(items) = value else {
            return Err(ContentError::FieldType {
                at,
                expected: "list",
            });
        };
        items.iter().map(&mut f).collect()
    }

    /// Any numeric value as `f64`. WCL promotes numerically, so an
    /// integer literal in a float slot is the author writing `10`, not a
    /// mistake.
    fn float(value: &Value) -> Option<f64> {
        match value {
            Value::F64(f) => Some(*f),
            Value::F32(f) => Some(f64::from(*f)),
            other => integer(other).map(|i| i as f64),
        }
    }

    /// Any integer value, widened for range-checking.
    fn integer(value: &Value) -> Option<i128> {
        Some(match value {
            Value::I8(i) => i128::from(*i),
            Value::I16(i) => i128::from(*i),
            Value::I32(i) => i128::from(*i),
            Value::I64(i) => i128::from(*i),
            Value::I128(i) => *i,
            Value::Isize(i) => *i as i128,
            Value::U8(u) => i128::from(*u),
            Value::U16(u) => i128::from(*u),
            Value::U32(u) => i128::from(*u),
            Value::U64(u) => i128::from(*u),
            Value::U128(u) => i128::try_from(*u).ok()?,
            Value::Usize(u) => *u as i128,
            _ => return None,
        })
    }
}

use read::*;

include!(concat!(env!("OUT_DIR"), "/content_ir.rs"));

/// The emitter that produced the `include!`d module, compiled a second
/// time so its own refusals can be tested: a build script is not reached
/// by `cargo test`, and its panics are the closedness guarantee.
#[cfg(test)]
#[path = "../build/content_ir.rs"]
mod emitter;

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use wcl_lang::ast::{Item, VariantBody};
    use wcl_lang::{Document, Environment, VariantPayload, disk_loader};

    /// The declaration the Rust in this module is generated from.
    const CONTENT_WCL: &str = include_str!("../lib/content.wcl");

    /// Open a fixture through the embedded wdoc registry (so `Content`
    /// resolves exactly as it does in a real document) and evaluate one
    /// top-level field.
    fn eval(field: &str, extra: &str) -> Value {
        let src = format!(
            "import <wdoc.wcl>\n\
             @document\n\
             type Probe {{ node: Content nodes: list<Content> }}\n\
             {extra}\n"
        );
        let loader = crate::schema_registry().loader(disk_loader());
        let doc = Document::open_at_with_loader(
            &src,
            "content-test.wcl",
            None,
            &Environment::new(),
            loader,
        )
        .expect("open content fixture");
        doc.field(field)
            .unwrap_or_else(|| panic!("no field `{field}`"))
            .value()
            .expect("evaluate field")
            .clone()
    }

    /// The variants of `union Content`, read off the declaration itself.
    fn declared_variants() -> Vec<(String, VariantBody)> {
        let source =
            wcl_lang::parse_for_edit(CONTENT_WCL, "content.wcl").expect("parse content.wcl");
        source
            .items
            .into_iter()
            .find_map(|item| match item {
                Item::UnionDecl(u) if u.name.last().map(String::as_str) == Some("Content") => {
                    Some(u.variants)
                }
                _ => None,
            })
            .expect("union Content is declared")
            .into_iter()
            .map(|v| (v.name, v.body))
            .collect()
    }

    #[test]
    fn declares_one_variant_per_document_concept() {
        let count = declared_variants().len();
        assert!(
            (15..=20).contains(&count),
            "the content IR should carry ~15-20 concepts, found {count}"
        );
    }

    #[test]
    fn the_ir_is_closed() {
        // No markup escape hatch and no generic container: a concept
        // that isn't declared here isn't page content.
        let banned = ["Html", "Raw", "Element", "Node", "Container", "Block"];
        for (name, body) in declared_variants() {
            assert!(
                !banned.contains(&name.as_str()),
                "`Content::{name}` reopens the markup / generic-container door"
            );
            assert!(
                matches!(body, VariantBody::Record { .. }),
                "`Content::{name}` must be record-shaped so backends read it by field name"
            );
        }
    }

    #[test]
    fn reads_a_heading_with_its_level_as_a_number() {
        let value = eval(
            "node",
            r#"node = Content::Heading { level: 2, text: "Hello" }"#,
        );
        assert_eq!(
            Content::try_from(&value),
            Ok(Content::Heading {
                level: 2,
                text: "Hello".to_string(),
                id: None,
                class: None,
            })
        );
    }

    #[test]
    fn absent_optional_fields_read_as_none() {
        let value = eval("node", r#"node = Content::Paragraph { text: "hi" }"#);
        let Ok(Content::Paragraph { id, class, .. }) = Content::try_from(&value) else {
            panic!("expected a paragraph");
        };
        assert_eq!((id, class), (None, None));
    }

    #[test]
    fn reads_nested_records_lists_and_symbol_sets() {
        let value = eval(
            "nodes",
            r#"nodes = [
                 Content::List {
                   style: :numbered,
                   items: [
                     { text: "one" },
                     { text: "two", blocks: [Content::Paragraph { text: "nested" }] },
                   ],
                 },
               ]"#,
        );
        let Value::List(items) = &value else {
            panic!("expected a list");
        };
        let Ok(Content::List { items, style, .. }) = Content::try_from(&items[0]) else {
            panic!("expected a list node");
        };
        assert_eq!(style, Some(ListStyle::Numbered));
        assert_eq!(items[0].text, "one");
        assert_eq!(items[0].blocks, None);
        assert_eq!(
            items[1].blocks,
            Some(vec![Content::Paragraph {
                text: "nested".to_string(),
                id: None,
                class: None,
            }])
        );
    }

    #[test]
    fn a_drawing_carries_shapes_not_markup() {
        let value = eval(
            "node",
            r#"node = Content::Drawing {
                 shapes: [SvgFundamental::Rect { x: 0.0, y: 1.0, width: 10.0, height: 4.0 }],
               }"#,
        );
        let Ok(Content::Drawing { shapes, .. }) = Content::try_from(&value) else {
            panic!("expected a drawing");
        };
        assert_eq!(
            shapes,
            vec![SvgFundamental::Rect {
                x: Some(0.0),
                y: Some(1.0),
                width: Some(10.0),
                height: Some(4.0),
                rx: None,
                fill: None,
                stroke: None,
                id: None,
                class: None,
            }]
        );
    }

    #[test]
    fn a_callout_reads_its_kind_from_the_declared_vocabulary() {
        let value = eval(
            "node",
            r#"node = Content::Callout {
                 kind: :warning, heading: "Careful",
                 body: [Content::Paragraph { text: "mind the gap" }],
               }"#,
        );
        let Ok(Content::Callout { kind, body, .. }) = Content::try_from(&value) else {
            panic!("expected a callout");
        };
        assert_eq!(kind, Some(CalloutKind::Warning));
        assert_eq!(kind.map(CalloutKind::as_wcl), Some("warning"));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn a_value_from_another_union_is_refused() {
        // A markup fundamental is not content: the union tag is checked,
        // so a `Raw` can't be read as a content node by shape alone.
        let foreign = Value::Variant {
            union: vec!["wdoc".to_string(), "HtmlFundamental".to_string()],
            variant: "Raw".to_string(),
            payload: VariantPayload::Record(Arc::new(BTreeMap::from([(
                "html".to_string(),
                Value::Utf8("<b>no</b>".to_string()),
            )]))),
        };
        assert_eq!(
            Content::try_from(&foreign),
            Err(ContentError::NotAVariant { owner: "Content" })
        );
    }

    #[test]
    fn a_missing_required_field_names_the_field() {
        let headless = Value::Variant {
            union: vec!["wdoc".to_string(), "Content".to_string()],
            variant: "Heading".to_string(),
            payload: VariantPayload::Record(Arc::new(BTreeMap::from([(
                "level".to_string(),
                Value::U8(2),
            )]))),
        };
        assert_eq!(
            Content::try_from(&headless),
            Err(ContentError::MissingField {
                at: At {
                    owner: "Content::Heading",
                    field: "text",
                },
            })
        );
    }

    #[test]
    fn an_out_of_range_number_is_a_type_error_not_a_wrap() {
        let huge = Value::Variant {
            union: vec!["wdoc".to_string(), "Content".to_string()],
            variant: "Heading".to_string(),
            payload: VariantPayload::Record(Arc::new(BTreeMap::from([
                ("level".to_string(), Value::I64(9000)),
                ("text".to_string(), Value::Utf8("Hello".to_string())),
            ]))),
        };
        assert!(matches!(
            Content::try_from(&huge),
            Err(ContentError::FieldType {
                at: At {
                    owner: "Content::Heading",
                    field: "level"
                },
                ..
            })
        ));
    }

    // ── The emitter ───────────────────────────────────────────────

    /// A one-file synthetic stdlib for the emitter's refusals.
    fn sources(wcl: &str) -> Vec<(String, String)> {
        vec![("synthetic.wcl".to_string(), wcl.to_string())]
    }

    #[test]
    fn the_included_module_is_what_the_emitter_produces_from_the_stdlib() {
        let lib = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lib");
        assert_eq!(
            super::emitter::generate(&lib),
            include_str!(concat!(env!("OUT_DIR"), "/content_ir.rs"))
        );
    }

    #[test]
    fn a_field_type_that_cannot_cross_a_backend_boundary_is_refused() {
        let err = std::panic::catch_unwind(|| {
            super::emitter::generate_from(&sources(
                "namespace wdoc\nunion Content { Odd { draw: fn(utf8) -> utf8 } }\n",
            ))
        })
        .expect_err("a fn-typed field must fail the build");
        let msg = err.downcast_ref::<String>().expect("panic message");
        assert!(msg.contains("cannot cross the backend boundary"), "{msg}");
    }

    #[test]
    fn a_variant_the_backends_could_not_read_by_name_is_refused() {
        // A positional variant has no field names in its payload, so a
        // backend has nothing to read it by.
        let err = std::panic::catch_unwind(|| {
            super::emitter::generate_from(&sources(
                "namespace wdoc\n\
                 type Note { text: utf8 }\n\
                 union Content { Aside Note }\n",
            ))
        })
        .expect_err("a positional variant must fail the build");
        let msg = err.downcast_ref::<String>().expect("panic message");
        assert!(msg.contains("must be a record variant"), "{msg}");
    }

    #[test]
    fn an_ambiguously_declared_type_is_refused() {
        // The parser catches a name declared twice in ONE file; two
        // files are how the stdlib actually does it (`Image` is both a
        // page block and a typedoc one), and only the emitter sees that.
        let err = std::panic::catch_unwind(|| {
            super::emitter::generate_from(&[
                (
                    "one.wcl".to_string(),
                    "namespace wdoc\ntype Note { text: utf8 }\n".to_string(),
                ),
                (
                    "two.wcl".to_string(),
                    "namespace wdoc\n\
                     type Note { body: utf8 }\n\
                     union Content { Aside { note: Note } }\n"
                        .to_string(),
                ),
            ])
        })
        .expect_err("two declarations of a reached name must fail the build");
        let msg = err.downcast_ref::<String>().expect("panic message");
        assert!(msg.contains("more than once"), "{msg}");
    }

    #[test]
    fn a_parameterised_type_is_refused() {
        // Type arguments are syntax only — nothing substitutes them, so
        // generating `Note` and dropping the `<utf8>` would be a lie.
        let err = std::panic::catch_unwind(|| {
            super::emitter::generate_from(&sources(
                "namespace wdoc\n\
                 type Note { text: utf8 }\n\
                 union Content { Aside { note: Note<utf8> } }\n",
            ))
        })
        .expect_err("a parameterised field type must fail the build");
        let msg = err.downcast_ref::<String>().expect("panic message");
        assert!(msg.contains("type arguments"), "{msg}");
    }

    #[test]
    fn a_field_named_after_a_rust_keyword_is_escaped() {
        let rust = super::emitter::generate_from(&sources(
            "namespace wdoc\nunion Content { Odd { type: utf8 } }\n",
        ));
        assert!(rust.contains("r#type: String,"), "{rust}");
    }

    #[test]
    fn an_undeclared_variant_is_refused() {
        let invented = Value::Variant {
            union: vec!["wdoc".to_string(), "Content".to_string()],
            variant: "Marquee".to_string(),
            payload: VariantPayload::Record(Arc::new(BTreeMap::new())),
        };
        assert_eq!(
            Content::try_from(&invented),
            Err(ContentError::UnknownVariant {
                owner: "Content",
                variant: "Marquee".to_string(),
            })
        );
    }
}
