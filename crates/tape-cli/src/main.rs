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
    /// Read-only analytics over a single cassette. Phase-2 of #31:
    /// adds `--format json` with a pinned `schema_version` so CI /
    /// dashboards / scripts can pin against a stable wire shape.
    /// Library/compare and pricing remain Phase-3+ work.
    Stats {
        file: std::path::PathBuf,
        /// Output format. `text` (default) preserves Phase-1
        /// byte-for-byte; `json` emits the pinned `schema_version
        /// 1.0` shape from issue #157, pretty-printed with a trailing
        /// newline (matches `tape verify --json`'s convention).
        #[arg(long, default_value = "text", value_parser = ["text", "json"])]
        format: String,
    },
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
    /// Generate a new `tape/v0` cassette from a bundled template.
    ///
    /// Phase-1 of issue #99: only the `minimal` template, only literal
    /// `{{task}}` substitution. The bundled-catalog, `--from` clone-shape,
    /// `--template-path`, and `.taperc::new` flows are Phase 2+.
    New {
        /// Output cassette path. Refuses if it already exists unless
        /// `--force` is supplied.
        out: std::path::PathBuf,
        /// One-line description of what the cassette represents. Lands
        /// in `meta.task`, in the task event's `prompt`, and in the
        /// liner-notes. Plain UTF-8; rejected if it contains a `"`,
        /// `\\`, `\n`, `\r`, or control character (keeps the literal
        /// `{{task}}` substitution JSONL-safe).
        #[arg(long)]
        task: String,
        /// Overwrite the output path if it already exists.
        #[arg(short = 'f', long)]
        force: bool,
        /// Override `meta.created_at` / the task event's `ts`. Defaults
        /// to `now()`. The `--created-at <RFC3339>` + `--recorder-agent`
        /// pair exists so fixture-regeneration tests get a deterministic
        /// output for the same inputs.
        #[arg(long)]
        created_at: Option<String>,
        /// Override `meta.recorder.agent`. Defaults to
        /// `tape-cli/<crate-version>+new+minimal`.
        #[arg(long)]
        recorder_agent: Option<String>,
    },
    /// Manage the `meta.recap` field — a 1–2 sentence summary suitable
    /// for pasting into Slack / Linear / Jira / PR descriptions.
    ///
    /// Phase-1 of issue #105: hand-written recaps only via `--set` /
    /// `--clear` / `--list`. The LLM-driven `--auto` and template flags
    /// are deferred to Phase 2 (blocked on the same judge-model wiring
    /// `tape diff --judge` is waiting on).
    Recap {
        /// Input cassette.
        file: std::path::PathBuf,
        /// Set `meta.recap` to this text and append a `set` entry to
        /// `meta.recaps[]`. ≤280 chars, no newline, non-empty. Mutually
        /// exclusive with `--clear` and `--list`.
        #[arg(long, conflicts_with_all = ["clear", "list"])]
        set: Option<String>,
        /// Clear `meta.recap` and append a `clear` entry to
        /// `meta.recaps[]`. Mutually exclusive with `--set` and `--list`.
        #[arg(long, conflicts_with_all = ["set", "list"])]
        clear: bool,
        /// Print `meta.recap` to stdout. Exit 4 if the cassette has no
        /// recap set. Read-only — no output cassette is written.
        /// Mutually exclusive with `--set` and `--clear`.
        #[arg(long, conflicts_with_all = ["set", "clear"])]
        list: bool,
        /// Output path for `--set` / `--clear`. Default
        /// `<basename>.recap.tape` next to the input. Refuses if equal
        /// to the input path.
        #[arg(short = 'o', long)]
        out: Option<std::path::PathBuf>,
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
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Cmd::Verify { file, json } => cmd_verify(&file, json),
        Cmd::Ls { file } => cmd_ls(&file),
        Cmd::Stats { file, format } => cmd_stats(&file, &format),
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
        Cmd::New {
            out,
            task,
            force,
            created_at,
            recorder_agent,
        } => cmd_new(&out, &task, force, created_at, recorder_agent),
        Cmd::Recap {
            file,
            set,
            clear,
            list,
            out,
        } => cmd_recap(&file, set, clear, list, out),
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
    let task_text = if task.is_empty() { cmd.join(" ") } else { task };

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
        // #106 step 1: explicit Claude Code adapter. Step 2 will wire
        // a `--runtime` flag + auto-detection in front of this default.
        runtime: tape_record::runtime::claude_code_adapter(),
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

fn cmd_diff(a: &std::path::Path, b: &std::path::Path, all: bool, format: &str) -> Result<()> {
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

/// Phase-1 of issue #105. Hand-managed `meta.recap` via three pairwise
/// exclusive subflags. Unlike `cmd_annotate`, the write path does **not**
/// go through the eject pipeline — we're editing only `meta.yaml`, so a
/// straight zip-rewrite via `PendingTape::write_to` is the smaller
/// surface (no `EjectOptions` field churn, no `tool_eject` deck
/// inheritance changes, no risk of perturbing track payloads or
/// artifacts on round-trip).
///
/// Defense-in-depth: `--set` does **not** route through the redaction
/// engine (no model call, no §3.7 scan). The post-write `tape verify`
/// at step 7 is the backstop — `LEAKED_SECRET_IN_META` (§10.5) fires on
/// any secret-shaped recap text, exits 3, and removes the corrupt
/// output. The recap field's narrow shape (≤280 chars, no newline) also
/// makes the leak surface small. A future `--auto` flag will run the
/// model-generated text through the redaction engine pre-write, the
/// same way `cmd_annotate` does for `--note`.
/// Embedded Step-1 `minimal` template. One template ships in Step 1 of
/// #99; an `include_dir!`-based catalog lands when Step 2 adds the
/// other seven.
mod templates {
    pub const MINIMAL_VERSION: &str = "1";
    pub const MINIMAL_LINER: &str = include_str!("../templates/minimal/liner-notes.md");
    pub const MINIMAL_TRACKS: &str = include_str!("../templates/minimal/tracks.jsonl");
}

/// Validate the `--task` value for `tape new`. Rejects empty strings,
/// JSONL-unsafe characters (`"`, `\\`, `\n`, `\r`, controls), and any
/// `{{` sequence (which would cascade through later placeholder
/// substitutions and silently diverge `meta.task` from
/// `tracks[0].payload.prompt`). On rejection, prints a
/// `NEW_MISSING_PLACEHOLDER` diagnostic and exits with code 2.
fn validate_new_task(task: &str) {
    if task.is_empty() {
        eprintln!("tape new: NEW_MISSING_PLACEHOLDER — --task must be non-empty");
        std::process::exit(2);
    }
    if let Some(bad) = task
        .chars()
        .find(|c| *c == '"' || *c == '\\' || *c == '\n' || *c == '\r' || c.is_control())
    {
        eprintln!(
            "tape new: NEW_MISSING_PLACEHOLDER — --task contains disallowed character {bad:?}; \
             rejected to keep the literal {{{{task}}}} substitution JSONL-safe"
        );
        std::process::exit(2);
    }
    if task.contains("{{") {
        eprintln!(
            "tape new: NEW_MISSING_PLACEHOLDER — --task must not contain `{{{{` \
             (would cascade through later placeholder substitutions)"
        );
        std::process::exit(2);
    }
}

/// Phase-1 of issue #99. Materializes a new cassette from the bundled
/// `minimal` template via literal `{{...}}` substitution, builds a
/// fresh `Meta` with a `meta.new` provenance block, and writes the
/// result through `PendingTape::write_to`. `tape verify` runs as a
/// post-write gate so any future template-content mistake is caught
/// before the file is left on disk.
fn cmd_new(
    out: &std::path::Path,
    task: &str,
    force: bool,
    created_at_override: Option<String>,
    recorder_agent_override: Option<String>,
) -> Result<()> {
    // 1. Validate --task. Literal substitution forbids characters that
    //    would un-balance the JSONL or smuggle in another track, and
    //    forbids `{{` so the value can't cascade through later
    //    placeholder substitutions.
    validate_new_task(task);

    // 2. Output-exists check.
    if out.exists() && !force {
        eprintln!(
            "tape new: NEW_OUTPUT_EXISTS — {} already exists (re-run with --force to overwrite)",
            out.display()
        );
        std::process::exit(2);
    }

    // 3. Resolve timestamps. Both task `ts` and `meta.created_at` use
    //    the same value so the cassette reads "this all happened at the
    //    same instant" — appropriate for a synthesized cassette. eject
    //    `ts` and `meta.ejected_at` get the same value too; SPEC §5.2
    //    allows equal `ts` and SPEC §3.1 allows `created_at == ejected_at`.
    let created_at = match created_at_override.as_deref() {
        Some(s) => {
            if chrono::DateTime::parse_from_rfc3339(s).is_err() {
                eprintln!("tape new: --created-at must be RFC-3339 (got {s:?})");
                std::process::exit(2);
            }
            s.to_owned()
        }
        None => chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
    };
    let ejected_at = created_at.clone();
    let recorder_agent = recorder_agent_override
        .unwrap_or_else(|| format!("tape-cli/{}+new+minimal", env!("CARGO_PKG_VERSION")));

    // 4. Substitute template placeholders. Literal `String::replace`,
    //    no expression language — the rule is "grep '{{' templates/`
    //    should always show every active placeholder."
    let liner_md = templates::MINIMAL_LINER.replace("{{task}}", task);
    let tracks_jsonl = templates::MINIMAL_TRACKS
        .replace("{{task}}", task)
        .replace("{{created_at}}", &created_at)
        .replace("{{ejected_at}}", &ejected_at);

    // 5. Build the Meta. The id is a deterministic UUIDv7 derived from
    //    (created_at, recorder_agent, task) so that two runs with the
    //    same overrides produce byte-identical track / meta content
    //    (the deterministic-output property in Principal's test plan).
    let id = derive_uuid_v7(&created_at, &recorder_agent, task);
    let meta = tape_format::meta::Meta {
        tape_version: tape_format::TAPE_VERSION.into(),
        id,
        created_at: created_at.clone(),
        ejected_at: ejected_at.clone(),
        task: task.to_owned(),
        recorder: tape_format::meta::Recorder {
            agent: recorder_agent.clone(),
            user: None,
        },
        outcome: tape_format::meta::Outcome::Unknown,
        models: vec![],
        tools: vec![],
        tool_budget: None,
        redaction_summary: None,
        label: None,
        recap: None,
        recaps: vec![],
        new_block: Some(tape_format::meta::NewBlock {
            template_id: "minimal".into(),
            template_version: templates::MINIMAL_VERSION.into(),
            generated_at: created_at.clone(),
            placeholders_filled: vec!["task".into()],
        }),
    };
    let meta_yaml = meta
        .to_yaml()
        .map_err(|e| anyhow::anyhow!("serialize meta.yaml: {e}"))?;

    // 6. Write the cassette.
    let pending = tape_format::writer::PendingTape {
        meta_yaml,
        liner_md,
        tracks_jsonl,
        redactions_json: None,
        artifacts: std::collections::BTreeMap::new(),
    };
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("create {}: {e}", parent.display()))?;
        }
    }
    pending
        .write_to(out)
        .map_err(|e| anyhow::anyhow!("write {}: {e}", out.display()))?;

    // 7. Verify the output. SPEC §10.5's defense-in-depth secret_scan
    //    runs as part of `tape verify`, so a template-text mistake
    //    (e.g. a stray API-key-shaped substring) is caught here. The
    //    bundled `minimal` template is hand-checked clean; this is the
    //    backstop for future templates / user substitutions.
    let written = tape_format::reader::RawTape::open(out)?;
    let report = tape_format::verify::verify(&written);
    if !report.is_valid() {
        let _ = std::fs::remove_file(out);
        let codes: Vec<&'static str> = report.errors().map(|d| d.code.as_str()).collect();
        eprintln!(
            "tape new: NEW_TEMPLATE_INVALID — generated cassette failed tape verify ({}); removed {}",
            codes.join(","),
            out.display()
        );
        std::process::exit(3);
    }

    println!("ok: wrote {} (template=minimal)", out.display());
    Ok(())
}

/// Derive a deterministic UUIDv7-shaped id from the three inputs that
/// `tape new`'s test plan pins for byte-equality: `--created-at`,
/// `--recorder-agent`, and `--task`. The high 48 bits encode the
/// `created_at` instant in milliseconds-since-epoch (per RFC 9562); the
/// remaining 74 random bits are deterministically derived from a
/// `blake3(created_at || recorder_agent || task)` digest. The version
/// nibble (`7`) and variant bits (`0b10`) are set per spec so the result
/// passes any `UUIDv7` syntactic check.
fn derive_uuid_v7(created_at: &str, recorder_agent: &str, task: &str) -> String {
    let unix_ms = chrono::DateTime::parse_from_rfc3339(created_at)
        .map(|dt| u64::try_from(dt.timestamp_millis().max(0)).unwrap_or(0))
        .unwrap_or(0);
    let mut hasher = blake3::Hasher::new();
    hasher.update(created_at.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(recorder_agent.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(task.as_bytes());
    let digest = hasher.finalize();
    let dbytes = digest.as_bytes();

    let mut bytes = [0u8; 16];
    bytes[0] = ((unix_ms >> 40) & 0xff) as u8;
    bytes[1] = ((unix_ms >> 32) & 0xff) as u8;
    bytes[2] = ((unix_ms >> 24) & 0xff) as u8;
    bytes[3] = ((unix_ms >> 16) & 0xff) as u8;
    bytes[4] = ((unix_ms >> 8) & 0xff) as u8;
    bytes[5] = (unix_ms & 0xff) as u8;
    // bytes[6] high nibble is version=7; low nibble is rand_a high.
    bytes[6] = 0x70 | (dbytes[0] & 0x0f);
    bytes[7] = dbytes[1];
    // bytes[8] high two bits are variant 0b10; low 6 bits are rand_b high.
    bytes[8] = 0x80 | (dbytes[2] & 0x3f);
    bytes[9..16].copy_from_slice(&dbytes[3..10]);

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

fn cmd_recap(
    file: &std::path::Path,
    set: Option<String>,
    clear: bool,
    list: bool,
    out: Option<std::path::PathBuf>,
) -> Result<()> {
    // 1. Verify exactly one mode flag is set. clap's `conflicts_with_all`
    //    handles the pairwise-exclusion side; this is the
    //    "at-least-one" half (clap can't model that cleanly when each
    //    flag also has a default like `bool: false` / `Option: None`).
    let mode_count = [set.is_some(), clear, list].iter().filter(|b| **b).count();
    if mode_count == 0 {
        eprintln!("tape recap: one of --set <text>, --clear, --list is required");
        std::process::exit(2);
    }

    // 2. `--list` is read-only: open meta.yaml, print recap or exit 4.
    //    Done before any output-path resolution.
    if list {
        let raw = open_input(file, "tape recap");
        let meta = parse_meta(&raw, "tape recap");
        if let Some(r) = meta.recap.as_deref() {
            println!("{r}");
            return Ok(());
        }
        eprintln!("tape recap: no recap set on {}", file.display());
        std::process::exit(4);
    }

    // 3. Mutating modes need an output path. Same shape as `cmd_annotate`.
    let out_path = if let Some(p) = out {
        p
    } else {
        let stem = file
            .file_stem()
            .map_or_else(|| "tape".to_owned(), |s| s.to_string_lossy().into_owned());
        let parent = file.parent().unwrap_or_else(|| std::path::Path::new("."));
        parent.join(format!("{stem}.recap.tape"))
    };
    if same_path(file, &out_path) {
        eprintln!("tape recap: --out must differ from <file>");
        std::process::exit(2);
    }

    // 4. Load the input cassette and parse meta.
    let raw = open_input(file, "tape recap");
    let mut meta = parse_meta(&raw, "tape recap");
    let prior_recap = meta.recap.clone();

    // 5. Apply the requested edit.
    let kind: tape_format::meta::RecapKind = if let Some(text) = set.as_deref() {
        if text.is_empty() {
            eprintln!("tape recap: --set text must be non-empty");
            std::process::exit(2);
        }
        if let Err(msg) = tape_format::meta::validate_recap_text(text) {
            eprintln!("tape recap: --set rejected: {msg}");
            std::process::exit(2);
        }
        meta.recap = Some(text.to_owned());
        tape_format::meta::RecapKind::Set
    } else {
        // --clear
        meta.recap = None;
        tape_format::meta::RecapKind::Clear
    };

    let entry = tape_format::meta::RecapEntry {
        applied_at: chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
        kind,
        prior_recap,
        new_recap: meta.recap.clone(),
    };
    meta.recaps.push(entry);

    // 6. Rewrite the zip. Everything but meta.yaml passes through
    //    byte-identical so tracks, liner notes, artifacts, and the
    //    existing redactions.json are preserved.
    let new_meta_yaml = meta
        .to_yaml()
        .map_err(|e| anyhow::anyhow!("re-serialize meta.yaml: {e}"))?;
    let pending = tape_format::writer::PendingTape {
        meta_yaml: new_meta_yaml,
        liner_md: raw.liner_md.clone().unwrap_or_default(),
        tracks_jsonl: raw.tracks_jsonl.clone().unwrap_or_default(),
        redactions_json: raw.redactions_json.clone(),
        artifacts: raw.artifacts.clone().into_iter().collect(),
    };
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("create {}: {e}", parent.display()))?;
        }
    }
    pending
        .write_to(&out_path)
        .map_err(|e| anyhow::anyhow!("write {}: {e}", out_path.display()))?;

    // 7. Post-write verify (exit 3 on regression; remove the corrupt
    //    output so the caller doesn't have to clean up). Same posture
    //    `cmd_annotate` takes.
    let written = tape_format::reader::RawTape::open(&out_path)?;
    let report = tape_format::verify::verify(&written);
    if !report.is_valid() {
        let _ = std::fs::remove_file(&out_path);
        let codes: Vec<&'static str> = report.errors().map(|d| d.code.as_str()).collect();
        eprintln!(
            "tape recap: output failed tape verify ({}); removed {}",
            codes.join(","),
            out_path.display()
        );
        std::process::exit(3);
    }

    let action_label = if set.is_some() { "set" } else { "cleared" };
    println!("ok: {action_label} recap on {}", out_path.display());
    Ok(())
}

/// Wrap `RawTape::open` with a CLI-facing exit-2 on failure. Used by
/// recap (and could be by future read-only commands) so error reporting
/// is consistent.
fn open_input(path: &std::path::Path, cmd: &str) -> tape_format::reader::RawTape {
    match tape_format::reader::RawTape::open(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{cmd}: failed to open {}: {e}", path.display());
            std::process::exit(2);
        }
    }
}

/// Wrap `Meta::parse` with the same exit-2-on-failure CLI surface.
fn parse_meta(raw: &tape_format::reader::RawTape, cmd: &str) -> tape_format::meta::Meta {
    let Some(meta_yaml) = raw.meta_yaml.as_deref() else {
        eprintln!("{cmd}: input cassette is missing meta.yaml");
        std::process::exit(2);
    };
    match tape_format::meta::Meta::parse(meta_yaml) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{cmd}: meta.yaml does not parse: {e}");
            std::process::exit(2);
        }
    }
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

fn load_tracks(
    path: &std::path::Path,
) -> Result<(
    tape_format::reader::RawTape,
    Vec<tape_format::tracks::Track>,
)> {
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

/// Issue #31 Step-1. Read meta + tracks (already opened by
/// `load_tracks`), pull a redaction count out of the optional
/// `redactions.json`, and hand off to `tape_play::render_stats`. No I/O
/// beyond what `load_tracks` already does.
fn cmd_stats(file: &std::path::Path, format: &str) -> Result<()> {
    let (raw, tracks) = load_tracks(file)?;
    let meta_yaml = raw
        .meta_yaml
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("input cassette is missing meta.yaml"))?;
    let meta = tape_format::meta::Meta::parse(meta_yaml)?;
    let redactions_count = raw.redactions_json.as_deref().map(|s| {
        serde_json::from_str::<serde_json::Value>(s)
            .ok()
            .and_then(|v| v.as_array().map(|a| a.len() as u64))
            .unwrap_or(0)
    });
    match format {
        // Phase-1 byte-for-byte text. clap's value_parser already
        // rejects anything other than `text` / `json`, so a bare
        // `_` arm here would be dead code.
        "text" => print!(
            "{}",
            tape_play::render_stats(&meta, &tracks, redactions_count)
        ),
        "json" => {
            let value = tape_play::render_stats_json(&meta, &tracks, redactions_count);
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        other => unreachable!("clap should reject this: {other}"),
    }
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
        print!(
            "{}",
            tape_play::render_summary_view(meta_yaml, liner, &tracks)
        );
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
