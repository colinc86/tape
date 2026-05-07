//! Find Claude Code's active session transcript file.
//!
//! Claude Code stores per-project session transcripts at:
//!
//!   ~/.claude/projects/<encoded-cwd>/<session-id>.jsonl
//!
//! where `<encoded-cwd>` is the cwd with `/` characters replaced by `-`.
//!
//! "Active" = newest by mtime among the JSONL files in that directory.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TranscriptHandle {
    pub jsonl_path: PathBuf,
    pub session_id: String,
    pub sibling_dir: PathBuf,
}

/// Find the active session for `cwd`. If `TAPE_TRANSCRIPT_OVERRIDE` is set,
/// returns it directly (used by integration tests).
pub fn find_active_session(cwd: &Path) -> std::io::Result<TranscriptHandle> {
    if let Ok(path) = std::env::var("TAPE_TRANSCRIPT_OVERRIDE") {
        return Ok(handle_from_path(PathBuf::from(path)));
    }

    let projects_dir = home_dir()
        .ok_or_else(|| std::io::Error::other("HOME not set"))?
        .join(".claude")
        .join("projects");
    let encoded = encode_cwd(cwd);
    let dir = projects_dir.join(&encoded);
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no transcript dir at {}", dir.display()),
        ));
    }

    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let mtime = entry.metadata()?.modified()?;
        match newest {
            Some((_, t)) if t >= mtime => {}
            _ => newest = Some((path, mtime)),
        }
    }

    let (path, _) = newest.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no .jsonl transcripts in {}", dir.display()),
        )
    })?;
    Ok(handle_from_path(path))
}

fn handle_from_path(jsonl_path: PathBuf) -> TranscriptHandle {
    let session_id = jsonl_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let sibling_dir = jsonl_path
        .parent()
        .map(|p| p.join(&session_id))
        .unwrap_or_else(|| PathBuf::from(&session_id));
    TranscriptHandle {
        jsonl_path,
        session_id,
        sibling_dir,
    }
}

/// Encode an absolute cwd path the way Claude Code does for its
/// `~/.claude/projects/` directory naming. Both `/` and spaces become `-`.
///
/// Empirically validated against Claude Code 2.1.129: a path like
/// `/Users/colin/Local Documents/Programming/Misc/tape` encodes to
/// `-Users-colin-Local-Documents-Programming-Misc-tape`.
pub fn encode_cwd(cwd: &Path) -> String {
    cwd.display()
        .to_string()
        .chars()
        .map(|c| if c == '/' || c == ' ' { '-' } else { c })
        .collect()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_cwd_replaces_slashes() {
        let p = Path::new("/Users/colin/Code/tape");
        assert_eq!(encode_cwd(p), "-Users-colin-Code-tape");
    }

    #[test]
    fn encode_cwd_replaces_spaces_with_dashes() {
        // Claude Code replaces BOTH `/` and ` ` with `-`.
        let p = Path::new("/Users/colin/Local Documents/Programming/Misc/tape");
        assert_eq!(
            encode_cwd(p),
            "-Users-colin-Local-Documents-Programming-Misc-tape"
        );
    }

    #[test]
    fn override_env_var_short_circuits() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("override.jsonl");
        std::fs::write(&path, b"{}").unwrap();
        std::env::set_var("TAPE_TRANSCRIPT_OVERRIDE", &path);
        let handle = find_active_session(Path::new("/anywhere")).unwrap();
        assert_eq!(handle.jsonl_path, path);
        assert_eq!(handle.session_id, "override");
        std::env::remove_var("TAPE_TRANSCRIPT_OVERRIDE");
    }

    #[test]
    fn handle_from_path_derives_sibling_dir() {
        let h = handle_from_path(PathBuf::from("/x/y/abc-123.jsonl"));
        assert_eq!(h.session_id, "abc-123");
        assert_eq!(h.sibling_dir, PathBuf::from("/x/y/abc-123"));
    }
}
