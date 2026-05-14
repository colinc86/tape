//! `tape doctor` — install-surface diagnostic.
//!
//! Phase 1 (issue #81): the framework + `binary.*`, `config.*`,
//! `permissions.*` checks. Text output only. JSON, `--fix`, MCP/plugin/
//! recording/claude-code checks, feature-gated checks, and the
//! `.taperc::doctor:` section land in follow-up phases.

pub mod catalog;
pub mod check;
pub mod checks;
pub mod report;
pub mod runner;

pub use check::Env;
pub use runner::{run, RunFilter};

use anyhow::Result;

/// CLI-layer options. Constructed by `main.rs` from the parsed clap args.
#[derive(Debug, Clone, Default)]
pub struct CliOptions {
    pub select_ids: Vec<String>,
    pub include_categories: Vec<String>,
    pub exclude_categories: Vec<String>,
    pub list_checks: bool,
    pub quiet: bool,
    pub no_color: bool,
}

/// Top-level entry point invoked from `Cmd::Doctor`. Returns the exit
/// code the CLI should use.
///
/// The `Result` wrapping is currently never `Err` — phase 1 only does text
/// rendering. Phase 2's JSON output will introduce fallible I/O paths; the
/// signature stays `Result<i32>` so phase 2's diff is additive.
#[allow(clippy::unnecessary_wraps)]
pub fn execute(opts: CliOptions) -> Result<i32> {
    if opts.list_checks {
        print!("{}", report::render_catalog_listing());
        return Ok(0);
    }
    let env = Env::from_process();
    let filter = RunFilter {
        select_ids: opts.select_ids,
        include_categories: opts.include_categories,
        exclude_categories: opts.exclude_categories,
    };
    let report = run(&env, &filter);
    let color = color_enabled(opts.no_color);
    print!("{}", report::render_text(&report, color, opts.quiet));
    Ok(report.summary.exit_code())
}

/// Respect both `--no-color` and `$NO_COLOR` (per the project convention).
/// Also disable color when stdout isn't a TTY — pipe-into-grep should not
/// receive escape codes.
fn color_enabled(no_color_flag: bool) -> bool {
    use std::io::IsTerminal;
    if no_color_flag {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}
