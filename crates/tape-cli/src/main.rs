//! `tape` CLI entrypoint. Subcommands route to crates.

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
    /// Append an annotation to an existing tape, writing a new cassette.
    ///
    /// Phase-1 CLI counterpart to the deck's `tape.annotate` tool. See
    /// issue #74 — implements only the minimum viable surface; `--editor`,
    /// `--import`, `--in-place`, and `--force-resign` are follow-ups.
    Annotate {
        /// Input cassette to annotate.
        file: std::path::PathBuf,
        /// Annotation body. SPEC §5.5.7 `note` field.
        #[arg(long)]
        note: String,
        /// Parent step the annotation hangs off. Validated against the
        /// tape's existing tracks: 1 ≤ N < new_step.
        #[arg(long)]
        step: Option<u64>,
        /// Free-form attribution shown in CLI output / `--json`. Defaults
        /// to `$USER`. Not stored in the payload (SPEC §5.5.7 is
        /// `{by, note}` only).
        #[arg(long)]
        actor: Option<String>,
        /// Who is making the note. Default `human` for the CLI (the deck
        /// defaults to `agent`).
        #[arg(long, default_value = "human", value_parser = ["agent", "human"])]
        by: String,
        /// Output path. Default: `<basename>.annotated.tape` next to the
        /// input. Refuses if equal to the input path.
        #[arg(short = 'o', long)]
        out: Option<std::path::PathBuf>,
        /// Override the annotation timestamp. Must be RFC-3339 (`Z`
        /// suffix). MUST be ≥ the last track's `ts` to preserve SPEC §5.2
        /// monotonicity.
        #[arg(long)]
        ts: Option<String>,
        /// Emit the §3.10 schema-v1 success summary on stdout.
        #[arg(long)]
        json: bool,
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
        Cmd::Annotate {
            file,
            note,
            step,
            actor,
            by,
            out,
            ts,
            json,
        } => cmd_annotate(&file, &note, step, actor, &by, out, ts, json),
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

/// Phase-1 of issue #74. Loads `file`, runs the user's `--note` body through
/// the redaction engine's defense-in-depth scan (the eject pipeline's
/// `Pass 1` would redact rather than reject, so the pre-scan here is what
/// gives leaks their explicit `ANNOT_LEAK` exit), assembles a fresh
/// `Session` by replaying the loaded tracks via `append_track` (preserves
/// `parent_step`/`refs`/`annotations` per #49), tacks on the new
/// `annotation` event, and routes through `eject::eject` so the output
/// passes `tape verify` with the same artifact and label inheritance the
/// deck's `tool_eject` provides.
#[allow(clippy::too_many_arguments)]
fn cmd_annotate(
    file: &std::path::Path,
    note: &str,
    step: Option<u64>,
    actor: Option<String>,
    by: &str,
    out: Option<std::path::PathBuf>,
    ts: Option<String>,
    json: bool,
) -> Result<()> {
    // 1. Resolve the output path (default sibling: `<stem>.annotated.tape`)
    //    and refuse equal-to-input. SPEC §1.3 — annotate is non-destructive.
    let out_path = match out {
        Some(p) => p,
        None => {
            let stem = file
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "tape".to_owned());
            let parent = file.parent().unwrap_or_else(|| std::path::Path::new("."));
            parent.join(format!("{stem}.annotated.tape"))
        }
    };
    if same_path(file, &out_path) {
        eprintln!("tape annotate: --out must differ from <file> (use --in-place once it ships)");
        std::process::exit(2);
    }

    // 2. Load the input cassette.
    let raw = match tape_format::reader::RawTape::open(file) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("tape annotate: failed to open {}: {e}", file.display());
            std::process::exit(2);
        }
    };
    let jsonl = raw
        .tracks_jsonl
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("input cassette is missing tracks.jsonl"))?;
    let loaded_tracks = tape_format::tracks::parse_jsonl(jsonl)?;
    let meta_yaml = raw
        .meta_yaml
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("input cassette is missing meta.yaml"))?;
    let meta = match tape_format::meta::Meta::parse(meta_yaml) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("tape annotate: meta.yaml does not parse: {e}");
            std::process::exit(2);
        }
    };

    // 3. Build the redact engine from `.taperc` and pre-scan the note body.
    //    The eject pipeline's Pass 1 *redacts* track payloads rather than
    //    rejecting, so a secret in the note would silently end up as
    //    `[REDACTED]` in the output. Pre-scanning here gives ANNOT_LEAK its
    //    explicit exit-6 surface that the issue body and Principal's
    //    acceptance criteria both call for.
    let cwd = std::env::current_dir().map_err(|e| anyhow::anyhow!("cwd: {e}"))?;
    let redact_engine = tape_redact::engine_with_taperc(&cwd)
        .map_err(|e| anyhow::anyhow!("failed to load .taperc: {e}"))?;
    let note_hits = redact_engine.scan(note);
    if !note_hits.is_empty() {
        eprintln!(
            "tape annotate: ANNOT_LEAK — --note matches redaction rule(s): {}",
            note_hits.join(", ")
        );
        std::process::exit(6);
    }

    // 4. Determine the new annotation's `ts` and compute monotonicity-aware
    //    warnings. SPEC §5.2: ts MUST be ≥ the last track's ts.
    let (annot_ts_str, mut warnings) = resolve_annotation_ts(&loaded_tracks, &meta, ts.as_deref())?;

    // 5. Validate `--step` against the new step's range (1 ≤ N < new_step).
    //    `new_step` is one past the last non-eject track we'll be replaying;
    //    a trailing eject (SPEC §5.4) is filtered below, so account for that.
    let replay_len = effective_replay_len(&loaded_tracks);
    let new_step = replay_len + 1;
    if let Some(s) = step {
        if s == 0 || s >= new_step {
            eprintln!("tape annotate: ANNOT_BAD_STEP — --step must be in [1, {new_step}); got {s}");
            std::process::exit(4);
        }
    }

    // 6. Reassemble: start a Session with the input's `created_at`, replay
    //    every loaded track via `append_track` (skip the auto-injected step 1
    //    and any trailing/embedded `eject` per SPEC §5.4), then push the new
    //    annotation track on top.
    let original_created_at = chrono::DateTime::parse_from_rfc3339(&meta.created_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());
    let task_text = extract_task_from_loaded(&loaded_tracks);
    let session = tape_record::session::Session::start_at(
        &task_text,
        format!("tape-cli/{}", env!("CARGO_PKG_VERSION")),
        original_created_at,
    );
    for t in loaded_tracks.iter().skip(1) {
        if t.kind == tape_format::tracks::Kind::Eject {
            continue;
        }
        session.append_track(t.clone());
    }
    let annot_track = tape_format::tracks::Track {
        step: new_step,
        kind: tape_format::tracks::Kind::Annotation,
        ts: annot_ts_str.clone(),
        payload: serde_json::json!({"by": by, "note": note}),
        parent_step: step,
        refs: vec![],
        annotations: vec![],
    };
    session.append_track(annot_track);

    // 7. Inherit artifacts and label (mirrors `tool_eject` per #41, #80).
    let inherited_artifacts: std::collections::BTreeMap<String, Vec<u8>> = raw
        .artifacts
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let inherited_label = meta.label.clone();

    // 8. Eject through the existing pipeline. `Outcome::Unknown` matches the
    //    deck's `tool_eject` default for handles whose outcome the caller
    //    didn't supply (#30) — annotate doesn't reach into the recording's
    //    declared outcome.
    let result = tape_record::eject::eject(
        &session,
        &tape_record::eject::EjectOptions {
            task: task_text,
            recorder_agent: format!("tape-cli/{}", env!("CARGO_PKG_VERSION")),
            outcome: tape_format::meta::Outcome::Unknown,
            stub_liner_notes: true,
            out_path: out_path.clone(),
            redact_engine: Some(redact_engine),
            inherited_artifacts,
            label: inherited_label,
        },
    )
    .map_err(|e| anyhow::anyhow!("eject failed: {e}"))?;

    // 9. Post-eject verify (exit 3 on a regression; preserve original).
    let written = tape_format::reader::RawTape::open(&out_path)?;
    let report = tape_format::verify::verify(&written);
    if !report.is_valid() {
        let _ = std::fs::remove_file(&out_path);
        let codes: Vec<&'static str> = report.errors().map(|d| d.code.as_str()).collect();
        eprintln!(
            "tape annotate: output failed tape verify ({}); removed {}",
            codes.join(","),
            out_path.display()
        );
        std::process::exit(3);
    }

    let actor_display =
        actor.unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "unknown".to_owned()));

    if json {
        let mut payload = serde_json::json!({
            "schema_version": "1",
            "output_path": out_path.to_string_lossy(),
            "new_step": new_step,
            "actor": actor_display,
            "by": by,
            "warnings": warnings,
        });
        if let Some(s) = step {
            payload["parent_step"] = serde_json::Value::from(s);
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("ok: annotated {}", out_path.display());
        let parent_desc = step
            .map(|s| format!("parent_step={s}"))
            .unwrap_or_else(|| "unparented".to_owned());
        println!("  new track: step {new_step} (kind=annotation, {parent_desc})");
        println!("  actor: {actor_display}, by: {by}");
        for w in warnings.drain(..) {
            println!("  warning: {w}");
        }
        let _ = result; // suppress unused warning
    }
    Ok(())
}

/// Determine the annotation's `ts`. Honors an explicit `--ts` (validated
/// against monotonicity vs the last loaded track's ts → exit 7 on
/// violation). Otherwise: snapshot-collapse-aware default — when every
/// loaded track shares one `ts` (the snapshot-import case from #5), fall
/// back to `meta.ejected_at` and warn so the new event doesn't claim
/// "now" relative to a frozen-time tape. Otherwise `now()`.
fn resolve_annotation_ts(
    loaded: &[tape_format::tracks::Track],
    meta: &tape_format::meta::Meta,
    ts_arg: Option<&str>,
) -> Result<(String, Vec<String>)> {
    let mut warnings: Vec<String> = Vec::new();

    // Last non-eject ts is the floor for monotonicity (SPEC §5.2). Note
    // we ignore the trailing eject to match the eject pipeline's behavior:
    // the new annotation goes *before* the freshly-appended eject in
    // `eject()`, so its ts only has to dominate the pre-eject tail.
    let last_ts = loaded
        .iter()
        .rev()
        .find(|t| t.kind != tape_format::tracks::Kind::Eject)
        .map(|t| t.ts.clone());

    if let Some(explicit) = ts_arg {
        if let Some(floor) = last_ts.as_deref() {
            if explicit < floor {
                eprintln!(
                    "tape annotate: ANNOT_TS_NOT_MONOTONIC — --ts {explicit} predates last track ts {floor}"
                );
                std::process::exit(7);
            }
        }
        return Ok((explicit.to_owned(), warnings));
    }

    // Snapshot-collapse detector — every non-eject track shares one ts.
    let unique_ts: std::collections::HashSet<&str> = loaded
        .iter()
        .filter(|t| t.kind != tape_format::tracks::Kind::Eject)
        .map(|t| t.ts.as_str())
        .collect();
    if unique_ts.len() == 1 && !meta.ejected_at.is_empty() {
        warnings.push("snapshot_collapse_ts_fallback".to_owned());
        return Ok((meta.ejected_at.clone(), warnings));
    }

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    if let Some(floor) = last_ts {
        if now < floor {
            // Wall clock is behind the last track — bump to floor + 1ms-ish
            // by using the floor verbatim. Maintains monotonicity without
            // hard-failing on a clock skew.
            return Ok((floor, warnings));
        }
    }
    Ok((now, warnings))
}

/// Count the non-eject loaded tracks for `new_step` math. Mirrors the
/// `iter().skip(1).filter(!eject)` replay loop above so step numbering is
/// self-consistent.
fn effective_replay_len(loaded: &[tape_format::tracks::Track]) -> u64 {
    // Start at 1 to account for the Session's auto-injected step-1 task,
    // which the replay loop skips.
    let mut n: u64 = 1;
    for t in loaded.iter().skip(1) {
        if t.kind == tape_format::tracks::Kind::Eject {
            continue;
        }
        n += 1;
    }
    n
}

fn extract_task_from_loaded(loaded: &[tape_format::tracks::Track]) -> String {
    loaded
        .first()
        .filter(|t| t.kind == tape_format::tracks::Kind::Task)
        .and_then(|t| t.payload.get("prompt").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_owned()
}

fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    // Canonicalize when both exist; fall back to lexical compare otherwise so
    // a not-yet-existing `--out` that names the input is still caught.
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
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
