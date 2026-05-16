//! End-to-end coverage for `tape encrypt` / `tape decrypt` Phase 1
//! (issue #238, carved from #89). All cases use
//! `--passphrase-stdin` so no TTY is involved — CI must be able to
//! run these without a controlling terminal.
//!
//! Asserts:
//! - round-trip via the canonical `minimal-success.tape` fixture →
//!   recovered plaintext is byte-identical to the input
//! - wrong passphrase on decrypt → exit 2 with `DECRYPT_FAILED`
//! - both `--passphrase` and `--passphrase-stdin` supplied →
//!   clap rejects (exit 2)
//! - `--passphrase-stdin` produced output exists at `<input>.age` by
//!   default (encrypt) and at the suffix-stripped path (decrypt)
//! - refuse-overwrite path (encrypt) without `--force`
//! - default-output path on decrypt requires the input to end in
//!   `.age` (without `--output`) — non-`.age` input exits 2
//! - `--help` for both subcommands mentions Phase 1 + passphrase
//! - `tape verify` byte-identity pinning: a plaintext cassette's
//!   verify output is unchanged regardless of whether a sibling
//!   `.age` envelope exists (Phase 1 must not touch verify)

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

fn run(args: &[&str]) -> std::process::Output {
    std::process::Command::new(binary_path())
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn round_trip_via_passphrase_stdin_recovers_identical_plaintext() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let original = std::fs::read(&plaintext).unwrap();
    let envelope = tmp.path().join("c.tape.age");

    // Encrypt with the default --output path: <cassette>.age.
    let enc = run_with_stdin(
        &["encrypt", plaintext.to_str().unwrap(), "--passphrase-stdin"],
        b"correct horse battery staple\n",
    );
    assert!(enc.status.success(), "{enc:?}");
    assert!(envelope.exists(), "expected {}", envelope.display());

    // Decrypt back with the same passphrase to a different path so
    // the test can compare byte-for-byte.
    let recovered = tmp.path().join("recovered.tape");
    let dec = run_with_stdin(
        &[
            "decrypt",
            envelope.to_str().unwrap(),
            "--passphrase-stdin",
            "--output",
            recovered.to_str().unwrap(),
        ],
        b"correct horse battery staple\n",
    );
    assert!(dec.status.success(), "{dec:?}");

    let got = std::fs::read(&recovered).unwrap();
    assert_eq!(original, got, "decrypted bytes differ from input");
}

#[test]
fn decrypt_with_wrong_passphrase_exits_two_with_decrypt_failed() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let envelope = tmp.path().join("c.tape.age");

    let enc = run_with_stdin(
        &["encrypt", plaintext.to_str().unwrap(), "--passphrase-stdin"],
        b"rightpw\n",
    );
    assert!(enc.status.success(), "{enc:?}");

    let out = tmp.path().join("nope.tape");
    let dec = run_with_stdin(
        &[
            "decrypt",
            envelope.to_str().unwrap(),
            "--passphrase-stdin",
            "--output",
            out.to_str().unwrap(),
        ],
        b"WRONGpw\n",
    );
    assert_eq!(dec.status.code(), Some(2), "{dec:?}");
    let stderr = String::from_utf8_lossy(&dec.stderr);
    assert!(stderr.contains("DECRYPT_FAILED"), "stderr: {stderr}");
}

#[test]
fn passphrase_and_passphrase_stdin_both_supplied_is_clap_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let r = run(&[
        "encrypt",
        plaintext.to_str().unwrap(),
        "--passphrase",
        "--passphrase-stdin",
    ]);
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("conflict") || stderr.contains("cannot be used with"),
        "stderr: {stderr}"
    );
}

#[test]
fn encrypt_refuses_overwrite_without_force() {
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let envelope = tmp.path().join("c.tape.age");
    // Pre-create the envelope path.
    std::fs::write(&envelope, b"existing").unwrap();
    let r = run_with_stdin(
        &["encrypt", plaintext.to_str().unwrap(), "--passphrase-stdin"],
        b"pw\n",
    );
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(stderr.contains("already exists"), "stderr: {stderr}");
    // Existing content untouched.
    assert_eq!(std::fs::read(&envelope).unwrap(), b"existing");

    // With --force, the encrypt succeeds and replaces the file.
    let r2 = run_with_stdin(
        &[
            "encrypt",
            plaintext.to_str().unwrap(),
            "--passphrase-stdin",
            "--force",
        ],
        b"pw\n",
    );
    assert!(r2.status.success(), "{r2:?}");
    let after = std::fs::read(&envelope).unwrap();
    assert_ne!(after, b"existing", "encrypt should have replaced the file");
}

#[test]
fn decrypt_without_output_and_without_age_suffix_exits_two() {
    let tmp = tempfile::tempdir().unwrap();
    // A real age envelope but renamed so it doesn't end in `.age`.
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let envelope = tmp.path().join("c.tape.age");
    let enc = run_with_stdin(
        &["encrypt", plaintext.to_str().unwrap(), "--passphrase-stdin"],
        b"pw\n",
    );
    assert!(enc.status.success(), "{enc:?}");
    // Rename to a non-.age suffix.
    let renamed = tmp.path().join("encrypted.bin");
    std::fs::rename(&envelope, &renamed).unwrap();

    let r = run_with_stdin(
        &["decrypt", renamed.to_str().unwrap(), "--passphrase-stdin"],
        b"pw\n",
    );
    assert_eq!(r.status.code(), Some(2), "{r:?}");
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("does not end in `.age`"),
        "stderr: {stderr}"
    );
}

#[test]
fn help_documents_phase_1_and_passphrase() {
    for sub in ["encrypt", "decrypt"] {
        let r = run(&[sub, "--help"]);
        assert!(r.status.success(), "{sub} --help failed: {r:?}");
        let stdout = String::from_utf8(r.stdout).unwrap();
        let lower = stdout.to_lowercase();
        assert!(lower.contains("phase 1"), "{sub} --help: {stdout}");
        assert!(lower.contains("passphrase"), "{sub} --help: {stdout}");
    }
}

#[test]
fn verify_byte_identity_with_sibling_age_envelope() {
    // Pinning: `tape verify <plaintext.tape>` output must be
    // byte-identical regardless of whether a sibling `.age`
    // envelope exists. Encrypt Phase 1 must not perturb verify.
    let tmp = tempfile::tempdir().unwrap();
    let plaintext = tmp.path().join("c.tape");
    copy_minimal_to(&plaintext);
    let before = run(&["verify", plaintext.to_str().unwrap()]);
    assert!(before.status.success(), "{before:?}");

    let enc = run_with_stdin(
        &["encrypt", plaintext.to_str().unwrap(), "--passphrase-stdin"],
        b"pw\n",
    );
    assert!(enc.status.success(), "{enc:?}");

    let after = run(&["verify", plaintext.to_str().unwrap()]);
    assert_eq!(before.status.code(), after.status.code(), "exit drift");
    assert_eq!(before.stdout, after.stdout, "stdout drift");
    assert_eq!(before.stderr, after.stderr, "stderr drift");
}
