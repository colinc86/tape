//! `index.*` — local library / catalog freshness checks. Issue #183 /
//! Step-5 of #81.
//!
//! Four checks: `index.exists`, `index.sqlite.integrity`,
//! `index.lock.stale`, `index.last_rescan.fresh`. All four are soft
//! dependencies on the local-library SQLite layer scoped under #2;
//! that layer was *not* shipped on `main` even though #2 closed.
//! Consequently every check short-circuits to `Na` when
//! `<cache>/tape/index/` is absent (the "library not in use"
//! branch) and to `Na` with a "deferred to the #2 follow-up"
//! message when the underlying directory / file exists but the
//! real probe (SQLite integrity, pidfile liveness, mtime read) has
//! not yet landed.
//!
//! This file establishes the wiring; the post-#2-SQLite follow-up
//! PR flips the "deferred" branches to real probes without touching
//! the catalog / category-list structure.

use std::path::PathBuf;

use super::super::check::{Check, CheckOutcome, Env, Severity};

const INDEX_SUBDIR_PARENT: &str = "tape";
const INDEX_SUBDIR: &str = "index";
const CATALOG_FILE: &str = "catalog.sqlite";
const LOCK_FILE: &str = ".lock";

/// `<cache>/tape/index/` path. `None` when `env.cache_dir` is
/// unset (which propagates the upstream `$HOME not set` condition).
fn index_dir(env: &Env) -> Option<PathBuf> {
    env.cache_dir
        .as_deref()
        .map(|c| c.join(INDEX_SUBDIR_PARENT).join(INDEX_SUBDIR))
}

fn catalog_path(env: &Env) -> Option<PathBuf> {
    index_dir(env).map(|d| d.join(CATALOG_FILE))
}

fn lock_path(env: &Env) -> Option<PathBuf> {
    index_dir(env).map(|d| d.join(LOCK_FILE))
}

/// Short-circuit reasons shared by every `index.*` check. Each
/// returns `Some(CheckOutcome)` when one of the soft-dependency
/// branches applies; `None` when the check should proceed to its
/// real probe (which, in this slice, is the "deferred to #2"
/// branch).
fn na_no_home(env: &Env) -> Option<CheckOutcome> {
    if env.home.is_none() {
        return Some(CheckOutcome::na("$HOME not set"));
    }
    None
}

fn na_no_index_dir(env: &Env) -> Option<CheckOutcome> {
    let dir = index_dir(env)?;
    if !dir.exists() {
        return Some(CheckOutcome::na(format!(
            "{} not present (library not in use)",
            dir.display()
        )));
    }
    None
}

/// Is `<cache>/tape/index/` present and a directory?
///
/// `Na` when `$HOME` is unset or the directory is absent (library
/// not in use). `Warn` when the path exists but isn't a directory
/// (defensive against partial-install corruption). `Pass` when the
/// directory is present.
pub struct Exists;
impl Check for Exists {
    fn id(&self) -> &'static str {
        "index.exists"
    }
    fn category(&self) -> &'static str {
        "index"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Warn
    }
    fn description(&self) -> &'static str {
        "~/.cache/tape/index/ exists (n/a when library not in use)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        if let Some(o) = na_no_home(env) {
            return o;
        }
        let Some(dir) = index_dir(env) else {
            return CheckOutcome::na("$HOME not set");
        };
        if !dir.exists() {
            return CheckOutcome::na(format!(
                "{} not present (library not in use)",
                dir.display()
            ));
        }
        let meta = match std::fs::metadata(&dir) {
            Ok(m) => m,
            Err(e) => {
                return CheckOutcome::warn(format!("{} cannot be stat'd: {e}", dir.display()));
            }
        };
        if !meta.is_dir() {
            return CheckOutcome::warn(format!("{} exists but is not a directory", dir.display()));
        }
        CheckOutcome::pass(format!("{} present", dir.display()))
    }
}

/// `PRAGMA integrity_check` against `<index>/catalog.sqlite`.
///
/// `Na` when the index dir / catalog file is absent (no library to
/// check). `Na` with a "deferred to the #2 follow-up" message when
/// the catalog file *does* exist — this slice does not implement
/// the real probe.
pub struct SqliteIntegrity;
impl Check for SqliteIntegrity {
    fn id(&self) -> &'static str {
        "index.sqlite.integrity"
    }
    fn category(&self) -> &'static str {
        "index"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Fail
    }
    fn description(&self) -> &'static str {
        "<index>/catalog.sqlite passes PRAGMA integrity_check (n/a until #2 SQLite ships)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        if let Some(o) = na_no_home(env) {
            return o;
        }
        if let Some(o) = na_no_index_dir(env) {
            return o;
        }
        let Some(catalog) = catalog_path(env) else {
            return CheckOutcome::na("$HOME not set");
        };
        if !catalog.exists() {
            return CheckOutcome::na(format!(
                "{} not present (library not in use)",
                catalog.display()
            ));
        }
        CheckOutcome::na("SQLite integrity check is deferred to the #2 follow-up")
    }
}

/// No stale write-lock left over from a crashed indexer.
///
/// `Na` when the index dir / lockfile is absent (no recent
/// indexer activity — the healthy fresh-run state). `Na` with a
/// "deferred to the #2 follow-up" message when a lockfile *does*
/// exist; the post-#2-SQLite PR flips this to a real pid-liveness
/// probe.
pub struct LockStale;
impl Check for LockStale {
    fn id(&self) -> &'static str {
        "index.lock.stale"
    }
    fn category(&self) -> &'static str {
        "index"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Warn
    }
    fn description(&self) -> &'static str {
        "no stale write lock on the index (n/a until #2 SQLite ships)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        if let Some(o) = na_no_home(env) {
            return o;
        }
        if let Some(o) = na_no_index_dir(env) {
            return o;
        }
        let Some(lock) = lock_path(env) else {
            return CheckOutcome::na("$HOME not set");
        };
        if !lock.exists() {
            return CheckOutcome::na(format!(
                "{} not present (no recent indexer activity)",
                lock.display()
            ));
        }
        CheckOutcome::na("lock liveness probe is deferred to the #2 follow-up")
    }
}

/// Last-rescan timestamp on the catalog is ≤24h old.
///
/// `Na` when the index dir / catalog file is absent. `Na` with a
/// "deferred to the #2 follow-up" message when the catalog file
/// *does* exist; the post-#2-SQLite PR flips this to a real mtime
/// read.
pub struct LastRescanFresh;
impl Check for LastRescanFresh {
    fn id(&self) -> &'static str {
        "index.last_rescan.fresh"
    }
    fn category(&self) -> &'static str {
        "index"
    }
    fn severity_on_fail(&self) -> Severity {
        Severity::Warn
    }
    fn description(&self) -> &'static str {
        "last rescan was within 24h (n/a until #2 SQLite ships)"
    }
    fn run(&self, env: &Env) -> CheckOutcome {
        if let Some(o) = na_no_home(env) {
            return o;
        }
        if let Some(o) = na_no_index_dir(env) {
            return o;
        }
        let Some(catalog) = catalog_path(env) else {
            return CheckOutcome::na("$HOME not set");
        };
        if !catalog.exists() {
            return CheckOutcome::na(format!(
                "{} not present (library not in use)",
                catalog.display()
            ));
        }
        CheckOutcome::na("rescan-freshness probe is deferred to the #2 follow-up")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::check::Status;
    use std::path::{Path, PathBuf};

    fn env_with(home: Option<&Path>, cache: Option<&Path>) -> Env {
        Env {
            home: home.map(Path::to_path_buf),
            cache_dir: cache.map(Path::to_path_buf),
            tmpdir: std::env::temp_dir(),
            path_dirs: vec![],
            cwd: PathBuf::from("."),
            compile_time_version: "0.0.0-test",
        }
    }

    #[test]
    fn all_four_na_when_home_unset() {
        let env = env_with(None, None);
        let outs = [
            Exists.run(&env),
            SqliteIntegrity.run(&env),
            LockStale.run(&env),
            LastRescanFresh.run(&env),
        ];
        for o in &outs {
            assert_eq!(o.status, Status::Na);
            assert!(o.message.contains("$HOME not set"), "{}", o.message);
        }
    }

    #[test]
    fn all_four_na_when_index_dir_absent() {
        let dir = tempfile::tempdir().unwrap();
        // home set, cache_dir set, but no <cache>/tape/index/.
        let env = env_with(Some(dir.path()), Some(dir.path()));
        let outs = [
            Exists.run(&env),
            SqliteIntegrity.run(&env),
            LockStale.run(&env),
            LastRescanFresh.run(&env),
        ];
        for o in &outs {
            assert_eq!(o.status, Status::Na, "{:?}", o);
            assert!(o.message.contains("library not in use"), "{}", o.message);
        }
    }

    #[test]
    fn exists_passes_when_dir_present_and_other_three_na_with_not_present() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("tape").join("index")).unwrap();
        let env = env_with(Some(dir.path()), Some(dir.path()));

        let exists = Exists.run(&env);
        assert_eq!(exists.status, Status::Pass, "{exists:?}");
        assert!(exists.message.contains("present"), "{}", exists.message);

        for o in [
            SqliteIntegrity.run(&env),
            LockStale.run(&env),
            LastRescanFresh.run(&env),
        ] {
            assert_eq!(o.status, Status::Na, "{o:?}");
            assert!(
                o.message.contains("not present"),
                "should NOT use the deferred wording yet: {}",
                o.message
            );
        }
    }

    #[test]
    fn exists_warns_when_path_is_a_file_not_a_dir() {
        let dir = tempfile::tempdir().unwrap();
        let tape_dir = dir.path().join("tape");
        std::fs::create_dir_all(&tape_dir).unwrap();
        // Create `<cache>/tape/index` as a *file*, not a directory.
        std::fs::write(tape_dir.join("index"), b"oops").unwrap();
        let env = env_with(Some(dir.path()), Some(dir.path()));

        let out = Exists.run(&env);
        assert_eq!(out.status, Status::Warn, "{out:?}");
        assert!(out.message.contains("not a directory"), "{}", out.message);
    }

    #[test]
    fn sqlite_integrity_defers_when_catalog_file_present() {
        let dir = tempfile::tempdir().unwrap();
        let idx = dir.path().join("tape").join("index");
        std::fs::create_dir_all(&idx).unwrap();
        std::fs::write(idx.join("catalog.sqlite"), []).unwrap();
        let env = env_with(Some(dir.path()), Some(dir.path()));

        let out = SqliteIntegrity.run(&env);
        assert_eq!(out.status, Status::Na, "{out:?}");
        assert!(
            out.message.contains("deferred to the #2 follow-up"),
            "{}",
            out.message
        );
    }

    #[test]
    fn last_rescan_fresh_defers_when_catalog_file_present() {
        let dir = tempfile::tempdir().unwrap();
        let idx = dir.path().join("tape").join("index");
        std::fs::create_dir_all(&idx).unwrap();
        std::fs::write(idx.join("catalog.sqlite"), []).unwrap();
        let env = env_with(Some(dir.path()), Some(dir.path()));

        let out = LastRescanFresh.run(&env);
        assert_eq!(out.status, Status::Na, "{out:?}");
        assert!(
            out.message.contains("deferred to the #2 follow-up"),
            "{}",
            out.message
        );
    }

    #[test]
    fn lock_stale_defers_when_lockfile_present() {
        let dir = tempfile::tempdir().unwrap();
        let idx = dir.path().join("tape").join("index");
        std::fs::create_dir_all(&idx).unwrap();
        std::fs::write(idx.join(".lock"), []).unwrap();
        let env = env_with(Some(dir.path()), Some(dir.path()));

        let out = LockStale.run(&env);
        assert_eq!(out.status, Status::Na, "{out:?}");
        assert!(
            out.message.contains("deferred to the #2 follow-up"),
            "{}",
            out.message
        );
    }
}
