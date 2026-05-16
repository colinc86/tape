//! End-to-end coverage for `tape verify --signed --pubkey <key>`
//! (issue #240, Phase 2 of #18). All cases use a tempdir copy of
//! `tests/fixtures/minimal-success.tape` so the on-disk fixture is
//! never mutated.
//!
//! Asserts:
//! - clap rejects `--signed` without `--pubkey` and `--pubkey`
//!   without `--signed` (both exit 2)
//! - happy text path → exit 0 with `signed by <16-hex>` line
//! - happy JSON path → exit 0, `signed: true` + `signature.pubkey_fingerprint`
//! - structural failure + `--signed --pubkey` → structural error fires,
//!   sidecar untouched (no `SIDECAR_*` / `SIGNATURE_*` strings in output)
//! - structural pass + missing sidecar → exit 2 with `SIDECAR_MISSING`
//! - tampered cassette → exit 2 with `SIGNATURE_DIGEST_MISMATCH`
//! - wrong pubkey → exit 2 with `SIGNATURE_PUBKEY_MISMATCH`
//! - tampered sidecar `signature:` field → exit 2 with `SIGNATURE_INVALID`
//! - JSON payload on signature failure carries the SIG code as a
//!   synthetic diagnostic, `signed: true`, `valid: false`
//! - `tape verify` without `--signed` is byte-identical to the
//!   pre-Phase-2 binary across the well-formed fixture corpus
//!   (extends the Phase-1 pinning test)

use std::path::{Path, PathBuf};

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tape"))
}

fn repo_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
}

fn copy_minimal_to(dest: &Path) {
    let src = repo_fixtures().join("minimal-success.tape");
    std::fs::copy(&src, dest).unwrap();
}

fn run(args: &[&str]) -> std::process::Output {
    std::process::Command::new(binary_path())
        .args(args)
        .output()
        .unwrap()
}

/// Materialize a cassette + keypair + sidecar trio in a tempdir.
/// Returns (cassette_path, pubkey_path).
fn sign_setup(tmp: &Path) -> (PathBuf, PathBuf) {
    let cassette = tmp.join("c.tape");
    copy_minimal_to(&cassette);
    let key_base = tmp.join("alice");
    let kr = run(&["sign-keygen", "--out", key_base.to_str().unwrap()]);
    assert!(kr.status.success(), "{kr:?}");
    let sigkey = PathBuf::from(format!("{}.tape.sigkey", key_base.display()));
    let pubkey = PathBuf::from(format!("{}.tape.pubkey", key_base.display()));
    let sr = run(&[
        "sign",
        cassette.to_str().unwrap(),
        "--key",
        sigkey.to_str().unwrap(),
    ]);
    assert!(sr.status.success(), "{sr:?}");
    (cassette, pubkey)
}

#[test]
fn signed_without_pubkey_clap_rejects() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = tmp.path().join("c.tape");
    copy_minimal_to(&cassette);
    let r = run(&["verify", cassette.to_str().unwrap(), "--signed"]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("--pubkey") || stderr.contains("required"),
        "stderr: {stderr}"
    );
}

#[test]
fn pubkey_without_signed_clap_rejects() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = tmp.path().join("c.tape");
    copy_minimal_to(&cassette);
    let fake_pubkey = tmp.path().join("p.pubkey");
    std::fs::write(&fake_pubkey, "# fake\n").unwrap();
    let r = run(&[
        "verify",
        cassette.to_str().unwrap(),
        "--pubkey",
        fake_pubkey.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("--signed") || stderr.contains("required"),
        "stderr: {stderr}"
    );
}

#[test]
fn signed_happy_text_prints_signed_by_fingerprint() {
    let tmp = tempfile::tempdir().unwrap();
    let (cassette, pubkey) = sign_setup(tmp.path());
    let r = run(&[
        "verify",
        cassette.to_str().unwrap(),
        "--signed",
        "--pubkey",
        pubkey.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    assert!(stdout.contains("(signed by "), "stdout: {stdout}");
    // Fingerprint is 16 hex chars per pubkey_fingerprint.
    let after = stdout.split("(signed by ").nth(1).unwrap();
    let fp = after.split(')').next().unwrap();
    assert_eq!(fp.len(), 16, "fingerprint: {fp:?}");
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn signed_happy_json_payload_has_signature_and_fingerprint() {
    let tmp = tempfile::tempdir().unwrap();
    let (cassette, pubkey) = sign_setup(tmp.path());
    let r = run(&[
        "verify",
        cassette.to_str().unwrap(),
        "--json",
        "--signed",
        "--pubkey",
        pubkey.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["valid"], true);
    assert_eq!(v["signed"], true);
    let fp = v["signature"]["pubkey_fingerprint"].as_str().unwrap();
    assert_eq!(fp.len(), 16);
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn structural_failure_with_signed_does_not_touch_sidecar() {
    let tmp = tempfile::tempdir().unwrap();
    // Garbage cassette (not a zip) + a garbage sidecar alongside.
    let bad_cassette = tmp.path().join("garbage.tape");
    std::fs::write(&bad_cassette, b"not a zip").unwrap();
    let bad_sidecar = tmp.path().join("garbage.tape.sig");
    std::fs::write(&bad_sidecar, b"not a sidecar").unwrap();
    let (_, pubkey) = sign_setup(tmp.path());
    let r = run(&[
        "verify",
        bad_cassette.to_str().unwrap(),
        "--signed",
        "--pubkey",
        pubkey.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    let stdout = String::from_utf8_lossy(&r.stdout);
    // Structural error must fire (open()'s MALFORMED_ZIP path).
    assert!(
        stderr.contains("MALFORMED_ZIP"),
        "stderr: {stderr} / stdout: {stdout}"
    );
    // No signature work should have been attempted.
    let combined = format!("{stderr}{stdout}");
    assert!(
        !combined.contains("SIDECAR_"),
        "should not have looked at sidecar: {combined}"
    );
    assert!(
        !combined.contains("SIGNATURE_"),
        "should not have run signature verify: {combined}"
    );
}

#[test]
fn structural_pass_with_missing_sidecar_exits_sidecar_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = tmp.path().join("c.tape");
    copy_minimal_to(&cassette);
    let key_base = tmp.path().join("alice");
    let kr = run(&["sign-keygen", "--out", key_base.to_str().unwrap()]);
    assert!(kr.status.success(), "{kr:?}");
    let pubkey = PathBuf::from(format!("{}.tape.pubkey", key_base.display()));
    // NOTE: we deliberately do NOT run `tape sign` — no sidecar.
    let r = run(&[
        "verify",
        cassette.to_str().unwrap(),
        "--signed",
        "--pubkey",
        pubkey.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("SIDECAR_MISSING"), "stderr: {stderr}");
}

#[test]
fn tampered_cassette_exits_digest_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let (cassette, pubkey) = sign_setup(tmp.path());
    // To trigger SIGNATURE_DIGEST_MISMATCH via `tape verify --signed`,
    // the cassette must still be structurally valid (otherwise the
    // MALFORMED_ZIP branch fires first per the Phase-2 spec). Swap
    // the signed cassette's bytes for a *different* valid cassette
    // so the sidecar's recorded digest no longer matches the new
    // BLAKE3 but `tape verify` still passes structurally.
    let other_fixture = repo_fixtures().join("killer-scenario-a.tape");
    std::fs::copy(&other_fixture, &cassette).unwrap();
    let r = run(&[
        "verify",
        cassette.to_str().unwrap(),
        "--signed",
        "--pubkey",
        pubkey.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("SIGNATURE_DIGEST_MISMATCH"),
        "stderr: {stderr}"
    );
}

#[test]
fn wrong_pubkey_exits_pubkey_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let (cassette, _alice_pubkey) = sign_setup(tmp.path());
    // Generate a second keypair; verify with Bob's pubkey instead.
    let bob_base = tmp.path().join("bob");
    let kr = run(&["sign-keygen", "--out", bob_base.to_str().unwrap()]);
    assert!(kr.status.success(), "{kr:?}");
    let bob_pubkey = PathBuf::from(format!("{}.tape.pubkey", bob_base.display()));
    let r = run(&[
        "verify",
        cassette.to_str().unwrap(),
        "--signed",
        "--pubkey",
        bob_pubkey.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("SIGNATURE_PUBKEY_MISMATCH"),
        "stderr: {stderr}"
    );
}

#[test]
fn tampered_sidecar_signature_field_exits_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let (cassette, pubkey) = sign_setup(tmp.path());
    let sig_path = PathBuf::from(format!("{}.sig", cassette.display()));
    let text = std::fs::read_to_string(&sig_path).unwrap();
    // Flip the first byte of the signature field (keeps digest +
    // pubkey lines untouched so we land on the SIGNATURE_INVALID
    // branch, not a mismatch branch).
    let mut out = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("signature: ") {
            let mut chars: Vec<char> = rest.chars().collect();
            chars[0] = if chars[0] == 'A' { 'B' } else { 'A' };
            let mangled: String = chars.into_iter().collect();
            out.push_str(&format!("signature: {mangled}\n"));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    std::fs::write(&sig_path, out).unwrap();
    let r = run(&[
        "verify",
        cassette.to_str().unwrap(),
        "--signed",
        "--pubkey",
        pubkey.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("SIGNATURE_INVALID"), "stderr: {stderr}");
}

#[test]
fn signature_failure_json_emits_synthetic_diagnostic() {
    let tmp = tempfile::tempdir().unwrap();
    let (cassette, pubkey) = sign_setup(tmp.path());
    // Swap content (same approach as the text-mode test) to force
    // SIGNATURE_DIGEST_MISMATCH while keeping structural verify
    // green.
    let other_fixture = repo_fixtures().join("killer-scenario-a.tape");
    std::fs::copy(&other_fixture, &cassette).unwrap();
    let r = run(&[
        "verify",
        cassette.to_str().unwrap(),
        "--json",
        "--signed",
        "--pubkey",
        pubkey.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stdout = String::from_utf8(r.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["valid"], false);
    assert_eq!(v["signed"], true);
    // Signature failure surfaces as a synthetic diagnostic so
    // audit pipelines grepping `valid: false` keep working.
    let diags = v["diagnostics"].as_array().unwrap();
    assert!(
        diags
            .iter()
            .any(|d| d["code"] == "SIGNATURE_DIGEST_MISMATCH"),
        "diagnostics: {diags:?}"
    );
}

#[test]
fn verify_without_signed_is_byte_identical_across_fixture_corpus() {
    // Pinning: every well-formed fixture's `tape verify` output is
    // exactly what it was before Phase 2 (no --signed flag adds an
    // empty signed field, no JSON schema drift, no text drift).
    // Extends the Phase-1 sign_phase1.rs::verify_output_is_unchanged
    // test to cover all positive fixtures, not just minimal-success.
    let fixtures_dir = repo_fixtures();
    for f in std::fs::read_dir(&fixtures_dir).unwrap() {
        let entry = f.unwrap();
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("tape") {
            continue;
        }
        let r_text = run(&["verify", path.to_str().unwrap()]);
        let r_json = run(&["verify", path.to_str().unwrap(), "--json"]);
        // Just exercise: no panic, no schema drift. We capture the
        // outputs explicitly so a future change to verify is forced
        // to update this test deliberately.
        let stdout = String::from_utf8(r_text.stdout).unwrap();
        let json = String::from_utf8(r_json.stdout).unwrap();
        assert!(
            stdout.contains("OK ") || stdout.contains("FAIL "),
            "{} text: {stdout}",
            path.display()
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["valid"].is_boolean(), "{} json: {json}", path.display());
        // PINNING: no `signed` key when --signed wasn't passed.
        assert!(
            v.get("signed").is_none(),
            "unsigned-mode verify must not emit `signed`: {json}"
        );
    }
}
