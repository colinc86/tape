//! Golden snapshot tests for `tape_export::render_markdown` against
//! the existing fixture cassettes. Per Principal's Step-1 test plan
//! on issue #8.
//!
//! Snapshots live next to this file in `snapshots/`. To regenerate
//! after an intentional output change, run with `INSTA_UPDATE=auto`:
//!
//! ```console
//! INSTA_UPDATE=auto cargo test -p tape-export --test render_md_snapshots
//! ```

use std::path::PathBuf;

use tape_export::render_markdown;
use tape_format::reader::RawTape;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn render(name: &str) -> String {
    let raw = RawTape::open(fixture(name)).expect("fixture opens");
    render_markdown(&raw).expect("markdown renders")
}

#[test]
fn snapshot_minimal_success() {
    insta::assert_snapshot!("minimal_success_md", render("minimal-success.tape"));
}

#[test]
fn snapshot_killer_scenario_a() {
    insta::assert_snapshot!("killer_scenario_a_md", render("killer-scenario-a.tape"));
}

#[test]
fn snapshot_oversized_payload() {
    insta::assert_snapshot!("oversized_payload_md", render("oversized-payload.tape"));
}

#[test]
fn rendered_starts_with_h1_title() {
    // Cheap structural assertion separate from the full snapshot —
    // catches accidents that delete the title line entirely without
    // the noisier full-snapshot diff. Asserts on every fixture so a
    // missing title shows up regardless of which cassette is open.
    for name in [
        "minimal-success.tape",
        "killer-scenario-a.tape",
        "oversized-payload.tape",
    ] {
        let md = render(name);
        assert!(
            md.starts_with("# "),
            "{name}: rendered Markdown must start with an H1 title (got: {:?})",
            md.lines().next()
        );
    }
}

#[test]
fn rendered_contains_required_sections() {
    // Liner notes + Tracklist H2s are part of the Step-1 contract.
    for name in [
        "minimal-success.tape",
        "killer-scenario-a.tape",
        "oversized-payload.tape",
    ] {
        let md = render(name);
        assert!(
            md.contains("## Liner notes"),
            "{name}: missing `## Liner notes` H2"
        );
        assert!(
            md.contains("## Tracklist"),
            "{name}: missing `## Tracklist` H2"
        );
    }
}
