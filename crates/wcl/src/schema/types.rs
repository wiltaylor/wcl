use crate::eval::value::Value;
use crate::lang::ast::{StringPart, TypeExpr};

/// Check if a Value matches a TypeExpr
pub fn check_type(value: &Value, type_expr: &TypeExpr) -> bool {
    match (value, type_expr) {
        (_, TypeExpr::Any(_)) => true,
        (Value::String(_), TypeExpr::String(_)) => true,
        (Value::Int(_), TypeExpr::Int(_)) => true,
        (Value::Float(_), TypeExpr::Float(_)) => true,
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
        (_, TypeExpr::Union(types, _)) => types.iter().any(|t| check_type(value, t)),
        _ => false,
    }
}

/// Get a human-readable type name for a TypeExpr
pub fn type_name(type_expr: &TypeExpr) -> String {
    match type_expr {
        TypeExpr::String(_) => "string".to_string(),
        TypeExpr::Int(_) => "int".to_string(),
        TypeExpr::Float(_) => "float".to_string(),
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
    fn check_int() {
        assert!(check_type(&Value::Int(42), &TypeExpr::Int(sp())));
        assert!(!check_type(&Value::Float(1.0), &TypeExpr::Int(sp())));
    }

    #[test]
    fn check_float() {
        assert!(check_type(&Value::Float(3.14), &TypeExpr::Float(sp())));
        assert!(!check_type(&Value::Int(3), &TypeExpr::Float(sp())));
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
        let t = TypeExpr::List(Box::new(TypeExpr::Int(sp())), sp());
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
        assert_eq!(type_name(&TypeExpr::Int(sp())), "int");
        assert_eq!(type_name(&TypeExpr::Float(sp())), "float");
        assert_eq!(type_name(&TypeExpr::Bool(sp())), "bool");
        assert_eq!(type_name(&TypeExpr::Null(sp())), "null");
        assert_eq!(type_name(&TypeExpr::Identifier(sp())), "identifier");
        assert_eq!(type_name(&TypeExpr::Any(sp())), "any");
    }

    #[test]
    fn type_name_compound() {
        assert_eq!(
            type_name(&TypeExpr::List(Box::new(TypeExpr::Int(sp())), sp())),
            "list(int)"
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
