//! `tape` CLI entrypoint. Subcommands route to crates.

mod doctor;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "tape", version, about = "A cassette tape for agent runs")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Validate a `.tape` file against the tape/v0 spec.
    Verify {
        /// Path to the `.tape` file.
        file: std::path::PathBuf,
        /// Emit machine-readable JSON diagnostics.
        #[arg(long)]
        json: bool,
    },
    /// Pretty-print a tape's contents.
    Play {
        file: std::path::PathBuf,
        #[arg(long)]
        step: Option<u64>,
        #[arg(long)]
        range: Option<String>,
        #[arg(long)]
        kind: Option<String>,
    },
    /// One-line-per-track listing.
    Ls { file: std::path::PathBuf },
    /// Compare two tapes.
    Diff {
        a: std::path::PathBuf,
        b: std::path::PathBuf,
        #[arg(long)]
        all: bool,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        judge: Option<String>,
    },
    /// Record a Claude Code session into a `.tape` file.
    Record {
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        #[arg(long)]
        yes: bool,
        /// One-line description of the task being recorded. Lands in `meta.task`.
        #[arg(long, default_value = "")]
        task: String,
        /// Override Anthropic upstream URL (default: env var or `https://api.anthropic.com`).
        #[arg(long)]
        upstream_anthropic: Option<String>,
        /// Override OpenAI upstream URL (default: env var or `https://api.openai.com`).
        #[arg(long)]
        upstream_openai: Option<String>,
        /// Command and args after `--`.
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Run the deck (MCP server) over stdio.
    Mcp,
    /// Eject an in-flight session (used internally; rare standalone).
    Eject {
        #[arg(long)]
        from: String,
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Diagnose the install surface. Reports pass/warn/fail per check.
    Doctor {
        /// Run only the named checks. Comma-separated; repeatable.
        #[arg(long, value_delimiter = ',')]
        check: Vec<String>,
        /// Limit to one or more categories. Repeatable.
        #[arg(long)]
        include: Vec<String>,
        /// Inverse of --include. Repeatable.
        #[arg(long)]
        exclude: Vec<String>,
        /// Enumerate every registered check and exit.
        #[arg(long)]
        list_checks: bool,
        /// Suppress `pass` lines; show only warn/fail/n/a.
        #[arg(long)]
        quiet: bool,
        /// Strip ANSI color. Also honors `$NO_COLOR`.
        #[arg(long)]
        no_color: bool,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Cmd::Verify { file, json } => cmd_verify(&file, json),
        Cmd::Ls { file } => cmd_ls(&file),
        Cmd::Play {
            file,
            step,
            range,
            kind,
        } => cmd_play(&file, step, range.as_deref(), kind.as_deref()),
        Cmd::Diff {
            a,
            b,
            all,
            format,
            judge,
        } => {
            if let Some(j) = judge {
                anyhow::bail!(
                    "tape diff --judge is not yet implemented (got: {j}). \
                     The judge-narrated alignment is on the roadmap; until then, \
                     tape diff produces structural alignment only. \
                     Re-run without --judge to get the structural diff."
                );
            }
            cmd_diff(&a, &b, all, &format)
        }
        Cmd::Record {
            label,
            out,
            yes: _,
            task,
            upstream_anthropic,
            upstream_openai,
            cmd,
        } => cmd_record(label, out, task, upstream_anthropic, upstream_openai, cmd),
        Cmd::Eject { .. } => {
            anyhow::bail!("standalone eject not yet implemented (used internally by record)")
        }
        Cmd::Mcp => {
            tape_mcp::stdio_loop()?;
            Ok(())
        }
        Cmd::Doctor {
            check,
            include,
            exclude,
            list_checks,
            quiet,
            no_color,
        } => {
            let opts = doctor::CliOptions {
                select_ids: check,
                include_categories: include,
                exclude_categories: exclude,
                list_checks,
                quiet,
                no_color,
            };
            let code = doctor::execute(opts)?;
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }
    }
}

fn cmd_record(
    label: Option<String>,
    out: Option<std::path::PathBuf>,
    task: String,
    upstream_anthropic: Option<String>,
    upstream_openai: Option<String>,
    cmd: Vec<String>,
) -> Result<()> {
    if cmd.is_empty() {
        anyhow::bail!("tape record: no command supplied (try `-- claude \"say hi\"`)");
    }
    let anthropic_upstream = upstream_anthropic
        .or_else(|| std::env::var("TAPE_UPSTREAM_ANTHROPIC").ok())
        .unwrap_or_else(|| "https://api.anthropic.com".to_owned());
    let openai_upstream = upstream_openai
        .or_else(|| std::env::var("TAPE_UPSTREAM_OPENAI").ok())
        .unwrap_or_else(|| "https://api.openai.com".to_owned());
    let out_path = out.unwrap_or_else(|| {
        let stem = label
            .as_deref()
            .map(sanitize_label)
            .filter(|s| !s.is_empty() && !s.chars().all(|c| c == '-'))
            .unwrap_or_else(|| "session".to_owned());
        std::path::PathBuf::from(format!("{stem}.tape"))
    });
    let task_text = if task.is_empty() {
        cmd.join(" ")
    } else {
        task
    };

    let opts = tape_record::run::RecordOptions {
        task: task_text,
        recorder_agent: format!("tape-cli/{}", env!("CARGO_PKG_VERSION")),
        out_path,
        upstream_anthropic: anthropic_upstream,
        upstream_openai: openai_upstream,
        label,
        command: cmd,
        env: vec![],
        mcp_servers: vec![],
        tape_hook_bin: None,
        tape_mcp_wrap_bin: None,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = rt.block_on(tape_record::run::record(opts))?;

    eprintln!(
        "tape: wrote {} ({} tracks, {} artifacts)",
        result.eject.path.display(),
        result.eject.track_count,
        result.eject.artifact_count
    );
    if !result.child_status.success() {
        std::process::exit(result.child_status.code().unwrap_or(1));
    }
    Ok(())
}

fn cmd_diff(
    a: &std::path::Path,
    b: &std::path::Path,
    all: bool,
    format: &str,
) -> Result<()> {
    let diff = tape_diff::compute(a, b)?;
    match format {
        "json" => {
            println!("{}", tape_diff::render_json(&diff));
        }
        _ => {
            print!("{}", tape_diff::render_text(&diff, all));
        }
    }
    Ok(())
}

fn sanitize_label(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn load_tracks(path: &std::path::Path) -> Result<(tape_format::reader::RawTape, Vec<tape_format::tracks::Track>)> {
    let raw = tape_format::reader::RawTape::open(path)?;
    let jsonl = raw
        .tracks_jsonl
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("tape is missing tracks.jsonl"))?;
    let tracks = tape_format::tracks::parse_jsonl(jsonl)?;
    Ok((raw, tracks))
}

fn cmd_ls(file: &std::path::Path) -> Result<()> {
    let (_, tracks) = load_tracks(file)?;
    print!("{}", tape_play::render_ls(&tracks));
    Ok(())
}

fn cmd_play(
    file: &std::path::Path,
    step: Option<u64>,
    range: Option<&str>,
    kind: Option<&str>,
) -> Result<()> {
    let (raw, tracks) = load_tracks(file)?;
    let parsed_range = range.and_then(tape_play::parse_range);

    if step.is_none() && range.is_none() && kind.is_none() {
        let meta_yaml = raw.meta_yaml.as_deref().unwrap_or("");
        let liner = raw.liner_md.as_deref().unwrap_or("");
        print!("{}", tape_play::render_summary_view(meta_yaml, liner, &tracks));
    } else {
        let filtered = tape_play::filter(&tracks, step, parsed_range, kind);
        let owned: Vec<tape_format::tracks::Track> = filtered.into_iter().cloned().collect();
        print!("{}", tape_play::render_play(&owned));
    }
    Ok(())
}

fn cmd_verify(file: &std::path::Path, json: bool) -> Result<()> {
    let raw = match tape_format::reader::RawTape::open(file) {
        Ok(r) => r,
        Err(e) => {
            if json {
                let payload = serde_json::json!({
                    "valid": false,
                    "diagnostics": [{
                        "code": "MALFORMED_ZIP",
                        "severity": "error",
                        "message": e.to_string(),
                    }],
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                eprintln!("ERROR MALFORMED_ZIP: {e}");
            }
            std::process::exit(2);
        }
    };

    let report = tape_format::verify::verify(&raw);

    if json {
        let diags: Vec<_> = report
            .diagnostics
            .iter()
            .map(|d| {
                serde_json::json!({
                    "code": d.code.as_str(),
                    "severity": match d.severity {
                        tape_format::verify::Severity::Error => "error",
                        tape_format::verify::Severity::Warning => "warning",
                    },
                    "message": d.message,
                })
            })
            .collect();
        let payload = serde_json::json!({
            "valid": report.is_valid(),
            "diagnostics": diags,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if report.diagnostics.is_empty() {
        println!("OK {}", file.display());
    } else {
        for d in &report.diagnostics {
            let level = match d.severity {
                tape_format::verify::Severity::Error => "ERROR",
                tape_format::verify::Severity::Warning => "WARN ",
            };
            println!("{level} {}: {}", d.code.as_str(), d.message);
        }
        if !report.is_valid() {
            println!(
                "\nFAIL {} ({} errors, {} warnings)",
                file.display(),
                report.errors().count(),
                report.warnings().count(),
            );
        } else {
            println!(
                "\nOK   {} ({} warnings)",
                file.display(),
                report.warnings().count()
            );
        }
    }

    if report.is_valid() {
        Ok(())
    } else {
        std::process::exit(2);
    }
}
