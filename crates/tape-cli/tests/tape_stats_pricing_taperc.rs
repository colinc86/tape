//! `tape stats --with-cost` + `.taperc::pricing.pricing_file`
//! integration coverage. Step-5 of #31 (issue #186). Generates a
//! priceable cassette via `tape new --template test-fixture` and
//! drives a hermetic `$HOME` so each test owns its `.taperc`.

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

fn make_priceable_cassette(dir: &std::path::Path) -> std::path::PathBuf {
    let cassette = dir.join("input.tape");
    let out = Command::new(binary_path())
        .args([
            "new",
            cassette.to_str().unwrap(),
            "--template",
            "test-fixture",
        ])
        .env_remove("HOME")
        .output()
        .unwrap();
    assert!(out.status.success(), "tape new failed: {out:?}");
    cassette
}

/// Run `tape stats` with a hermetic `$HOME` and `--cwd <home>` (via
/// `current_dir`) so the workspace `.taperc` walk lands inside the
/// caller's tempdir. Returns the raw `Output` for the caller to
/// assert on.
fn run_stats(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(binary_path());
    cmd.args(args)
        .env_remove("HOME")
        .env("HOME", home)
        .current_dir(home);
    cmd.output().unwrap()
}

#[test]
fn taperc_pricing_file_consumed_when_flag_absent() {
    // AC: `tape stats --with-cost` with `pricing.pricing_file:
    // <good>` in `.taperc` produces the same output as the same
    // command run with the `--pricing-file` flag explicit.
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());
    let good = pricing_fixture("good.toml");
    let taperc = dir.path().join(".taperc");
    std::fs::write(
        &taperc,
        format!("pricing:\n  pricing_file: {}\n", good.to_string_lossy(),),
    )
    .unwrap();

    let with_taperc = run_stats(
        dir.path(),
        &["stats", "--with-cost", cassette.to_str().unwrap()],
    );
    let with_flag = run_stats(
        dir.path(),
        &[
            "stats",
            "--with-cost",
            "--pricing-file",
            good.to_str().unwrap(),
            cassette.to_str().unwrap(),
        ],
    );
    assert!(with_taperc.status.success(), "{with_taperc:?}");
    assert!(with_flag.status.success(), "{with_flag:?}");
    assert_eq!(
        with_taperc.stdout, with_flag.stdout,
        "taperc-resolved output should byte-match flag-resolved output"
    );
}

#[test]
fn cli_flag_overrides_taperc() {
    // `--pricing-file <X>` in the CLI takes precedence; the `.taperc`
    // points at `stale.toml` so we'd see a >90-day warning if it were
    // consumed. The CLI override is `good.toml` so no warning fires.
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());
    let taperc = dir.path().join(".taperc");
    std::fs::write(
        &taperc,
        format!(
            "pricing:\n  pricing_file: {}\n",
            pricing_fixture("stale.toml").to_string_lossy(),
        ),
    )
    .unwrap();

    let out = run_stats(
        dir.path(),
        &[
            "stats",
            "--with-cost",
            "--pricing-file",
            pricing_fixture("good.toml").to_str().unwrap(),
            cassette.to_str().unwrap(),
        ],
    );
    assert!(out.status.success(), "{out:?}");
    let s = String::from_utf8(out.stdout).unwrap();
    // good.toml has last_updated 2026-04-01; stale.toml has 2024-01-01.
    // A correct override consults `good.toml` only.
    assert!(s.contains("pricing table 2026-04-01"), "{s}");
    assert!(
        !s.contains("warning:"),
        "CLI override should not surface the stale-taperc warning:\n{s}"
    );
}

#[test]
fn taperc_relative_path_resolves_to_taperc_parent_not_cwd() {
    // AC: relative paths in `.taperc` resolve against the `.taperc`'s
    // parent directory. The `.taperc` lives at <root>/.taperc and
    // references `./prices.toml`. We invoke `tape stats` from a
    // subdir; if path resolution used `cwd` the load would fail.
    let dir = tempfile::tempdir().unwrap();
    let good = std::fs::read_to_string(pricing_fixture("good.toml")).unwrap();
    std::fs::write(dir.path().join("prices.toml"), good).unwrap();
    let taperc = dir.path().join(".taperc");
    std::fs::write(&taperc, "pricing:\n  pricing_file: ./prices.toml\n").unwrap();
    let subdir = dir.path().join("subdir");
    std::fs::create_dir_all(&subdir).unwrap();
    let cassette = make_priceable_cassette(&subdir);

    let mut cmd = Command::new(binary_path());
    cmd.args(["stats", "--with-cost", cassette.to_str().unwrap()])
        .env_remove("HOME")
        .env("HOME", dir.path())
        .current_dir(&subdir);
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "relative path should resolve against .taperc's parent: {out:?}"
    );
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("pricing table 2026-04-01"), "{s}");
}

#[test]
fn taperc_bad_pricing_file_exits_two_naming_both_paths() {
    // AC: when a `PricingLoadError` fires via the `.taperc` path,
    // exit 2 and the diagnostic names *both* the `.taperc` and the
    // resolved pricing-file path so the user can find each.
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());
    let bad = pricing_fixture("bad.toml");
    let taperc = dir.path().join(".taperc");
    std::fs::write(
        &taperc,
        format!("pricing:\n  pricing_file: {}\n", bad.to_string_lossy(),),
    )
    .unwrap();

    let out = run_stats(
        dir.path(),
        &["stats", "--with-cost", cassette.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(taperc.to_string_lossy().as_ref()),
        ".taperc path must appear in diagnostic: {stderr}"
    );
    assert!(
        stderr.contains(bad.to_string_lossy().as_ref()),
        "resolved pricing-file path must appear in diagnostic: {stderr}"
    );
}

#[test]
fn taperc_without_pricing_section_falls_through_to_bundled() {
    // Missing `pricing:` block → behaves as today. The bundled
    // table's date appears in the cost-line qualifier (today's date,
    // since the table's `last_updated` was bumped in the v0.2.x
    // line). Just assert success + a `pricing table` line is
    // emitted; pinning the date would be bit-rot-prone.
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());
    let taperc = dir.path().join(".taperc");
    // Empty `.taperc` — no `pricing:` block.
    std::fs::write(&taperc, "").unwrap();

    let out = run_stats(
        dir.path(),
        &["stats", "--with-cost", cassette.to_str().unwrap()],
    );
    assert!(out.status.success(), "{out:?}");
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("pricing table 2026-"), "{s}");
    // The taperc-specific qualifier must NOT appear (no fallback was
    // consumed); the loaded-table 2026-04-01 date from good.toml
    // wasn't used.
    assert!(!s.contains("pricing table 2026-04-01"), "{s}");
}

#[test]
fn taperc_typo_in_pricing_section_exits_two() {
    // `pricing_path:` instead of `pricing_file:` → config-load fails
    // with a clear error.
    let dir = tempfile::tempdir().unwrap();
    let cassette = make_priceable_cassette(dir.path());
    let taperc = dir.path().join(".taperc");
    std::fs::write(&taperc, "pricing:\n  pricing_path: ./prices.toml\n").unwrap();

    let out = run_stats(
        dir.path(),
        &["stats", "--with-cost", cassette.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(taperc.to_string_lossy().as_ref()),
        ".taperc path must appear in diagnostic: {stderr}"
    );
}
