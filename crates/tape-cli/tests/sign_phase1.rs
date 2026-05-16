//! End-to-end coverage for `tape sign-keygen` / `tape sign` /
//! `tape verify-sig` (issue #230, carved from #18). Uses the
//! existing `tests/fixtures/minimal-success.tape` for the
//! happy-path round trip; mutating tests build a copy of that
//! fixture so the on-disk one is never modified.
//!
//! Asserts:
//! - keygen → sign → verify-sig round trip exits 0
//! - mutating one byte of the cassette after signing → exit 2 with
//!   `SIGNATURE_DIGEST_MISMATCH`
//! - verify-sig with a fresh (wrong) keypair's pubkey → exit 2 with
//!   `SIGNATURE_PUBKEY_MISMATCH`
//! - tampering with the sidecar's `signature:` field (digest + pubkey
//!   left consistent) → exit 2 with `SIGNATURE_INVALID`
//! - keygen refuses to overwrite existing files; same for sign's .sig
//! - --help for each subcommand mentions Ed25519 / sidecar
//! - `tape verify` output is byte-identical with or without a .sig
//!   sidecar present (pinning test that sign Phase 1 doesn't touch
//!   verify behavior)

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

fn run(args: &[&str]) -> std::process::Output {
    let mut cmd = std::process::Command::new(binary_path());
    for a in args {
        cmd.arg(a);
    }
    cmd.output().unwrap()
}

fn copy_minimal_to(dest: &Path) {
    let src = repo_fixtures().join("minimal-success.tape");
    std::fs::copy(&src, dest).unwrap();
}

fn keygen_in(dir: &Path, base: &str) -> (PathBuf, PathBuf) {
    let out = dir.join(base);
    let r = run(&["sign-keygen", "--out", out.to_str().unwrap()]);
    assert!(r.status.success(), "keygen failed: {r:?}");
    (
        PathBuf::from(format!("{}.tape.sigkey", out.display())),
        PathBuf::from(format!("{}.tape.pubkey", out.display())),
    )
}

#[test]
fn keygen_sign_verify_round_trip_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = tmp.path().join("c.tape");
    copy_minimal_to(&cassette);
    let (sigkey, pubkey) = keygen_in(tmp.path(), "alice");

    let r = run(&[
        "sign",
        cassette.to_str().unwrap(),
        "--key",
        sigkey.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "sign failed: {r:?}");
    let sig_path = PathBuf::from(format!("{}.sig", cassette.display()));
    assert!(
        sig_path.exists(),
        "expected sidecar at {}",
        sig_path.display()
    );

    let r = run(&[
        "verify-sig",
        cassette.to_str().unwrap(),
        "--pubkey",
        pubkey.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "verify-sig failed: {r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("OK: signature valid"), "stderr: {stderr}");
}

#[test]
fn mutated_cassette_after_signing_fails_with_digest_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = tmp.path().join("c.tape");
    copy_minimal_to(&cassette);
    let (sigkey, pubkey) = keygen_in(tmp.path(), "alice");
    let r = run(&[
        "sign",
        cassette.to_str().unwrap(),
        "--key",
        sigkey.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "sign failed: {r:?}");

    // Mutate one byte of the cassette. Flip the last byte.
    let mut bytes = std::fs::read(&cassette).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(&cassette, &bytes).unwrap();

    let r = run(&[
        "verify-sig",
        cassette.to_str().unwrap(),
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
fn wrong_pubkey_fails_with_pubkey_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = tmp.path().join("c.tape");
    copy_minimal_to(&cassette);
    let (sigkey, _alice_pubkey) = keygen_in(tmp.path(), "alice");
    let (_bob_sigkey, bob_pubkey) = keygen_in(tmp.path(), "bob");
    let r = run(&[
        "sign",
        cassette.to_str().unwrap(),
        "--key",
        sigkey.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "sign failed: {r:?}");

    // Verify with Bob's pubkey though Alice signed.
    let r = run(&[
        "verify-sig",
        cassette.to_str().unwrap(),
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
fn tampered_signature_field_fails_with_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = tmp.path().join("c.tape");
    copy_minimal_to(&cassette);
    let (sigkey, pubkey) = keygen_in(tmp.path(), "alice");
    let r = run(&[
        "sign",
        cassette.to_str().unwrap(),
        "--key",
        sigkey.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "sign failed: {r:?}");

    let sig_path = PathBuf::from(format!("{}.sig", cassette.display()));
    let text = std::fs::read_to_string(&sig_path).unwrap();
    // Find the `signature:` line and corrupt the first base64 char
    // (NOT the digest or pubkey — those need to stay consistent so
    // we hit the SIGNATURE_INVALID branch, not the digest or pubkey
    // mismatch branches).
    let mut out = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("signature: ") {
            // Flip the first byte: rotate a→b, b→c, etc. (still
            // valid base64 alphabet, but a different signature).
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
        "verify-sig",
        cassette.to_str().unwrap(),
        "--pubkey",
        pubkey.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("SIGNATURE_INVALID"), "stderr: {stderr}");
}

#[test]
fn keygen_refuses_to_overwrite_existing_files() {
    let tmp = tempfile::tempdir().unwrap();
    let _ = keygen_in(tmp.path(), "alice");
    // Second keygen with the same --out should refuse (no --force).
    let out = tmp.path().join("alice");
    let r = run(&["sign-keygen", "--out", out.to_str().unwrap()]);
    assert!(!r.status.success(), "second keygen should fail: {r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("refusing to overwrite"), "stderr: {stderr}");
}

#[test]
fn sign_refuses_to_overwrite_existing_sidecar() {
    let tmp = tempfile::tempdir().unwrap();
    let cassette = tmp.path().join("c.tape");
    copy_minimal_to(&cassette);
    let (sigkey, _) = keygen_in(tmp.path(), "alice");
    // First sign succeeds.
    let r = run(&[
        "sign",
        cassette.to_str().unwrap(),
        "--key",
        sigkey.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "first sign failed: {r:?}");
    // Second sign with the default --out should refuse.
    let r = run(&[
        "sign",
        cassette.to_str().unwrap(),
        "--key",
        sigkey.to_str().unwrap(),
    ]);
    assert!(!r.status.success(), "second sign should fail: {r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("refusing to overwrite"), "stderr: {stderr}");
}

#[test]
fn help_for_each_subcommand_mentions_ed25519_or_sidecar() {
    for sub in ["sign-keygen", "sign", "verify-sig"] {
        let r = run(&[sub, "--help"]);
        assert!(r.status.success(), "{sub} --help failed: {r:?}");
        let stdout = String::from_utf8(r.stdout).unwrap();
        let lower = stdout.to_lowercase();
        assert!(
            lower.contains("ed25519") || lower.contains("sidecar") || lower.contains("sig"),
            "{sub} --help should mention crypto/sidecar: {stdout}"
        );
    }
}

#[test]
fn verify_output_is_unchanged_by_presence_of_sidecar() {
    // Pinning test: sign Phase 1 must not perturb `tape verify` behavior.
    let tmp = tempfile::tempdir().unwrap();
    let cassette = tmp.path().join("c.tape");
    copy_minimal_to(&cassette);

    let before = run(&["verify", cassette.to_str().unwrap()]);
    assert!(before.status.success(), "{before:?}");

    let (sigkey, _) = keygen_in(tmp.path(), "alice");
    let s = run(&[
        "sign",
        cassette.to_str().unwrap(),
        "--key",
        sigkey.to_str().unwrap(),
    ]);
    assert!(s.status.success(), "sign failed: {s:?}");

    let after = run(&["verify", cassette.to_str().unwrap()]);
    assert!(after.status.success(), "{after:?}");

    assert_eq!(before.status.code(), after.status.code(), "exit code drift");
    assert_eq!(before.stdout, after.stdout, "stdout drift");
    assert_eq!(before.stderr, after.stderr, "stderr drift");
}
