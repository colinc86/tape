//! End-to-end coverage for `tape redact-test` Phase 1 (issue #223,
//! carved from #104). Builds rules + JSONL test-case files at runtime
//! in a tempdir and shells out to the binary. Asserts:
//! - all-correct cases → exit 0 with summary; no FP/FN sections
//! - mixed FP + FN → exit 1, both sections present, summary counts match
//! - malformed JSONL line → exit 2 with line number
//! - typoed key (`expectmatch`) → exit 2 (deny_unknown_fields)
//! - bad `.taperc` (unknown key under `redact:`) → exit 2
//! - bad regex → exit 2 with rules-file path
//! - empty cases file → exit 0 with `0 test cases:` summary
//! - built-in rule (`email`) coexists with a custom rule
//! - `--help` documents the format

use std::path::{Path, PathBuf};

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

fn write(path: &Path, body: &str) {
    std::fs::write(path, body).expect("write fixture");
}

fn run(rules: &Path, cases: &Path) -> std::process::Output {
    std::process::Command::new(binary_path())
        .args([
            "redact-test",
            rules.to_str().unwrap(),
            cases.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

const CUSTOM_RULE_YAML: &str = "\
redact:
  custom:
    - id: pii_customer
      pattern: 'CUST-\\d{6}'
      replacement: '<CUSTOMER>'
";

#[test]
fn all_correct_cases_exit_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let rules = tmp.path().join("rules.yaml");
    write(&rules, CUSTOM_RULE_YAML);
    let cases = tmp.path().join("cases.jsonl");
    write(
        &cases,
        "\
{\"input\":\"CUST-123456 in body\",\"expect_match\":true}
{\"input\":\"no customer id\",\"expect_match\":false}

{\"input\":\"CUST-999999\",\"expect_match\":true}
",
    );

    let r = run(&rules, &cases);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("3 test cases: 3 passed, 0 failed (0 false positives, 0 false negatives)"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("FALSE POSITIVES"));
    assert!(!stdout.contains("FALSE NEGATIVES"));
}

#[test]
fn mixed_failures_exit_one_with_both_sections() {
    let tmp = tempfile::tempdir().unwrap();
    let rules = tmp.path().join("rules.yaml");
    write(&rules, CUSTOM_RULE_YAML);
    let cases = tmp.path().join("cases.jsonl");
    // 1 true positive, 1 true negative, 1 false positive
    // (matches but expected no), 1 false negative (didn't match
    // but expected yes).
    write(
        &cases,
        "\
{\"input\":\"CUST-111111\",\"expect_match\":true}
{\"input\":\"benign text\",\"expect_match\":false}
{\"input\":\"CUST-222222 should be ignored per author\",\"expect_match\":false}
{\"input\":\"customer 9 — should match but won't\",\"expect_match\":true}
",
    );

    let r = run(&rules, &cases);
    assert_eq!(r.status.code(), Some(1), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("4 test cases: 2 passed, 2 failed (1 false positives, 1 false negatives)"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("FALSE POSITIVES"), "stdout: {stdout}");
    assert!(stdout.contains("FALSE NEGATIVES"), "stdout: {stdout}");
    assert!(stdout.contains("CUST-222222"), "stdout: {stdout}");
    assert!(stdout.contains("customer 9"), "stdout: {stdout}");
}

#[test]
fn malformed_jsonl_line_exits_two_with_line_number() {
    let tmp = tempfile::tempdir().unwrap();
    let rules = tmp.path().join("rules.yaml");
    write(&rules, CUSTOM_RULE_YAML);
    let cases = tmp.path().join("cases.jsonl");
    // Line 2 is truncated JSON — should be the line reported.
    write(
        &cases,
        "\
{\"input\":\"ok\",\"expect_match\":false}
{this is not json
{\"input\":\"never reached\",\"expect_match\":false}
",
    );

    let r = run(&rules, &cases);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("line 2"), "stderr: {stderr}");
}

#[test]
fn typoed_expect_match_field_exits_two() {
    let tmp = tempfile::tempdir().unwrap();
    let rules = tmp.path().join("rules.yaml");
    write(&rules, CUSTOM_RULE_YAML);
    let cases = tmp.path().join("cases.jsonl");
    // `expectmatch` (no underscore) — deny_unknown_fields must reject.
    write(&cases, "{\"input\":\"x\",\"expectmatch\":true}\n");

    let r = run(&rules, &cases);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("line 1"), "stderr: {stderr}");
}

#[test]
fn bad_taperc_unknown_key_exits_two() {
    let tmp = tempfile::tempdir().unwrap();
    let rules = tmp.path().join("rules.yaml");
    // `disabled_default` (typo) under `redact:` — RedactConfig is
    // deny_unknown_fields, so this must fail at parse time.
    write(&rules, "redact:\n  disabled_default: [email]\n");
    let cases = tmp.path().join("cases.jsonl");
    write(&cases, "{\"input\":\"x\",\"expect_match\":false}\n");

    let r = run(&rules, &cases);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("rules.yaml"), "stderr: {stderr}");
}

#[test]
fn bad_regex_in_custom_rule_exits_two() {
    let tmp = tempfile::tempdir().unwrap();
    let rules = tmp.path().join("rules.yaml");
    write(
        &rules,
        "\
redact:
  custom:
    - id: bad_regex
      pattern: '['  # unmatched bracket
      replacement: '<X>'
",
    );
    let cases = tmp.path().join("cases.jsonl");
    write(&cases, "{\"input\":\"x\",\"expect_match\":false}\n");

    let r = run(&rules, &cases);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("rules.yaml"), "stderr: {stderr}");
}

#[test]
fn empty_cases_file_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let rules = tmp.path().join("rules.yaml");
    write(&rules, CUSTOM_RULE_YAML);
    let cases = tmp.path().join("cases.jsonl");
    write(&cases, "\n# (no test cases)\n\n");
    // Note: `#` is NOT a comment in the JSONL format — it's a
    // malformed JSON line and would be exit 2. Use only blanks.
    write(&cases, "\n\n\n");

    let r = run(&rules, &cases);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("0 test cases: 0 passed, 0 failed (0 false positives, 0 false negatives)"),
        "stdout: {stdout}"
    );
}

#[test]
fn built_in_rule_and_custom_rule_coexist() {
    let tmp = tempfile::tempdir().unwrap();
    let rules = tmp.path().join("rules.yaml");
    write(&rules, CUSTOM_RULE_YAML);
    let cases = tmp.path().join("cases.jsonl");
    // Built-in `email` rule should fire on a real email; the custom
    // `pii_customer` rule should fire on the CUST- pattern. Both
    // should pass; a benign string should match neither.
    write(
        &cases,
        "\
{\"input\":\"alice@example.com\",\"expect_match\":true}
{\"input\":\"CUST-333333\",\"expect_match\":true}
{\"input\":\"plain text\",\"expect_match\":false}
",
    );

    let r = run(&rules, &cases);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(
        stdout.contains("3 test cases: 3 passed, 0 failed"),
        "stdout: {stdout}"
    );
}

#[test]
fn help_documents_the_format() {
    let r = std::process::Command::new(binary_path())
        .args(["redact-test", "--help"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let lower = stdout.to_lowercase();
    assert!(lower.contains("jsonl"), "help: {stdout}");
    assert!(lower.contains("expect_match"), "help: {stdout}");
    assert!(lower.contains("exit code"), "help: {stdout}");
}

#[test]
fn missing_rules_file_exits_two() {
    let tmp = tempfile::tempdir().unwrap();
    let rules = tmp.path().join("nope.yaml");
    let cases = tmp.path().join("c.jsonl");
    write(&cases, "");
    let r = run(&rules, &cases);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("nope.yaml"), "stderr: {stderr}");
}
