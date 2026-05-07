//! Runs `tape_format::verify::verify` against every checked-in fixture and
//! asserts: valid fixtures produce zero errors; malformed fixtures produce
//! exactly the diagnostic codes their `<name>.expected.json` sidecar lists.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use tape_format::reader::RawTape;
use tape_format::verify::{verify, Severity};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
}

#[test]
fn valid_fixtures_verify_clean() {
    let dir = fixtures_dir();
    let mut found = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() || path.extension().and_then(|e| e.to_str()) != Some("tape") {
            continue;
        }
        found += 1;
        let raw = RawTape::open(&path).expect("open fixture");
        let report = verify(&raw);
        let errors: Vec<_> = report.errors().collect();
        assert!(
            errors.is_empty(),
            "valid fixture {} produced errors: {:?}",
            path.display(),
            errors.iter().map(|d| d.code.as_str()).collect::<Vec<_>>()
        );
    }
    assert!(found >= 2, "expected ≥2 valid fixtures, found {found}");
}

#[test]
fn malformed_fixtures_produce_expected_codes() {
    let dir = fixtures_dir().join("malformed");
    let mut found = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("tape") {
            continue;
        }
        found += 1;
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let expected_path = path.with_file_name(format!("{stem}.expected.json"));
        let expected_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&expected_path).unwrap()).unwrap();
        let expected_codes: BTreeSet<String> = expected_json["expect_codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();

        let raw = RawTape::open(&path).expect("open malformed fixture");
        let report = verify(&raw);
        let actual_codes: BTreeSet<String> = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.code.as_str().to_owned())
            .collect();

        assert!(
            expected_codes.is_subset(&actual_codes),
            "fixture {}: expected codes {:?} not a subset of actual {:?}",
            path.display(),
            expected_codes,
            actual_codes
        );
    }
    assert!(found >= 5, "expected ≥5 malformed fixtures, found {found}");
}
