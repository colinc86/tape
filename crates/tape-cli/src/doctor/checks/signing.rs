//! `signing.*` — keystore + trust-store readability checks. Issue
//! #166 / Step-3 of #81.
//!
//! All three checks are `Warn`-severity: signing is opt-in
//! (`tape sign` is `priority:later` per #18 at the time of writing),
//! so absence of `~/.tape/keys/` is the "feature not in use" branch
//! and reports `Na`. When the keystore is present these checks act as
//! a tripwire for over-permissive directory / file modes — the kind
//! of subtle install drift that's easy to miss when a user copies a
//! keystore across machines or unzips one from a backup.
//!
//! Mode comparisons are `#[cfg(unix)]`-gated. On non-Unix targets
//! (`tape sign` won't initially ship there anyway), the checks
//! degrade to existence-only and surface `Pass` whenever the path is
//! a directory / file, with no mode assertion.

use std::path::{Path, PathBuf};

use super::super::check::{Check, CheckOutcome, Env, Severity};

const KEYSTORE_DIR: &str = ".tape/keys";
const TRUST_STORE_DIR: &str = ".tape/trust";
const KEYSTORE_DIR_MODE: u32 = 0o700;
const KEY_FILE_MODE: u32 = 0o600;

/// Is `~/.tape/keys/` present, a directory, and at mode 0700?
///
/// `Na` when `$HOME` is unset or the directory is absent (signing
/// not in use). `Warn` when the path exists but isn't a directory,
/// or when the directory exists but has a mode that exposes private
/// keys to other users. `Pass` when the directory is at mode 0700
/// (or on non-Unix, just when it's a directory).
pub struct KeystoreReadable;
impl Check for KeystoreReadable {
    fn id(&self) -> &'static str {
        "signing.keystore.readable"
    }
    fn category(&self) -> &'static str {
        "signing"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Warn
    }
    fn description(&self) -> &'static str {
        "~/.tape/keys/ exists and is at mode 0700 (n/a when not in use)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        let Some(path) = keystore_path(env) else {
            return CheckOutcome::na("$HOME not set");
        };
        if !path.exists() {
            return CheckOutcome::na(format!(
                "{} not present (signing not in use)",
                path.display()
            ));
        }
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                return CheckOutcome::warn(format!("{} cannot be stat'd: {e}", path.display()));
            }
        };
        if !meta.is_dir() {
            return CheckOutcome::warn(format!("{} exists but is not a directory", path.display()))
                .with_fix(format!(
                    "remove the file at {} and run `tape sign init` once #18 lands",
                    path.display()
                ));
        }
        match check_mode(&meta, KEYSTORE_DIR_MODE) {
            ModeProbe::Match => {
                CheckOutcome::pass(format!("{} readable with mode 0700", path.display()))
            }
            ModeProbe::Mismatch { actual } => CheckOutcome::warn(format!(
                "{} mode is 0{actual:o} (expected 0{:o})",
                path.display(),
                KEYSTORE_DIR_MODE,
            ))
            .with_fix(format!("chmod 0700 {}", path.display())),
            #[cfg(not(unix))]
            ModeProbe::NotUnix => CheckOutcome::pass(format!(
                "{} readable (mode check skipped on this platform)",
                path.display()
            )),
        }
    }
}

/// Are all `*.key` direct children of `~/.tape/keys/` at mode 0600?
///
/// Returns `Na` when the parent is absent (same "feature not in use"
/// branch as `KeystoreReadable`) or when the directory exists but
/// contains zero `*.key` entries. `Warn` on the first over-permissive
/// `*.key` (full enumeration is a `--fix`-mode concern). `Pass` with
/// the count on a clean keystore. Non-recursive — subdirectories are
/// out of scope for this slice.
pub struct KeystorePerms;
impl Check for KeystorePerms {
    fn id(&self) -> &'static str {
        "signing.keystore.perms"
    }
    fn category(&self) -> &'static str {
        "signing"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Warn
    }
    fn description(&self) -> &'static str {
        "every *.key under ~/.tape/keys is at mode 0600 (n/a when not in use)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        let Some(path) = keystore_path(env) else {
            return CheckOutcome::na("$HOME not set");
        };
        if !path.exists() {
            return CheckOutcome::na(format!(
                "{} not present (signing not in use)",
                path.display()
            ));
        }
        let entries = match std::fs::read_dir(&path) {
            Ok(it) => it,
            Err(e) => {
                return CheckOutcome::warn(format!("{} could not be read: {e}", path.display()));
            }
        };
        let mut count: u64 = 0;
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let entry_path = entry.path();
            if !is_direct_key_file(&entry_path) {
                continue;
            }
            count += 1;
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    return CheckOutcome::warn(format!(
                        "{} cannot be stat'd: {e}",
                        entry_path.display()
                    ));
                }
            };
            match check_mode(&meta, KEY_FILE_MODE) {
                ModeProbe::Match => {}
                #[cfg(not(unix))]
                ModeProbe::NotUnix => {}
                ModeProbe::Mismatch { actual } => {
                    return CheckOutcome::warn(format!(
                        "{} mode is 0{actual:o} (expected 0{:o})",
                        entry_path.display(),
                        KEY_FILE_MODE,
                    ))
                    .with_fix(format!("chmod 0600 {}", entry_path.display()));
                }
            }
        }
        if count == 0 {
            return CheckOutcome::na(format!(
                "no *.key files under {} (signing not in use)",
                path.display()
            ));
        }
        CheckOutcome::pass(format!("{count} *.key file(s) at mode 0600"))
    }
}

/// Is `~/.tape/trust/` present and a directory?
///
/// `Na` when `$HOME` is unset or the directory is absent (no
/// configured trusted publishers). `Warn` when the path exists but
/// isn't a directory. `Pass` when it's a directory. No mode
/// requirement — trust lists are public.
pub struct TrustStoreReadable;
impl Check for TrustStoreReadable {
    fn id(&self) -> &'static str {
        "signing.trust_store.readable"
    }
    fn category(&self) -> &'static str {
        "signing"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Warn
    }
    fn description(&self) -> &'static str {
        "~/.tape/trust/ exists and is a directory (n/a when no trusted publishers configured)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        let Some(path) = trust_store_path(env) else {
            return CheckOutcome::na("$HOME not set");
        };
        if !path.exists() {
            return CheckOutcome::na(format!(
                "{} not present (no trusted publishers configured)",
                path.display()
            ));
        }
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                return CheckOutcome::warn(format!("{} cannot be stat'd: {e}", path.display()));
            }
        };
        if meta.is_dir() {
            CheckOutcome::pass(format!("{} readable", path.display()))
        } else {
            CheckOutcome::warn(format!("{} exists but is not a directory", path.display()))
        }
    }
}

fn keystore_path(env: &Env) -> Option<PathBuf> {
    env.home.as_ref().map(|h| h.join(KEYSTORE_DIR))
}

fn trust_store_path(env: &Env) -> Option<PathBuf> {
    env.home.as_ref().map(|h| h.join(TRUST_STORE_DIR))
}

/// `true` when `path` is a regular `*.key` file (not a directory,
/// not a symlink-to-directory) directly under the keystore. The
/// `*.key` filter is case-sensitive, matching the keystore's
/// canonical lower-case naming.
fn is_direct_key_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    path.extension().is_some_and(|e| e == "key")
}

/// Result of comparing a path's mode bits against an expected value.
/// `NotUnix` is the platform-degraded branch — the caller treats it
/// as a soft pass so the check still reports something useful on
/// targets where `tape sign` doesn't ship. The `NotUnix` arm is
/// `#[cfg(not(unix))]`-only so the Unix build doesn't trip the
/// dead-code lint.
enum ModeProbe {
    Match,
    Mismatch {
        actual: u32,
    },
    #[cfg(not(unix))]
    NotUnix,
}

#[cfg(unix)]
fn check_mode(meta: &std::fs::Metadata, expected: u32) -> ModeProbe {
    use std::os::unix::fs::PermissionsExt;
    let actual = meta.permissions().mode() & 0o777;
    if actual == expected {
        ModeProbe::Match
    } else {
        ModeProbe::Mismatch { actual }
    }
}

#[cfg(not(unix))]
fn check_mode(_meta: &std::fs::Metadata, _expected: u32) -> ModeProbe {
    ModeProbe::NotUnix
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::super::super::check::Status;
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn env_with_home(home: Option<PathBuf>) -> Env {
        Env {
            home,
            tmpdir: PathBuf::from("/tmp"),
            path_dirs: vec![],
            cwd: PathBuf::from("."),
            compile_time_version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn set_mode(path: &Path, mode: u32) {
        let mut p = std::fs::metadata(path).unwrap().permissions();
        p.set_mode(mode);
        std::fs::set_permissions(path, p).unwrap();
    }

    // --- KeystoreReadable -------------------------------------------

    #[test]
    fn keystore_readable_na_when_home_unset() {
        let env = env_with_home(None);
        let out = KeystoreReadable.run(&env);
        assert_eq!(out.status, Status::Na);
        assert!(out.message.contains("$HOME"));
    }

    #[test]
    fn keystore_readable_na_when_dir_absent() {
        let home = tempfile::tempdir().unwrap();
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = KeystoreReadable.run(&env);
        assert_eq!(out.status, Status::Na);
        assert!(out.message.contains("signing not in use"));
    }

    #[test]
    fn keystore_readable_pass_at_0700() {
        let home = tempfile::tempdir().unwrap();
        let keys = home.path().join(KEYSTORE_DIR);
        std::fs::create_dir_all(&keys).unwrap();
        set_mode(&keys, 0o700);
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = KeystoreReadable.run(&env);
        assert_eq!(out.status, Status::Pass, "{out:?}");
        assert!(out.message.contains("0700"));
    }

    #[test]
    fn keystore_readable_warn_at_0755() {
        let home = tempfile::tempdir().unwrap();
        let keys = home.path().join(KEYSTORE_DIR);
        std::fs::create_dir_all(&keys).unwrap();
        set_mode(&keys, 0o755);
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = KeystoreReadable.run(&env);
        assert_eq!(out.status, Status::Warn, "{out:?}");
        assert!(out.message.contains("0755"));
        assert!(out.message.contains("0700"));
        assert!(
            out.suggested_fix
                .as_deref()
                .is_some_and(|s| s.contains("chmod 0700")),
            "fix should suggest chmod: {out:?}"
        );
    }

    #[test]
    fn keystore_readable_warn_when_path_is_file() {
        let home = tempfile::tempdir().unwrap();
        let keys = home.path().join(".tape");
        std::fs::create_dir_all(&keys).unwrap();
        std::fs::write(home.path().join(KEYSTORE_DIR), b"stray").unwrap();
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = KeystoreReadable.run(&env);
        assert_eq!(out.status, Status::Warn);
        assert!(out.message.contains("not a directory"));
    }

    // --- KeystorePerms ----------------------------------------------

    #[test]
    fn keystore_perms_na_when_dir_absent() {
        let home = tempfile::tempdir().unwrap();
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = KeystorePerms.run(&env);
        assert_eq!(out.status, Status::Na);
        assert!(out.message.contains("signing not in use"));
    }

    #[test]
    fn keystore_perms_na_when_no_key_files() {
        let home = tempfile::tempdir().unwrap();
        let keys = home.path().join(KEYSTORE_DIR);
        std::fs::create_dir_all(&keys).unwrap();
        set_mode(&keys, 0o700);
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = KeystorePerms.run(&env);
        assert_eq!(out.status, Status::Na);
        assert!(out.message.contains("no *.key files"));
    }

    #[test]
    fn keystore_perms_pass_with_one_0600_key() {
        let home = tempfile::tempdir().unwrap();
        let keys = home.path().join(KEYSTORE_DIR);
        std::fs::create_dir_all(&keys).unwrap();
        set_mode(&keys, 0o700);
        let key = keys.join("default.key");
        std::fs::write(&key, b"fake").unwrap();
        set_mode(&key, 0o600);
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = KeystorePerms.run(&env);
        assert_eq!(out.status, Status::Pass, "{out:?}");
        assert!(out.message.contains("1 *.key file"));
    }

    #[test]
    fn keystore_perms_warn_on_overpermissive_key() {
        let home = tempfile::tempdir().unwrap();
        let keys = home.path().join(KEYSTORE_DIR);
        std::fs::create_dir_all(&keys).unwrap();
        set_mode(&keys, 0o700);
        let key = keys.join("leaky.key");
        std::fs::write(&key, b"fake").unwrap();
        set_mode(&key, 0o644);
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = KeystorePerms.run(&env);
        assert_eq!(out.status, Status::Warn, "{out:?}");
        assert!(out.message.contains("leaky.key"));
        assert!(out.message.contains("0644"));
        assert!(out.message.contains("0600"));
        assert!(
            out.suggested_fix
                .as_deref()
                .is_some_and(|s| s.contains("chmod 0600")),
            "fix string: {out:?}"
        );
    }

    #[test]
    fn keystore_perms_warn_names_first_bad_key_only() {
        // Two key files; one over-permissive. We don't pin order
        // (readdir is platform-defined) but assert the bad name
        // appears in the message and the report stops at the first
        // hit (no `Pass` count on the message).
        let home = tempfile::tempdir().unwrap();
        let keys = home.path().join(KEYSTORE_DIR);
        std::fs::create_dir_all(&keys).unwrap();
        set_mode(&keys, 0o700);
        let good = keys.join("good.key");
        std::fs::write(&good, b"ok").unwrap();
        set_mode(&good, 0o600);
        let bad = keys.join("bad.key");
        std::fs::write(&bad, b"leak").unwrap();
        set_mode(&bad, 0o644);
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = KeystorePerms.run(&env);
        assert_eq!(out.status, Status::Warn);
        assert!(out.message.contains("bad.key"), "{out:?}");
    }

    // --- TrustStoreReadable -----------------------------------------

    #[test]
    fn trust_store_na_when_dir_absent() {
        let home = tempfile::tempdir().unwrap();
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = TrustStoreReadable.run(&env);
        assert_eq!(out.status, Status::Na);
        assert!(out.message.contains("no trusted publishers"));
    }

    #[test]
    fn trust_store_pass_when_dir_present() {
        let home = tempfile::tempdir().unwrap();
        let trust = home.path().join(TRUST_STORE_DIR);
        std::fs::create_dir_all(&trust).unwrap();
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = TrustStoreReadable.run(&env);
        assert_eq!(out.status, Status::Pass);
        assert!(out.message.contains("readable"));
    }

    #[test]
    fn trust_store_warn_when_path_is_file() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".tape")).unwrap();
        std::fs::write(home.path().join(TRUST_STORE_DIR), b"stray").unwrap();
        let env = env_with_home(Some(home.path().to_path_buf()));
        let out = TrustStoreReadable.run(&env);
        assert_eq!(out.status, Status::Warn);
        assert!(out.message.contains("not a directory"));
    }
}
