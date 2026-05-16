//! End-to-end coverage for `tape watch` Phase 1 (issue #250,
//! carved from #100). The polling loop itself can't be tested
//! end-to-end without flaky wall-clock waits + a way to send
//! SIGINT mid-test, so the AC's renderer + helper coverage lives
//! on the pure `tape_play::render_watch` snapshot in
//! `crates/tape-play/src/lib.rs#watch_tests`. What we cover here:
//!
//! - `tape watch --help` lists Phase 1 + #100 + the glob pattern
//!   per AC #6 (mirrors the `Cmd::Replay` / `Cmd::SelfUpdate`
//!   doc-comment convention).
//! - `tape watch` with no positional argument exits 2 (clap's
//!   standard missing-arg diagnostic). Belt-and-suspenders so a
//!   future refactor can't silently drop the positional.

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

#[test]
fn help_documents_phase_1_and_links_umbrella_issue() {
    let r = std::process::Command::new(binary_path())
        .args(["watch", "--help"])
        .output()
        .unwrap();
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let lower = stdout.to_lowercase();
    assert!(lower.contains("phase 1"), "help: {stdout}");
    assert!(lower.contains("#100"), "help: {stdout}");
    assert!(lower.contains("glob"), "help: {stdout}");
}

#[test]
fn missing_pattern_exits_two() {
    let r = std::process::Command::new(binary_path())
        .args(["watch"])
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("required") || stderr.contains("PATTERN"),
        "stderr: {stderr}"
    );
}
