pub use wcl_lang::json::{
    block_ref_to_json, diagnostic_to_json, json_to_value, value_to_json, values_to_json,
};

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use wcl_lang::Value;

    #[test]
    fn test_value_roundtrip_primitives() {
        let cases = vec![
            Value::String("hello".into()),
            Value::Int(42),
            Value::Float(2.72),
            Value::Bool(true),
            Value::Null,
        ];
        for val in cases {
            let json = value_to_json(&val);
            let back = json_to_value(&json).unwrap();
            assert_eq!(val, back);
        }
    }

    #[test]
    fn test_value_roundtrip_list() {
        let val = Value::List(vec![Value::Int(1), Value::String("two".into())]);
        let json = value_to_json(&val);
        let back = json_to_value(&json).unwrap();
        assert_eq!(val, back);
    }

    #[test]
    fn test_value_roundtrip_map() {
        let mut map = IndexMap::new();
        map.insert("key".to_string(), Value::Int(42));
        let val = Value::Map(map);
        let json = value_to_json(&val);
        let back = json_to_value(&json).unwrap();
        assert_eq!(val, back);
    }

    #[test]
    fn test_block_ref_to_json() {
        let br = wcl_lang::BlockRef {
            kind: "server".to_string(),
            id: Some("main".to_string()),
            qualified_id: None,
            attributes: IndexMap::new(),
            children: vec![],
            decorators: vec![],
            span: wcl_lang::Span::dummy(),
        };
        let json = block_ref_to_json(&br);
        assert_eq!(json["kind"], "server");
        assert_eq!(json["id"], "main");
    }
}
