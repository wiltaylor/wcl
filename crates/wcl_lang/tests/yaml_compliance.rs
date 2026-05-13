use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use wcl_lang::Value;

const FIXTURE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/yaml-suite");

#[derive(Debug, Deserialize)]
struct Manifest {
    upstream_commit: String,
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

#[derive(Debug, Deserialize)]
struct SuiteCase {
    yaml: String,
    json: Option<String>,
    #[serde(default)]
    fail: bool,
}

#[test]
fn vendored_yaml_suite_cases_are_classified() {
    let manifest = load_manifest();
    assert_eq!(
        manifest.upstream_commit,
        "da267a5c4782e7361e82889e76c0dc7df0e1e870"
    );

    let classified = manifest
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<HashSet<_>>();

    for entry in std::fs::read_dir(Path::new(FIXTURE_ROOT).join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
            continue;
        }
        let id = path.file_stem().unwrap().to_str().unwrap();
        assert!(classified.contains(id), "unclassified YAML fixture {id}");
    }

    for case in &manifest.cases {
        assert!(!case.reason.trim().is_empty(), "{} lacks a reason", case.id);
        assert!(
            fixture_path(&case.id).exists(),
            "{} is classified but not vendored",
            case.id
        );
    }
}

#[test]
fn yaml_compliance_gate_curated_cases() {
    std::thread::Builder::new()
        .name("yaml-compliance".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(run_yaml_compliance_gate_curated_cases)
        .unwrap()
        .join()
        .unwrap();
}

fn run_yaml_compliance_gate_curated_cases() {
    let manifest = load_manifest();
    let registry = wcl_lang::transform::codec::custom::standard_registry().unwrap();
    let yaml = registry.get("yaml").expect("standard yaml codec");

    for case in manifest.cases {
        let suite_case = load_suite_case(&case.id);
        match case.classification {
            Classification::Pass => {
                let expected = expected_records(suite_case.json.as_deref().unwrap_or_else(|| {
                    panic!("{} pass fixture must include upstream json", case.id)
                }));
                let actual = wcl_lang::transform::codec::custom::decode_custom_records(
                    suite_case.yaml.as_bytes(),
                    yaml,
                )
                .unwrap_or_else(|err| panic!("{} should decode: {}", case.id, err));
                assert_eq!(actual, expected, "{} mismatch", case.id);
            }
            Classification::Error => {
                assert!(suite_case.fail, "{} error fixture should set fail", case.id);
                assert!(
                    wcl_lang::transform::codec::custom::decode_custom_records(
                        suite_case.yaml.as_bytes(),
                        yaml,
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
fn yaml_wcl_encoder_roundtrips_records() {
    std::thread::Builder::new()
        .name("yaml-encoder".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(run_yaml_wcl_encoder_roundtrips_records)
        .unwrap()
        .join()
        .unwrap();
}

fn run_yaml_wcl_encoder_roundtrips_records() {
    let registry = wcl_lang::transform::codec::custom::standard_registry().unwrap();
    let yaml = registry.get("yaml").expect("standard yaml codec");

    let expected_json = serde_json::json!([
        {"name": "Alice", "active": true, "score": 42, "tags": ["a", "b"]},
        {"name": "Bob", "active": false, "score": 7, "meta": {"city": "Brisbane"}}
    ]);
    let expected = expected_records(&expected_json.to_string());

    let mut output = Vec::new();
    wcl_lang::transform::codec::custom::encode_custom_records(
        &expected,
        yaml,
        &Default::default(),
        &mut output,
    )
    .unwrap();

    let decoded =
        wcl_lang::transform::codec::custom::decode_custom_records(output.as_slice(), yaml).unwrap();
    assert_eq!(decoded, expected);
}

fn load_manifest() -> Manifest {
    let path = Path::new(FIXTURE_ROOT).join("manifest.json");
    let source = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&source).unwrap()
}

fn load_suite_case(id: &str) -> SuiteCase {
    let source = std::fs::read_to_string(fixture_path(id)).unwrap();
    let mut cases: Vec<SuiteCase> = serde_yaml_ng::from_str(&source).unwrap();
    assert_eq!(cases.len(), 1, "{id} should contain one suite case");
    cases.pop().unwrap()
}

fn fixture_path(id: &str) -> PathBuf {
    Path::new(FIXTURE_ROOT)
        .join("src")
        .join(format!("{id}.yaml"))
}

fn expected_records(json_stream: &str) -> Vec<Value> {
    let mut records = Vec::new();
    let stream = serde_json::Deserializer::from_str(json_stream).into_iter::<serde_json::Value>();
    for value in stream {
        let value = wcl_lang::json::json_value_to_wcl(&value.unwrap());
        match value {
            Value::List(items) => records.extend(items),
            other => records.push(other),
        }
    }
    records
}
