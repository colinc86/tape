//! `tape stats --with-cost --pricing-file <PATH>` integration coverage.
//! Step-4 of #31 (issue #181). Mirrors the AC bullets in the issue
//! body. Generates a priceable fixture via `tape new --template
//! test-fixture` so the `model_call` events carry `tokens_in` / `tokens_out`
//! — the bundled `minimal-success.tape` fixture lacks them.

use std::process::Command;

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

fn pricing_fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("pricing")
        .join(name)
}

/// Generate a fresh `test-fixture` cassette into the given tempdir.
/// Returns the path to the generated cassette. The template ships
/// with three `model_call` events at `anthropic / claude-haiku-4-5`
/// with non-zero token counts — exactly what `--with-cost` needs to
/// produce a non-trivial dollar total.
fn make_priceable_cassette(dir: &std::path::Path) -> std::path::PathBuf {
    let cassette = dir.join("input.tape");
    let out = Command::new(binary_path())
        .args([
            "new",
            cassette.to_str().unwrap(),
            "--template",
            "test-fixture",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "tape new failed: {out:?}");
    cassette
}

#[test]
fn pricing_file_replaces_bundled_table_and_qualifier_names_loaded_date() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());

    let out = Command::new(binary_path())
        .args([
            "stats",
            "--with-cost",
            "--pricing-file",
            pricing_fixture("good.toml").to_str().unwrap(),
            cassette.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "tape stats failed: {out:?}");
    let s = String::from_utf8(out.stdout).unwrap();

    // The qualifier on the `cost:` line must name the loaded file's
    // `last_updated` (2026-04-01), NOT the bundled date.
    assert!(
        s.contains("pricing table 2026-04-01"),
        "expected loaded date in qualifier:\n{s}"
    );
    // Haiku rates in the loaded file are doubled vs the bundled table
    // ($2 / $10 per Mtok vs $1 / $5). The fixture's three model_calls
    // are 100+40, 60+25, 80+35 = 240 in + 100 out tokens.
    // Loaded: 240*2 + 100*10 = 480 + 1000 = 1480 µ$ = $0.0015
    // Bundled would have been: 240*1 + 100*5 = 240 + 500 = 740 µ$ = $0.0007
    // Asserting on the loaded-table-only value proves replace-not-merge.
    assert!(
        s.contains("$0.0015"),
        "expected loaded-table dollar total $0.0015 (240 in @ $2/Mtok + 100 out @ $10/Mtok):\n{s}"
    );
}

#[test]
fn pricing_file_missing_file_exits_two() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());
    let bogus = dir.path().join("does-not-exist.toml");

    let out = Command::new(binary_path())
        .args([
            "stats",
            "--with-cost",
            "--pricing-file",
            bogus.to_str().unwrap(),
            cassette.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("failed to read file"), "{stderr}");
    assert!(stderr.contains(bogus.to_string_lossy().as_ref()));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("cost:") && !stdout.contains("id:"),
        "no stats body should leak on error: {stdout}"
    );
}

#[test]
fn pricing_file_bad_toml_exits_two_with_path_named() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());

    let bad = pricing_fixture("bad.toml");
    let out = Command::new(binary_path())
        .args([
            "stats",
            "--with-cost",
            "--pricing-file",
            bad.to_str().unwrap(),
            cassette.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(bad.to_string_lossy().as_ref()), "{stderr}");
    assert!(stderr.contains("negative"), "{stderr}");
}

#[test]
fn pricing_file_stale_emits_warning_naming_the_user_file() {
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());
    let stale = pricing_fixture("stale.toml");

    let out = Command::new(binary_path())
        .args([
            "stats",
            "--with-cost",
            "--pricing-file",
            stale.to_str().unwrap(),
            cassette.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("warning:"), "stale-guard should warn:\n{s}");
    assert!(
        s.contains(stale.to_string_lossy().as_ref()),
        "warning must name the user's file:\n{s}"
    );
}

#[test]
fn pricing_file_without_with_cost_is_a_soft_warning_not_an_error() {
    // AC: "--pricing-file without --with-cost is a no-op" — the
    // implementer picked "soft warning on stderr, still proceed".
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());

    let out = Command::new(binary_path())
        .args([
            "stats",
            "--pricing-file",
            pricing_fixture("good.toml").to_str().unwrap(),
            cassette.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "expected exit 0: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--pricing-file has no effect without --with-cost"),
        "expected soft warning: {stderr}"
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        !s.contains("cost:"),
        "cost line still suppressed without --with-cost:\n{s}"
    );
}

#[test]
fn pricing_file_with_format_json_is_rejected() {
    // Same rejection as Step-3 (`--with-cost --format json`); the
    // pricing-file flag changes nothing about the JSON-schema deferral.
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());

    let out = Command::new(binary_path())
        .args([
            "stats",
            "--with-cost",
            "--pricing-file",
            pricing_fixture("good.toml").to_str().unwrap(),
            "--format",
            "json",
            cassette.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("text-only"), "{stderr}");
}

#[test]
fn no_pricing_file_keeps_bundled_table_byte_for_byte() {
    // Regression guard for AC #9 / the Phase-1/2 byte-for-byte rule.
    // Without `--pricing-file` the renderer must use the bundled
    // table and the qualifier must name the bundled date.
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());

    let out = Command::new(binary_path())
        .args(["stats", "--with-cost", cassette.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("pricing table 2026-"), "{s}");
    // The bundled haiku rate is $1/$5 per Mtok. 240*1 + 100*5 = 740
    // µ$ = $0.0007.
    assert!(
        s.contains("$0.0007"),
        "expected bundled-table dollar value:\n{s}"
    );
}
