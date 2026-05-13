use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use wcl_lang::Value;

const FIXTURE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/toml-suite");

#[derive(Debug, Deserialize)]
struct Manifest {
    spec: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    classification: Classification,
    reason: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Classification {
    Pass,
    Error,
    Skip,
}

#[test]
fn vendored_toml_suite_cases_are_classified() {
    let manifest = load_manifest();
    assert_eq!(manifest.spec, "TOML v1.1.0");

    let classified = manifest
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<HashSet<_>>();

    for dir in ["valid", "invalid"] {
        for entry in std::fs::read_dir(Path::new(FIXTURE_ROOT).join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }
            let id = path.file_stem().unwrap().to_str().unwrap();
            assert!(classified.contains(id), "unclassified TOML fixture {id}");
        }
    }

    for case in &manifest.cases {
        assert!(!case.reason.trim().is_empty(), "{} lacks a reason", case.id);
        assert!(fixture_path(case).exists(), "{} is missing", case.id);
    }
}

#[test]
fn toml_compliance_gate_curated_cases() {
    std::thread::Builder::new()
        .name("toml-compliance".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(run_toml_compliance_gate_curated_cases)
        .unwrap()
        .join()
        .unwrap();
}

fn run_toml_compliance_gate_curated_cases() {
    let manifest = load_manifest();
    let registry = wcl_lang::transform::codec::custom::standard_registry().unwrap();
    let toml = registry.get("toml").expect("standard toml codec");

    for case in manifest.cases {
        match case.classification {
            Classification::Pass => {
                let source = std::fs::read_to_string(fixture_path(&case)).unwrap();
                let expected = expected_records(&case.id);
                let actual = wcl_lang::transform::codec::custom::decode_custom_records(
                    source.as_bytes(),
                    toml,
                )
                .unwrap_or_else(|err| panic!("{} should decode: {}", case.id, err));
                assert_eq!(actual, expected, "{} mismatch", case.id);
            }
            Classification::Error => {
                let source = std::fs::read_to_string(fixture_path(&case)).unwrap();
                assert!(
                    wcl_lang::transform::codec::custom::decode_custom_records(
                        source.as_bytes(),
                        toml
                    )
                    .is_err(),
                    "{} should fail to decode",
                    case.id
                );
            }
            Classification::Skip => {}
        }
    }
}

#[test]
fn toml_wcl_encoder_roundtrips_records() {
    std::thread::Builder::new()
        .name("toml-encoder".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(run_toml_wcl_encoder_roundtrips_records)
        .unwrap()
        .join()
        .unwrap();
}

fn run_toml_wcl_encoder_roundtrips_records() {
    let registry = wcl_lang::transform::codec::custom::standard_registry().unwrap();
    let toml = registry.get("toml").expect("standard toml codec");
    let records = expected_records("array-of-tables");

    let mut output = Vec::new();
    wcl_lang::transform::codec::custom::encode_custom_records(
        &records,
        toml,
        &Default::default(),
        &mut output,
    )
    .unwrap();

    let output_str = String::from_utf8(output).unwrap();
    toml::from_str::<toml::Value>(&output_str).expect("emitted valid TOML");
    let decoded =
        wcl_lang::transform::codec::custom::decode_custom_records(output_str.as_bytes(), toml)
            .unwrap();
    assert_eq!(decoded, records);
}

fn load_manifest() -> Manifest {
    let path = Path::new(FIXTURE_ROOT).join("manifest.json");
    let source = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&source).unwrap()
}

fn fixture_path(case: &Case) -> PathBuf {
    let dir = match case.classification {
        Classification::Pass => "valid",
        Classification::Error => "invalid",
        Classification::Skip => "valid",
    };
    Path::new(FIXTURE_ROOT)
        .join(dir)
        .join(format!("{}.toml", case.id))
}

fn expected_records(id: &str) -> Vec<Value> {
    let path = Path::new(FIXTURE_ROOT)
        .join("valid")
        .join(format!("{id}.json"));
    let source = std::fs::read_to_string(path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&source).unwrap();
    let value = tagged_json_to_wcl(&json);
    match value {
        Value::List(items) => items,
        other => vec![other],
    }
}

fn tagged_json_to_wcl(value: &serde_json::Value) -> Value {
    if let Some(obj) = value.as_object() {
        if let (Some(kind), Some(raw)) = (obj.get("type"), obj.get("value")) {
            let kind = kind.as_str().unwrap();
            let raw = raw.as_str().unwrap();
            return match kind {
                "string" => Value::String(raw.into()),
                "integer" => Value::Int(raw.parse().unwrap()),
                "float" => Value::Float(raw.parse().unwrap()),
                "bool" => Value::Bool(raw == "true"),
                "datetime" => Value::OffsetDateTime(raw.into()),
                "datetime-local" => Value::LocalDateTime(raw.into()),
                "date-local" => Value::Date(raw.into()),
                "time-local" => Value::LocalTime(raw.into()),
                other => panic!("unsupported TOML tagged type {other}"),
            };
        }
        let mut map = indexmap::IndexMap::new();
        for (key, item) in obj {
            map.insert(key.clone(), tagged_json_to_wcl(item));
        }
        return Value::Map(map);
    }
    if let Some(items) = value.as_array() {
        return Value::List(items.iter().map(tagged_json_to_wcl).collect());
    }
    panic!("unexpected TOML tagged JSON shape: {value}");
}
