use crate::eval::value::Value;
use crate::lang::ast::{StringPart, TypeExpr};

/// Check if a Value matches a TypeExpr
pub fn check_type(value: &Value, type_expr: &TypeExpr) -> bool {
    match (value, type_expr) {
        (_, TypeExpr::Any(_)) => true,
        (Value::String(_), TypeExpr::String(_)) => true,
        // Signed integer types — range-checked against i64 value
        (Value::Int(v), TypeExpr::I8(_)) => *v >= i8::MIN as i64 && *v <= i8::MAX as i64,
        (Value::Int(v), TypeExpr::I16(_)) => *v >= i16::MIN as i64 && *v <= i16::MAX as i64,
        (Value::Int(v), TypeExpr::I32(_)) => *v >= i32::MIN as i64 && *v <= i32::MAX as i64,
        (Value::Int(_), TypeExpr::I64(_)) => true,
        (Value::Int(_), TypeExpr::I128(_)) => true,
        // Unsigned integer types — range-checked against i64 value
        (Value::Int(v), TypeExpr::U8(_)) => *v >= 0 && *v <= u8::MAX as i64,
        (Value::Int(v), TypeExpr::U16(_)) => *v >= 0 && *v <= u16::MAX as i64,
        (Value::Int(v), TypeExpr::U32(_)) => *v >= 0 && *v <= u32::MAX as i64,
        (Value::Int(v), TypeExpr::U64(_)) => *v >= 0,
        (Value::Int(v), TypeExpr::U128(_)) => *v >= 0,
        // BigInt range checks
        (Value::BigInt(_), TypeExpr::I128(_)) => true,
        (Value::BigInt(v), TypeExpr::U128(_)) => *v >= 0,
        (Value::BigInt(v), TypeExpr::U64(_)) => *v >= 0 && *v <= u64::MAX as i128,
        (Value::BigInt(v), TypeExpr::I64(_)) => *v >= i64::MIN as i128 && *v <= i64::MAX as i128,
        (Value::BigInt(v), TypeExpr::I8(_)) => *v >= i8::MIN as i128 && *v <= i8::MAX as i128,
        (Value::BigInt(v), TypeExpr::U8(_)) => *v >= 0 && *v <= u8::MAX as i128,
        (Value::BigInt(v), TypeExpr::I16(_)) => *v >= i16::MIN as i128 && *v <= i16::MAX as i128,
        (Value::BigInt(v), TypeExpr::U16(_)) => *v >= 0 && *v <= u16::MAX as i128,
        (Value::BigInt(v), TypeExpr::I32(_)) => *v >= i32::MIN as i128 && *v <= i32::MAX as i128,
        (Value::BigInt(v), TypeExpr::U32(_)) => *v >= 0 && *v <= u32::MAX as i128,
        // Float types — both f32 and f64 accept any Float value
        (Value::Float(_), TypeExpr::F32(_)) => true,
        (Value::Float(_), TypeExpr::F64(_)) => true,
        // Int values also match float types (implicit widening)
        (Value::Int(_), TypeExpr::F32(_)) => true,
        (Value::Int(_), TypeExpr::F64(_)) => true,
        (Value::BigInt(_), TypeExpr::F32(_)) => true,
        (Value::BigInt(_), TypeExpr::F64(_)) => true,
        // Date and duration types
        (Value::Date(_), TypeExpr::Date(_)) => true,
        (Value::Duration(_), TypeExpr::Duration(_)) => true,
        (Value::Bool(_), TypeExpr::Bool(_)) => true,
        (Value::Null, TypeExpr::Null(_)) => true,
        (Value::Identifier(_), TypeExpr::Identifier(_)) => true,
        (Value::Symbol(_), TypeExpr::Symbol(_)) => true,
        (Value::List(items), TypeExpr::List(inner, _)) => {
            items.iter().all(|item| check_type(item, inner))
        }
        (Value::Map(map), TypeExpr::Map(_key_type, val_type, _)) => {
            // Keys are always strings in our impl
            map.values().all(|v| check_type(v, val_type))
        }
        (Value::Set(items), TypeExpr::Set(inner, _)) => {
            items.iter().all(|item| check_type(item, inner))
        }
        (Value::Pattern(_), TypeExpr::Pattern(_)) => true,
        // StructType validation is handled by the StructRegistry, not here
        (Value::Map(_), TypeExpr::StructType(_, _)) => true,
        (_, TypeExpr::Union(types, _)) => types.iter().any(|t| check_type(value, t)),
        _ => false,
    }
}

/// Get a human-readable type name for a TypeExpr
pub fn type_name(type_expr: &TypeExpr) -> String {
    match type_expr {
        TypeExpr::String(_) => "string".to_string(),
        TypeExpr::I8(_) => "i8".to_string(),
        TypeExpr::U8(_) => "u8".to_string(),
        TypeExpr::I16(_) => "i16".to_string(),
        TypeExpr::U16(_) => "u16".to_string(),
        TypeExpr::I32(_) => "i32".to_string(),
        TypeExpr::U32(_) => "u32".to_string(),
        TypeExpr::I64(_) => "i64".to_string(),
        TypeExpr::U64(_) => "u64".to_string(),
        TypeExpr::I128(_) => "i128".to_string(),
        TypeExpr::U128(_) => "u128".to_string(),
        TypeExpr::F32(_) => "f32".to_string(),
        TypeExpr::F64(_) => "f64".to_string(),
        TypeExpr::Date(_) => "date".to_string(),
        TypeExpr::Duration(_) => "duration".to_string(),
        TypeExpr::Bool(_) => "bool".to_string(),
        TypeExpr::Null(_) => "null".to_string(),
        TypeExpr::Identifier(_) => "identifier".to_string(),
        TypeExpr::Any(_) => "any".to_string(),
        TypeExpr::Symbol(_) => "symbol".to_string(),
        TypeExpr::List(inner, _) => format!("list({})", type_name(inner)),
        TypeExpr::Map(k, v, _) => format!("map({}, {})", type_name(k), type_name(v)),
        TypeExpr::Set(inner, _) => format!("set({})", type_name(inner)),
        TypeExpr::Ref(name, _) => {
            let n = match &name.parts[..] {
                [StringPart::Literal(s)] => s.clone(),
                _ => "?".to_string(),
            };
            format!("ref(\"{}\")", n)
        }
        TypeExpr::Union(types, _) => {
            let names: Vec<_> = types.iter().map(type_name).collect();
            format!("union({})", names.join(", "))
        }
        TypeExpr::StructType(ident, _) => ident.name.clone(),
        TypeExpr::Pattern(_) => "pattern".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::span::Span;

    fn sp() -> Span {
        Span::dummy()
    }

    #[test]
    fn check_string() {
        assert!(check_type(
            &Value::String("hello".into()),
            &TypeExpr::String(sp())
        ));
        assert!(!check_type(&Value::Int(1), &TypeExpr::String(sp())));
    }

    #[test]
    fn check_i64() {
        assert!(check_type(&Value::Int(42), &TypeExpr::I64(sp())));
        assert!(!check_type(&Value::Float(1.0), &TypeExpr::I64(sp())));
    }

    #[test]
    fn check_i8_range() {
        assert!(check_type(&Value::Int(127), &TypeExpr::I8(sp())));
        assert!(check_type(&Value::Int(-128), &TypeExpr::I8(sp())));
        assert!(!check_type(&Value::Int(128), &TypeExpr::I8(sp())));
        assert!(!check_type(&Value::Int(-129), &TypeExpr::I8(sp())));
    }

    #[test]
    fn check_u8_range() {
        assert!(check_type(&Value::Int(0), &TypeExpr::U8(sp())));
        assert!(check_type(&Value::Int(255), &TypeExpr::U8(sp())));
        assert!(!check_type(&Value::Int(256), &TypeExpr::U8(sp())));
        assert!(!check_type(&Value::Int(-1), &TypeExpr::U8(sp())));
    }

    #[test]
    fn check_u64() {
        assert!(check_type(&Value::Int(0), &TypeExpr::U64(sp())));
        assert!(check_type(&Value::Int(i64::MAX), &TypeExpr::U64(sp())));
        assert!(!check_type(&Value::Int(-1), &TypeExpr::U64(sp())));
    }

    #[test]
    fn check_f64() {
        assert!(check_type(&Value::Float(3.14), &TypeExpr::F64(sp())));
        // int values should also match float types
        assert!(check_type(&Value::Int(3), &TypeExpr::F64(sp())));
    }

    #[test]
    fn check_date() {
        assert!(check_type(
            &Value::Date("2024-03-15".into()),
            &TypeExpr::Date(sp())
        ));
        assert!(!check_type(
            &Value::String("2024-03-15".into()),
            &TypeExpr::Date(sp())
        ));
    }

    #[test]
    fn check_duration() {
        assert!(check_type(
            &Value::Duration("P1Y2M3D".into()),
            &TypeExpr::Duration(sp())
        ));
        assert!(!check_type(
            &Value::String("P1Y2M3D".into()),
            &TypeExpr::Duration(sp())
        ));
    }

    #[test]
    fn check_bool() {
        assert!(check_type(&Value::Bool(true), &TypeExpr::Bool(sp())));
        assert!(!check_type(&Value::Null, &TypeExpr::Bool(sp())));
    }

    #[test]
    fn check_null() {
        assert!(check_type(&Value::Null, &TypeExpr::Null(sp())));
        assert!(!check_type(&Value::Bool(false), &TypeExpr::Null(sp())));
    }

    #[test]
    fn check_identifier() {
        assert!(check_type(
            &Value::Identifier("svc-auth".into()),
            &TypeExpr::Identifier(sp())
        ));
        assert!(!check_type(
            &Value::String("svc-auth".into()),
            &TypeExpr::Identifier(sp())
        ));
    }

    #[test]
    fn check_any_accepts_all() {
        assert!(check_type(&Value::Int(1), &TypeExpr::Any(sp())));
        assert!(check_type(&Value::Null, &TypeExpr::Any(sp())));
        assert!(check_type(&Value::String("x".into()), &TypeExpr::Any(sp())));
    }

    #[test]
    fn check_list_homogeneous() {
        let t = TypeExpr::List(Box::new(TypeExpr::I64(sp())), sp());
        assert!(check_type(
            &Value::List(vec![Value::Int(1), Value::Int(2)]),
            &t
        ));
        assert!(!check_type(
            &Value::List(vec![Value::Int(1), Value::String("x".into())]),
            &t
        ));
    }

    #[test]
    fn check_union() {
        let t = TypeExpr::Union(vec![TypeExpr::String(sp()), TypeExpr::Null(sp())], sp());
        assert!(check_type(&Value::String("hi".into()), &t));
        assert!(check_type(&Value::Null, &t));
        assert!(!check_type(&Value::Int(1), &t));
    }

    #[test]
    fn type_name_primitives() {
        assert_eq!(type_name(&TypeExpr::String(sp())), "string");
        assert_eq!(type_name(&TypeExpr::I8(sp())), "i8");
        assert_eq!(type_name(&TypeExpr::U8(sp())), "u8");
        assert_eq!(type_name(&TypeExpr::I64(sp())), "i64");
        assert_eq!(type_name(&TypeExpr::F64(sp())), "f64");
        assert_eq!(type_name(&TypeExpr::Date(sp())), "date");
        assert_eq!(type_name(&TypeExpr::Duration(sp())), "duration");
        assert_eq!(type_name(&TypeExpr::Bool(sp())), "bool");
        assert_eq!(type_name(&TypeExpr::Null(sp())), "null");
        assert_eq!(type_name(&TypeExpr::Identifier(sp())), "identifier");
        assert_eq!(type_name(&TypeExpr::Any(sp())), "any");
    }

    #[test]
    fn type_name_compound() {
        assert_eq!(
            type_name(&TypeExpr::List(Box::new(TypeExpr::I64(sp())), sp())),
            "list(i64)"
        );
        assert_eq!(
            type_name(&TypeExpr::Union(
                vec![TypeExpr::String(sp()), TypeExpr::Null(sp())],
                sp()
            )),
            "union(string, null)"
        );
    }
}
