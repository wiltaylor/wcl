use indexmap::IndexMap;
use wcl_lang::eval::value::Value;
use wcl_lang::transform::codec::{custom, CodecOptions};
use wcl_lang::transform::TransformError;

fn msgpack_codec() -> custom::CustomCodec {
    custom::standard_registry()
        .expect("standard codecs")
        .get("msgpack")
        .expect("msgpack codec")
        .clone()
}

fn decode(input: &[u8]) -> Result<Vec<Value>, TransformError> {
    custom::decode_custom_records_with_options(input, &msgpack_codec(), &CodecOptions::new())
}

fn encode(records: &[Value]) -> Result<Vec<u8>, TransformError> {
    let mut output = Vec::new();
    custom::encode_custom_records(records, &msgpack_codec(), &CodecOptions::new(), &mut output)?;
    Ok(output)
}

#[test]
fn decodes_core_types_and_top_level_array_records() {
    let input = [
        0x96, 0xc0, 0xc3, 0xd0, 0xfb, 0xcd, 0x01, 0x00, 0xa2, b'h', b'i', 0x92, 0x01, 0x02,
    ];
    let records = decode(&input).unwrap();
    assert_eq!(
        records,
        vec![
            Value::Null,
            Value::Bool(true),
            Value::Int(-5),
            Value::Int(256),
            Value::String("hi".into()),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        ]
    );
}

#[test]
fn decodes_bytes_ext_timestamps_and_non_string_map_keys() {
    assert_eq!(
        decode(&[0xc4, 0x03, 0x01, 0x02, 0x03]).unwrap(),
        vec![Value::Bytes(vec![1, 2, 3])]
    );
    assert_eq!(
        decode(&[0xd4, 0x02, 0xff]).unwrap(),
        vec![Value::MsgPackExt {
            type_id: 2,
            data: vec![255],
        }]
    );
    assert_eq!(
        decode(&[0xd6, 0xff, 0x00, 0x00, 0x00, 0x2a]).unwrap(),
        vec![Value::MsgPackTimestamp {
            seconds: 42,
            nanoseconds: 0,
        }]
    );

    let decoded = decode(&[0x81, 0x01, 0xa3, b'o', b'n', b'e']).unwrap();
    let Value::Map(map) = &decoded[0] else {
        panic!("expected tagged map");
    };
    assert_eq!(
        map.get("__msgpack_type"),
        Some(&Value::String("map".into()))
    );
    let Some(Value::List(entries)) = map.get("entries") else {
        panic!("expected entries");
    };
    let Value::Map(entry) = &entries[0] else {
        panic!("expected entry map");
    };
    assert_eq!(entry.get("key"), Some(&Value::Int(1)));
    assert_eq!(entry.get("value"), Some(&Value::String("one".into())));
}

#[test]
fn encodes_canonical_representative_values() {
    assert_eq!(
        encode(&[Value::Int(1), Value::Int(2)]).unwrap(),
        vec![0x92, 0x01, 0x02]
    );
    assert_eq!(
        encode(&[Value::Bytes(vec![1, 2, 3])]).unwrap(),
        vec![0x91, 0xc4, 0x03, 0x01, 0x02, 0x03]
    );
    assert_eq!(
        encode(&[Value::MsgPackExt {
            type_id: 2,
            data: vec![255],
        }])
        .unwrap(),
        vec![0x91, 0xd4, 0x02, 0xff]
    );
    assert_eq!(
        encode(&[Value::MsgPackTimestamp {
            seconds: 42,
            nanoseconds: 0,
        }])
        .unwrap(),
        vec![0x91, 0xd6, 0xff, 0x00, 0x00, 0x00, 0x2a]
    );
}

#[test]
fn encodes_tagged_non_string_key_map() {
    let mut entry = IndexMap::new();
    entry.insert("key".into(), Value::Int(1));
    entry.insert("value".into(), Value::String("one".into()));
    let mut tagged = IndexMap::new();
    tagged.insert("__msgpack_type".into(), Value::String("map".into()));
    tagged.insert("entries".into(), Value::List(vec![Value::Map(entry)]));

    assert_eq!(
        encode(&[Value::Map(tagged)]).unwrap(),
        vec![0x91, 0x81, 0x01, 0xa3, b'o', b'n', b'e']
    );
}
