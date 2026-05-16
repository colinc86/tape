//! End-to-end coverage for `tape encrypt --recipient` /
//! `tape decrypt --identity` / `tape encrypt-keygen` (issue #248,
//! Phase 2 of #89). All cases use tempdir copies of
//! `tests/fixtures/minimal-success.tape` so the on-disk fixture is
//! never mutated.
//!
//! Asserts:
//! - `encrypt-keygen` writes both files with expected suffixes +
//!   fingerprint (recipient bech32) on stderr; refuses overwrite
//! - round-trip via `--recipient <pub>` / `--identity <key>` recovers
//!   byte-identical plaintext
//! - decrypt with the wrong identity → exit 2 with `DECRYPT_FAILED`
//! - decrypt `--identity` on a passphrase-encrypted file →
//!   `DECRYPT_FAILED` with the mismatch-kind hint
//! - decrypt `--passphrase-stdin` on a recipient-encrypted file →
//!   `DECRYPT_FAILED` with the mismatch-kind hint (live replacement
//!   of the Phase-1 stub)
//! - clap conflict: `--recipient` + `--passphrase` rejected
//! - `--recipient age1…` (bare bech32, no file) works equally well
//! - `--help` documents Phase 2 + recipient/identity flags

use std::io::Write;
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

fn run_with_stdin(args: &[&str], stdin_body: &[u8]) -> std::process::Output {
    let mut child = std::process::Command::new(binary_path())
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn tape");
    {
        let stdin = child.stdin.as_mut().expect("child stdin");
        stdin.write_all(stdin_body).expect("write stdin");
    }
    child.wait_with_output().expect("wait")
}

/// Materialize a keypair in `dir` via `tape encrypt-keygen`.
/// Returns `(agekey_path, agepub_path)`.
fn keygen(dir: &Path, base: &str) -> (PathBuf, PathBuf) {
    let out = dir.join(base);
    let r = run(&["encrypt-keygen", "--out", out.to_str().unwrap()]);
    assert!(r.status.success(), "keygen failed: {r:?}");
    (
        PathBuf::from(format!("{}.tape.agekey", out.display())),
        PathBuf::from(format!("{}.tape.agepub", out.display())),
    )
}

#[test]
fn encrypt_keygen_writes_both_files_with_recipient_bech32() {
    let tmp = tempfile::tempdir().unwrap();
    let (agekey, agepub) = keygen(tmp.path(), "alice");
    assert!(agekey.exists(), "expected {}", agekey.display());
    assert!(agepub.exists(), "expected {}", agepub.display());
    // Pubkey file should carry an `age1…` bech32 line.
    let pub_text = std::fs::read_to_string(&agepub).unwrap();
    assert!(
        pub_text.lines().any(|l| l.starts_with("age1")),
        "agepub missing `age1…` line: {pub_text}"
    );
    // Keyfile should carry an `AGE-SECRET-KEY-1…` bech32 line.
    let key_text = std::fs::read_to_string(&agekey).unwrap();
    assert!(
        key_text.lines().any(|l| l.starts_with("AGE-SECRET-KEY-1")),
        "agekey missing secret bech32: <redacted>"
    );
}

#[test]
fn encrypt_keygen_refuses_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let _ = keygen(tmp.path(), "alice");
    let out = tmp.path().join("alice");
    let r = run(&["encrypt-keygen", "--out", out.to_str().unwrap()]);
    assert!(!r.status.success(), "second keygen should fail: {r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("refusing to overwrite"), "stderr: {stderr}");
}

#[test]
fn recipient_round_trip_via_file_path_recovers_identical_plaintext() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let original = std::fs::read(&plaintext).unwrap();
    let envelope = tmp.path().join("c.tape.age");

    let (agekey, agepub) = keygen(tmp.path(), "alice");

    // Encrypt to Alice's pubkey.
    let enc = run(&[
        "encrypt",
        plaintext.to_str().unwrap(),
        "--recipient",
        agepub.to_str().unwrap(),
    ]);
    assert!(enc.status.success(), "encrypt failed: {enc:?}");
    assert!(envelope.exists());

    // Decrypt with Alice's identity.
    let recovered = tmp.path().join("recovered.tape");
    let dec = run(&[
        "decrypt",
        envelope.to_str().unwrap(),
        "--identity",
        agekey.to_str().unwrap(),
        "--output",
        recovered.to_str().unwrap(),
    ]);
    assert!(dec.status.success(), "decrypt failed: {dec:?}");
    assert_eq!(original, std::fs::read(&recovered).unwrap());
}

#[test]
fn recipient_round_trip_via_bare_bech32_recipient_works() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let envelope = tmp.path().join("c.tape.age");

    let (agekey, agepub) = keygen(tmp.path(), "alice");
    // Pull the bare `age1…` line out of the pubkey file and pass
    // it directly as the --recipient value.
    let pub_text = std::fs::read_to_string(&agepub).unwrap();
    let bech = pub_text
        .lines()
        .find(|l| l.starts_with("age1"))
        .unwrap()
        .to_owned();

    let enc = run(&["encrypt", plaintext.to_str().unwrap(), "--recipient", &bech]);
    assert!(enc.status.success(), "encrypt failed: {enc:?}");

    let recovered = tmp.path().join("recovered.tape");
    let dec = run(&[
        "decrypt",
        envelope.to_str().unwrap(),
        "--identity",
        agekey.to_str().unwrap(),
        "--output",
        recovered.to_str().unwrap(),
    ]);
    assert!(dec.status.success(), "{dec:?}");
}

#[test]
fn decrypt_with_wrong_identity_exits_two_with_decrypt_failed() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let envelope = tmp.path().join("c.tape.age");
    let (_alice_key, alice_pub) = keygen(tmp.path(), "alice");
    let (bob_key, _bob_pub) = keygen(tmp.path(), "bob");

    let enc = run(&[
        "encrypt",
        plaintext.to_str().unwrap(),
        "--recipient",
        alice_pub.to_str().unwrap(),
    ]);
    assert!(enc.status.success(), "{enc:?}");

    // Try to decrypt with Bob's identity though Alice was the recipient.
    let out = tmp.path().join("nope.tape");
    let dec = run(&[
        "decrypt",
        envelope.to_str().unwrap(),
        "--identity",
        bob_key.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(dec.status.code(), Some(2), "{dec:?}");
    let stderr = String::from_utf8_lossy(&dec.stderr);
    assert!(stderr.contains("DECRYPT_FAILED"), "stderr: {stderr}");
}

#[test]
fn decrypt_identity_on_passphrase_envelope_exits_two_with_mismatch_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let envelope = tmp.path().join("c.tape.age");

    // Encrypt with a passphrase (Phase-1 mode).
    let enc = run_with_stdin(
        &["encrypt", plaintext.to_str().unwrap(), "--passphrase-stdin"],
        b"correct horse battery staple\n",
    );
    assert!(enc.status.success(), "{enc:?}");

    // Try to decrypt with an X25519 identity → kind mismatch.
    let (agekey, _agepub) = keygen(tmp.path(), "alice");
    let out = tmp.path().join("nope.tape");
    let dec = run(&[
        "decrypt",
        envelope.to_str().unwrap(),
        "--identity",
        agekey.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(dec.status.code(), Some(2), "{dec:?}");
    let stderr = String::from_utf8_lossy(&dec.stderr);
    assert!(stderr.contains("DECRYPT_FAILED"), "stderr: {stderr}");
    assert!(
        stderr.contains("passphrase"),
        "stderr should hint at passphrase mode: {stderr}"
    );
}

#[test]
fn decrypt_passphrase_on_recipient_envelope_exits_two_with_mismatch_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let envelope = tmp.path().join("c.tape.age");
    let (_agekey, agepub) = keygen(tmp.path(), "alice");

    let enc = run(&[
        "encrypt",
        plaintext.to_str().unwrap(),
        "--recipient",
        agepub.to_str().unwrap(),
    ]);
    assert!(enc.status.success(), "{enc:?}");

    // Try to decrypt with a passphrase → kind mismatch.
    let out = tmp.path().join("nope.tape");
    let dec = run_with_stdin(
        &[
            "decrypt",
            envelope.to_str().unwrap(),
            "--passphrase-stdin",
            "--output",
            out.to_str().unwrap(),
        ],
        b"any-passphrase\n",
    );
    assert_eq!(dec.status.code(), Some(2), "{dec:?}");
    let stderr = String::from_utf8_lossy(&dec.stderr);
    assert!(stderr.contains("DECRYPT_FAILED"), "stderr: {stderr}");
    assert!(
        stderr.contains("--identity"),
        "stderr should hint at --identity: {stderr}"
    );
}

#[test]
fn encrypt_recipient_and_passphrase_both_supplied_is_clap_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let (_agekey, agepub) = keygen(tmp.path(), "alice");
    let r = run(&[
        "encrypt",
        plaintext.to_str().unwrap(),
        "--passphrase-stdin",
        "--recipient",
        agepub.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
}

#[test]
fn decrypt_identity_and_passphrase_both_supplied_is_clap_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let envelope = tmp.path().join("c.tape.age");
    let (agekey, agepub) = keygen(tmp.path(), "alice");
    let enc = run(&[
        "encrypt",
        plaintext.to_str().unwrap(),
        "--recipient",
        agepub.to_str().unwrap(),
    ]);
    assert!(enc.status.success(), "{enc:?}");

    let out = tmp.path().join("out.tape");
    let r = run(&[
        "decrypt",
        envelope.to_str().unwrap(),
        "--passphrase-stdin",
        "--identity",
        agekey.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
}

#[test]
fn help_documents_phase_2_recipient_and_identity() {
    for sub in ["encrypt", "decrypt", "encrypt-keygen"] {
        let r = run(&[sub, "--help"]);
        assert!(r.status.success(), "{sub} --help failed: {r:?}");
        let stdout = String::from_utf8(r.stdout).unwrap();
        let lower = stdout.to_lowercase();
        match sub {
            "encrypt" => {
                assert!(lower.contains("--recipient"), "{sub} --help: {stdout}");
                assert!(lower.contains("phase 2"), "{sub} --help: {stdout}");
            }
            "decrypt" => {
                assert!(lower.contains("--identity"), "{sub} --help: {stdout}");
                assert!(lower.contains("phase 2"), "{sub} --help: {stdout}");
            }
            "encrypt-keygen" => {
                assert!(lower.contains("x25519"), "{sub} --help: {stdout}");
                assert!(
                    lower.contains("agekey") && lower.contains("agepub"),
                    "{sub} --help: {stdout}"
                );
            }
            _ => unreachable!(),
        }
    }
}
