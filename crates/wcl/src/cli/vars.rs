use crate::Value;

pub fn parse_var_args(vars: &[String]) -> Result<indexmap::IndexMap<String, Value>, String> {
    let mut result = indexmap::IndexMap::new();
    for var in vars {
        let (key, raw_value) = var
            .split_once('=')
            .ok_or_else(|| format!("invalid --var format '{}', expected KEY=VALUE", var))?;
        let value = parse_value_string(raw_value);
        result.insert(key.to_string(), value);
    }
    Ok(result)
}

fn parse_value_string(s: &str) -> Value {
    // Try int
    if let Ok(i) = s.parse::<i64>() {
        return Value::Int(i);
    }
    // Try float
    if let Ok(f) = s.parse::<f64>() {
        return Value::Float(f);
    }
    // Bool
    match s {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    // Null
    if s == "null" {
        return Value::Null;
    }
    // Try JSON array/object
    if s.starts_with('[') || s.starts_with('{') {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(s) {
            if let Ok(val) = crate::json::json_to_value(&json) {
                return val;
            }
        }
    }
    // Strip outer quotes if present
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return Value::String(s[1..s.len() - 1].to_string());
    }
    Value::String(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_var_args_basic() {
        let vars = vec![
            "PORT=8080".to_string(),
            "DEBUG=true".to_string(),
            "NAME=\"my-app\"".to_string(),
            "RATE=3.14".to_string(),
            "EMPTY=null".to_string(),
        ];
        let result = parse_var_args(&vars).unwrap();
        assert_eq!(result.get("PORT"), Some(&Value::Int(8080)));
        assert_eq!(result.get("DEBUG"), Some(&Value::Bool(true)));
        assert_eq!(
            result.get("NAME"),
            Some(&Value::String("my-app".to_string()))
        );
        assert_eq!(result.get("RATE"), Some(&Value::Float(3.14)));
        assert_eq!(result.get("EMPTY"), Some(&Value::Null));
    }

    #[test]
    fn test_parse_var_args_list() {
        let vars = vec!["ITEMS=[1,2,3]".to_string()];
        let result = parse_var_args(&vars).unwrap();
        assert_eq!(
            result.get("ITEMS"),
            Some(&Value::List(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3)
            ]))
        );
    }

    #[test]
    fn test_parse_var_args_bare_string() {
        let vars = vec!["ENV=production".to_string()];
        let result = parse_var_args(&vars).unwrap();
        assert_eq!(
            result.get("ENV"),
            Some(&Value::String("production".to_string()))
        );
    }

    #[test]
    fn test_parse_var_args_invalid_format() {
        let vars = vec!["NOEQUALS".to_string()];
        assert!(parse_var_args(&vars).is_err());
    }
}
