//! `.tapelist` Phase 1 parser (issue #221, carved from #78).
//!
//! Plain text, UTF-8, one cassette path per line. Lines starting with
//! `#` (after trimming leading whitespace) are comments. Blank lines
//! are ignored. Relative paths resolve against the parent directory of
//! the `.tapelist` file, NOT the process CWD — that's what makes a
//! playlist portable when its sibling cassettes move with it.
//!
//! Phase 1 is intentionally a strict subset of the richer YAML schema
//! proposed in #78 (no `schema_version`, no per-entry `sha256` /
//! `label` / `required`, no `--apply`). A Phase 2 parser is free to
//! support the bare-paths form as a degenerate case, but Phase 1 makes
//! no such commitment.

use std::path::{Path, PathBuf};

/// Parsed `.tapelist`: an ordered list of resolved cassette paths.
///
/// Duplicate paths are preserved — a curriculum may legitimately
/// repeat the same cassette (e.g. "warm-up exercise" + "end-of-week
/// review"); Phase 1 validates each occurrence independently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub entries: Vec<PathBuf>,
}

/// Parse the textual contents of a `.tapelist`, resolving relative
/// entries against `base_dir` (typically the parent directory of the
/// `.tapelist` file). Pure function; performs no I/O. Never fails in
/// Phase 1 — every line is either a comment, blank, or a path.
///
/// Path resolution rules (kept narrow for Phase 1):
///
/// - Absolute paths pass through unchanged.
/// - `~/` is expanded via `$HOME` when the env var is set; bare `~`
///   (without a trailing `/`) is left as-is. If `$HOME` is unset, the
///   `~/`-prefixed path is left as-is — the per-entry classifier will
///   then surface it as `[MISSING]` rather than silently substituting.
/// - All other relative paths are joined against `base_dir`.
pub fn parse(text: &str, base_dir: &Path) -> Playlist {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    parse_with_home(text, base_dir, home.as_deref())
}

/// Inner form that takes `home` explicitly. Exposed inside the crate
/// (and to unit tests) so tilde expansion can be exercised without
/// mutating the process environment.
pub(crate) fn parse_with_home(text: &str, base_dir: &Path, home: Option<&Path>) -> Playlist {
    let mut entries = Vec::new();
    for raw_line in text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        entries.push(resolve(trimmed, base_dir, home));
    }
    Playlist { entries }
}

fn resolve(entry: &str, base_dir: &Path, home: Option<&Path>) -> PathBuf {
    if let Some(rest) = entry.strip_prefix("~/") {
        if let Some(h) = home {
            return h.join(rest);
        }
        return PathBuf::from(entry);
    }
    let p = Path::new(entry);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn base() -> PathBuf {
        PathBuf::from("/tmp/base")
    }

    #[test]
    fn parse_skips_blank_and_comment_lines() {
        let text = "\
# header comment
./a.tape

   # indented comment
./b.tape
";
        let pl = parse(text, &base());
        assert_eq!(
            pl.entries,
            vec![
                PathBuf::from("/tmp/base/./a.tape"),
                PathBuf::from("/tmp/base/./b.tape"),
            ]
        );
    }

    #[test]
    fn parse_trims_whitespace_around_paths() {
        let pl = parse("   ./trim-me.tape   \n", &base());
        assert_eq!(pl.entries, vec![PathBuf::from("/tmp/base/./trim-me.tape")]);
    }

    #[test]
    fn parse_keeps_absolute_paths_unchanged() {
        let pl = parse("/etc/passwd\n", &base());
        assert_eq!(pl.entries, vec![PathBuf::from("/etc/passwd")]);
    }

    #[test]
    fn parse_preserves_duplicates() {
        let pl = parse("./same.tape\n./same.tape\n", &base());
        assert_eq!(pl.entries.len(), 2);
        assert_eq!(pl.entries[0], pl.entries[1]);
    }

    #[test]
    fn parse_hash_mid_line_is_part_of_path() {
        // `# at column 0 (after trim) is a comment; `#` mid-string isn't.
        let pl = parse("./weird#name.tape\n", &base());
        assert_eq!(
            pl.entries,
            vec![PathBuf::from("/tmp/base/./weird#name.tape")]
        );
    }

    #[test]
    fn parse_empty_input_yields_no_entries() {
        assert!(parse("", &base()).entries.is_empty());
    }

    #[test]
    fn parse_comment_only_input_yields_no_entries() {
        assert!(parse("# only\n   #also\n\n", &base()).entries.is_empty());
    }

    #[test]
    fn resolve_tilde_uses_supplied_home() {
        let home = PathBuf::from("/tmp/fake-home");
        let pl = parse_with_home("~/sub/c.tape\n", &base(), Some(&home));
        assert_eq!(pl.entries, vec![PathBuf::from("/tmp/fake-home/sub/c.tape")]);
    }

    #[test]
    fn resolve_tilde_unchanged_when_no_home() {
        let pl = parse_with_home("~/sub/c.tape\n", &base(), None);
        // No HOME → leave as-is; the per-entry classifier will then
        // surface this as [MISSING] rather than silently substituting.
        assert_eq!(pl.entries, vec![PathBuf::from("~/sub/c.tape")]);
    }
}
