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
    /// Read-only analytics over a single cassette. Phase-3 of #31:
    /// adds `--with-cost` for the bundled pricing-table dollar
    /// estimate column. Library/compare and the user-supplied
    /// `--pricing-file` flow remain Phase-4+ work.
    Stats {
        file: std::path::PathBuf,
        /// Output format. `text` (default) preserves Phase-1
        /// byte-for-byte; `json` emits the pinned `schema_version
        /// 1.0` shape from issue #157, pretty-printed with a trailing
        /// newline (matches `tape verify --json`'s convention).
        #[arg(long, default_value = "text", value_parser = ["text", "json"])]
        format: String,
        /// Enable the dollar-cost estimate column. Uses the bundled
        /// pricing table; appends a stale-guard warning when the
        /// table is older than 90 days. Text-only for now; pairing
        /// with `--format json` is rejected (the JSON schema bump
        /// lands with the per-model breakdown in Phase 4). Issue
        /// #168.
        #[arg(long)]
        with_cost: bool,
        /// Override the bundled pricing table with one loaded from
        /// the given TOML file. Same schema as the bundled table
        /// (`last_updated = "YYYY-MM-DD"` plus one or more
        /// `[[model]]` rows with `vendor` / `model` / `input_per_mtok`
        /// / `output_per_mtok`). Replace-not-merge: rows the file
        /// omits land in the unpriced bucket for this invocation.
        /// Stale-guard uses the file's `last_updated`. No effect
        /// without `--with-cost`. Issue #181.
        #[arg(long, value_name = "PATH")]
        pricing_file: Option<std::path::PathBuf>,
    },
    /// Compare two tapes.
    Diff {
        a: std::path::PathBuf,
        b: std::path::PathBuf,
        #[arg(long)]
        all: bool,
        #[arg(long, default_value = "text")]
        format: String,
        /// Enable judge-narrated alignment. Substantive diff entries
        /// get a one-to-three-sentence behavioral summary attached
        /// from the configured judge model. Overrides
        /// `JudgeConfig::model` from `.taperc` for this invocation.
        /// Requires a `judge:` block in `.taperc` and the API-key
        /// env var named in `judge.api_key_env`.
        #[arg(long, value_name = "MODEL")]
        judge: Option<String>,
        /// Cap the number of judge calls made by this invocation
        /// (default 25). Substantive entries beyond the cap render
        /// with `[narration skipped — budget exceeded]`. Ignored
        /// when `--judge` is not supplied.
        #[arg(long, value_name = "N", default_value_t = 25)]
        judge_budget: u32,
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
        /// Override `OpenAI` upstream URL (default: env var or `https://api.openai.com`).
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
    /// Step-2 of issue #99 (#162) bundles `test-fixture` and
    /// `bug-investigation` alongside the original `minimal`.
    /// `--list-templates` / `--describe-template`, the `.taperc::new`
    /// section, `--set <k>=<v>` for richer placeholders, `--from`
    /// clone-shape, `--template-path` for user-supplied templates,
    /// and `--from`/auto-tag emission are still Phase 3+.
    New {
        /// Output cassette path. Refuses if it already exists unless
        /// `--force` is supplied. Not consumed when `--list-templates`
        /// or `--describe-template` is set (those introspection flags
        /// exit before the generation path).
        out: Option<std::path::PathBuf>,
        /// Template id. Built-ins: `minimal`, `test-fixture`,
        /// `bug-investigation`. Unknown values exit 2 with
        /// `NEW_TEMPLATE_NOT_FOUND`. Resolution order (issue #190):
        /// this flag > `.taperc::new.default_template` > `minimal`
        /// terminal fallback (preserves pre-#190 default for
        /// existing scripts; back-compat path (b) of #190 ACs —
        /// callers wanting hard failure on missing config can
        /// `.taperc`-pin a deliberate value and remove `minimal`
        /// from their workflow).
        #[arg(long, conflicts_with_all = ["list_templates", "describe_template"])]
        template: Option<String>,
        /// One-line description of what the cassette represents. Lands
        /// in `meta.task`, in the task event's `prompt`, and in the
        /// liner-notes. Plain UTF-8; rejected if it contains a `"`,
        /// `\\`, `\n`, `\r`, or control character (keeps the literal
        /// `{{task}}` substitution JSONL-safe). Required for templates
        /// whose `template.yaml` declares `task: required: true`
        /// (`minimal`, `bug-investigation`); optional for templates
        /// with no required placeholders (`test-fixture`).
        #[arg(long, conflicts_with_all = ["list_templates", "describe_template"])]
        task: Option<String>,
        /// Overwrite the output path if it already exists.
        #[arg(short = 'f', long, conflicts_with_all = ["list_templates", "describe_template"])]
        force: bool,
        /// Override `meta.created_at` / the task event's `ts`. Defaults
        /// to `now()`. The `--created-at <RFC3339>` + `--recorder-agent`
        /// pair exists so fixture-regeneration tests get a deterministic
        /// output for the same inputs.
        #[arg(long, conflicts_with_all = ["list_templates", "describe_template"])]
        created_at: Option<String>,
        /// Override `meta.recorder.agent`. Defaults to
        /// `tape-cli/<crate-version>+new+<template>`.
        #[arg(long, conflicts_with_all = ["list_templates", "describe_template"])]
        recorder_agent: Option<String>,
        /// Print the bundled template catalog (one line per template:
        /// id, version, required-task flag, description) and exit 0.
        /// Mutually exclusive with `--describe-template` and with the
        /// generation flags. Writes nothing to disk. (Issue #179.)
        #[arg(long, conflicts_with = "describe_template")]
        list_templates: bool,
        /// Print a full description of one bundled template
        /// (placeholders, track count, rendered liner-notes) and exit 0.
        /// Unknown ids exit 2. Mutually exclusive with
        /// `--list-templates` and with the generation flags. Writes
        /// nothing to disk. (Issue #179.)
        #[arg(long, value_name = "ID")]
        describe_template: Option<String>,
        /// Override a template default for this invocation. Repeatable.
        /// `KEY=VALUE` form; the right-hand side may contain further `=`
        /// (split on the first `=` only). Recognized keys are
        /// template-scoped — e.g. `--set required-task=false` on
        /// `minimal` makes `--task` optional. Unknown keys exit 2 with
        /// `NEW_UNKNOWN_OVERRIDE_KEY`. Duplicate keys: last-wins with a
        /// stderr warning. (Issue #188 / Step-4 of #99.)
        #[arg(
            long = "set",
            value_name = "KEY=VALUE",
            value_parser = parse_set_kv,
            conflicts_with_all = ["list_templates", "describe_template"],
        )]
        set: Vec<(String, String)>,
    },
    /// Manage the `meta.recap` field — a 1–2 sentence summary suitable
    /// for pasting into Slack / Linear / Jira / PR descriptions.
    ///
    /// Phase-1 of issue #105 shipped hand-managed `--set` / `--clear` /
    /// `--list`. Phase-2 (issue #151) adds `--auto`: ask the configured
    /// judge model in `.taperc::judge:` to draft the recap, validate it
    /// with the same `validate_recap_text` rules `--set` uses, and write
    /// through the same single-blob path.
    Recap {
        /// Input cassette.
        file: std::path::PathBuf,
        /// Set `meta.recap` to this text and append a `set` entry to
        /// `meta.recaps[]`. ≤280 chars, no newline, non-empty. Mutually
        /// exclusive with `--clear`, `--list`, and `--auto`.
        #[arg(long, conflicts_with_all = ["clear", "list", "auto"])]
        set: Option<String>,
        /// Clear `meta.recap` and append a `clear` entry to
        /// `meta.recaps[]`. Mutually exclusive with `--set`, `--list`,
        /// and `--auto`.
        #[arg(long, conflicts_with_all = ["set", "list", "auto"])]
        clear: bool,
        /// Print `meta.recap` to stdout. Exit 4 if the cassette has no
        /// recap set. Read-only — no output cassette is written.
        /// Mutually exclusive with `--set`, `--clear`, and `--auto`.
        #[arg(long, conflicts_with_all = ["set", "clear", "auto"])]
        list: bool,
        /// Ask the configured judge model (see `.taperc::judge:`) to
        /// draft a recap and write it after validation + the model
        /// client's defense-in-depth scan. Mutually exclusive with
        /// `--set` / `--clear` / `--list`. Issue #151.
        #[arg(long, conflicts_with_all = ["set", "clear", "list"])]
        auto: bool,
        /// Override the judge model for this `tape recap --auto`
        /// invocation only. Precedence: this flag >
        /// `.taperc::recap.default_model` > `.taperc::judge.model`.
        /// Leaves other tape-judge consumers (`tape diff --judge`,
        /// `tape relinernote`) unchanged. Only meaningful with
        /// `--auto`. (Issue #198 / Step-3 of #105.)
        #[arg(long, value_name = "MODEL", conflicts_with_all = ["set", "clear", "list"])]
        model: Option<String>,
        /// Output path for `--set` / `--clear` / `--auto`. Default
        /// `<basename>.recap.tape` next to the input. Refuses if equal
        /// to the input path.
        #[arg(short = 'o', long)]
        out: Option<std::path::PathBuf>,
    },
    /// Manage `meta.tags[]` — orthogonal multi-valued facet labels for
    /// filing, search, and CI gates.
    ///
    /// Step-1 vertical slice of issue #93: hand-managed tags via
    /// `--add` / `--remove` / `--list`. The `--auto`, closed-vocabulary
    /// (`--verify`), audit-trail (`meta.taggings[]`), `.taperc::tag:`
    /// section, count/length cap diagnostics, and plugin slash commands
    /// are deferred to Steps 2–5 as separate follow-ons.
    Tag {
        /// Input cassette.
        file: std::path::PathBuf,
        /// Add a tag to `meta.tags[]`. Repeatable. Idempotent — adding a
        /// tag that already exists is a silent no-op. Composes with
        /// `--remove`. Mutually exclusive with `--list`.
        #[arg(long, conflicts_with_all = ["list"])]
        add: Vec<String>,
        /// Remove a tag from `meta.tags[]`. Repeatable. Removing an
        /// absent tag is a silent no-op. Composes with `--add`.
        /// Mutually exclusive with `--list`.
        #[arg(long, conflicts_with_all = ["list"])]
        remove: Vec<String>,
        /// Print `meta.tags[]` one entry per line on stdout and exit.
        /// Read-only — no output cassette is written. Mutually exclusive
        /// with `--add` / `--remove` / `--in-place` / `--dry-run`.
        #[arg(long, conflicts_with_all = ["add", "remove", "in_place", "dry_run"])]
        list: bool,
        /// Print the would-be diff (added / removed) and the resulting
        /// tag list, then exit 4. Does NOT write an output cassette.
        /// Mutually exclusive with `--list`.
        #[arg(long, conflicts_with_all = ["list"])]
        dry_run: bool,
        /// Atomic rewrite of the input cassette in place (temp + rename
        /// via the same path the writer would have used for `-o`).
        /// Mutually exclusive with `-o` and `--list`.
        #[arg(long, conflicts_with_all = ["out", "list"])]
        in_place: bool,
        /// Output path. Default: `<basename>.tagged.tape` next to the
        /// input. Refuses if equal to the input path unless `--in-place`
        /// is set (in which case use `--in-place`, not `-o <input>`).
        #[arg(short = 'o', long, conflicts_with_all = ["in_place"])]
        out: Option<std::path::PathBuf>,
    },
    /// Append an annotation to an existing tape, writing a new cassette.
    ///
    /// CLI counterpart to the deck's `tape.annotate` tool (issue #74).
    /// See also `.taperc::annotate` (issue #192) for the
    /// `default_actor` / `default_by` / `editor` fallback fields.
    /// `--force-resign` remains a follow-up.
    Annotate {
        /// Input cassette to annotate.
        file: std::path::PathBuf,
        /// Annotation body. SPEC §5.5.7 `note` field. Mutually exclusive
        /// with `--editor` / `--import`; exactly one of the three MUST
        /// be supplied.
        #[arg(
            long,
            required_unless_present_any = ["editor", "import"],
            conflicts_with_all = ["editor", "import"],
        )]
        note: Option<String>,
        /// Compose the annotation body in `$VISUAL` / `$EDITOR` / `vi`
        /// (in that resolution order). Mutually exclusive with `--note`
        /// and `--import`; exactly one of the three MUST be supplied. An
        /// empty body (after comment-strip) cancels the operation
        /// cleanly with exit 0 and no output cassette. (Issue #158.)
        #[arg(long, conflicts_with_all = ["note", "import"])]
        editor: bool,
        /// Read the annotation body verbatim from `<PATH>`. UTF-8;
        /// trailing whitespace and newlines are trimmed but no `#`
        /// comment stripping. Empty-after-trim cancels with exit 0.
        /// 16 KiB cap. Mutually exclusive with `--note` and `--editor`;
        /// exactly one of the three MUST be supplied. (Issue #173.)
        #[arg(long, conflicts_with_all = ["note", "editor"])]
        import: Option<std::path::PathBuf>,
        /// Parent step the annotation hangs off. Validated against the
        /// tape's existing tracks: 1 ≤ N < `new_step`.
        #[arg(long)]
        step: Option<u64>,
        /// Free-form attribution shown in CLI output / `--json`. Defaults
        /// to `$USER`. Not stored in the payload (SPEC §5.5.7 is
        /// `{by, note}` only).
        #[arg(long)]
        actor: Option<String>,
        /// Who is making the note. Default `human` for the CLI when
        /// neither this flag nor `.taperc::annotate.default_by` is
        /// set (the deck defaults to `agent`). Resolution order
        /// (issue #192): this flag wins, then
        /// `.taperc::annotate.default_by`, else `"human"`. The
        /// value-set `{"agent", "human"}` validates the *resolved*
        /// value; an invalid `.taperc` value exits 2 with the
        /// config path named.
        #[arg(long, value_parser = ["agent", "human"])]
        by: Option<String>,
        /// Output path. Default: `<basename>.annotated.tape` next to the
        /// input. Refuses if equal to the input path; use `--in-place`
        /// for atomic rewrite of the input. Mutually exclusive with
        /// `--in-place`.
        #[arg(short = 'o', long, conflicts_with = "in_place")]
        out: Option<std::path::PathBuf>,
        /// Atomic rewrite of the input cassette via a sibling temp file
        /// followed by a rename. The post-write verify gate runs before
        /// the rename; on failure the input is preserved untouched and
        /// exit 3 is returned. Mutually exclusive with `--out`.
        /// (Issue #158.)
        #[arg(long, conflicts_with = "out")]
        in_place: bool,
        /// Override the annotation timestamp. Must be RFC-3339 (`Z`
        /// suffix). MUST be ≥ the last track's `ts` to preserve SPEC §5.2
        /// monotonicity.
        #[arg(long)]
        ts: Option<String>,
        /// Emit the §3.10 schema-v1 success summary on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Render a cassette to a portable, non-Claude-Code-friendly format.
    ///
    /// Step-1 vertical slice of issue #8: GitHub-flavored Markdown only,
    /// written to `<basename>.md` by default. `--format html` /
    /// `--format both`, themes, filter chips, the post-render
    /// defense-in-depth re-scan, `--audience` presets, `--strip-internal`,
    /// `--include-payloads`, `--inline-images`, the `.taperc::export:`
    /// section, and the `/tape:tape-export` plugin slash command are all
    /// Step 2–4 follow-ons.
    Export {
        /// Input cassette.
        file: std::path::PathBuf,
        /// Output format. Step 1 only supports `md`; the `html` /
        /// `both` values exit 2 with a TODO diagnostic naming the
        /// follow-on step until Step 2 lands.
        #[arg(short = 'f', long, default_value = "md")]
        format: String,
        /// Output path. Default: `<basename>.md` next to the input.
        /// Refuses if equal to the input path.
        #[arg(short = 'o', long)]
        out: Option<std::path::PathBuf>,
    },
    /// Regenerate `liner-notes.md` for an existing cassette via the
    /// configured judge model in `.taperc::judge:`.
    ///
    /// Phase-1 vertical slice of issue #71: bundled `default` prompt
    /// template only; `--template <path>` / `--template-id <id>`, the
    /// interactive confirmation UX, JSON `--report` sidecar,
    /// `.taperc::relinernote:` config, and pricing integration are
    /// follow-on PRs.
    Relinernote {
        /// Input cassette.
        file: std::path::PathBuf,
        /// Render the prompt with placeholders substituted, print it
        /// to stdout, and exit 0 without making an HTTP call.
        #[arg(long)]
        dry_run: bool,
        /// Override `.taperc::judge::model` for one invocation.
        /// Empty means "use the value the config provides".
        #[arg(long)]
        model: Option<String>,
        /// Output path. Default: `<basename>.relinernote.tape` next to
        /// the input. Refuses if equal to the input path.
        #[arg(short = 'o', long)]
        out: Option<std::path::PathBuf>,
        /// Bundled prompt template name. Today: `default` (canonical
        /// prose) or `terse` (bulleted, ~100-200 words target).
        /// Both render the same four required H2 sections (SPEC
        /// §4.1) so output-validation is unchanged. Unknown names
        /// exit 2 with `RELINER_TEMPLATE_NOT_FOUND`. (Issue #196.)
        #[arg(long, default_value = "default")]
        template: String,
    },
    /// Strip absolute `$HOME`-style file paths from a cassette and write
    /// a NEW cassette next to the input. Phase 1 of issue #42 — see
    /// issue #204 for the full identifier set.
    ///
    /// This slice ships exactly one rule (`unix_home_path`). The
    /// Phase-2+ rule classes (`windows_user_path`,
    /// `unix_username_prompt`, `git_remote_user`, `hostname_meta`,
    /// `env_user`, etc.), the `--rules` / `--disable` flags, the
    /// `--map` / `tape unanon` reversibility, the `--aggressive` free-
    /// text scan, the `--salt` / `--dry-run` flags, the
    /// `meta.anonymizations[]` audit array, the
    /// `LEAKED_IDENTIFIER_POST_ANON` `tape verify` diagnostic, and the
    /// `.taperc::anon:` config block are all deferred follow-on slices.
    Anon {
        /// Input cassette. Read-only; never mutated.
        file: std::path::PathBuf,
        /// Output cassette. Default: `<basename>.anon.tape` next to the
        /// input. Refuses if equal to `<file>` (per #42 §3.1: anon
        /// NEVER writes back to the input). Refuses if the output
        /// path already exists (no `--force` in Phase 1).
        #[arg(short = 'o', long)]
        out: Option<std::path::PathBuf>,
    },
    /// Convert a cassette into OpenTelemetry traces (OTLP/JSON). Phase
    /// 1 of issue #88 carved per #209: one span per track, flat walk,
    /// hand-rolled OTLP/JSON struct (no `opentelemetry` crate dep).
    ///
    /// Out of scope for Phase 1 (deferred to #88 Phase 2+): protobuf /
    /// gRPC push, `--endpoint` / `--headers`, env-var fallbacks,
    /// `--trace-id` override, `--include-kind` / `--exclude-kind` /
    /// `--max-tracks`, semconv attribute renaming, defense-in-depth
    /// re-scan, annotations-as-events policy, formats other than
    /// OTLP/JSON.
    ToOtlp {
        /// Input cassette.
        file: std::path::PathBuf,
        /// Output path. Default: write to stdout. Refuses if equal to
        /// the input path. Parent directories are created as needed.
        #[arg(short = 'o', long)]
        output: Option<std::path::PathBuf>,
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
        Cmd::Stats {
            file,
            format,
            with_cost,
            pricing_file,
        } => cmd_stats(&file, &format, with_cost, pricing_file.as_deref()),
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
            judge_budget,
        } => {
            if let Some(model) = judge {
                cmd_diff_with_judge(&a, &b, all, &format, model, judge_budget)
            } else {
                cmd_diff(&a, &b, all, &format)
            }
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
        cmd @ Cmd::New { .. } => dispatch_new(cmd),
        Cmd::Recap {
            file,
            set,
            clear,
            list,
            auto,
            model,
            out,
        } => cmd_recap(&file, set, clear, list, auto, model, out),
        Cmd::Tag {
            file,
            add,
            remove,
            list,
            dry_run,
            in_place,
            out,
        } => cmd_tag(&file, add, remove, list, dry_run, in_place, out),
        Cmd::Annotate {
            file,
            note,
            editor,
            import,
            step,
            actor,
            by,
            out,
            in_place,
            ts,
            json,
        } => cmd_annotate(
            &file, note, editor, import, step, actor, by, out, in_place, ts, json,
        ),
        Cmd::Export { file, format, out } => cmd_export(&file, &format, out),
        Cmd::Relinernote {
            file,
            model,
            dry_run,
            out,
            template,
        } => cmd_relinernote(&file, model, dry_run, out, &template),
        Cmd::Anon { file, out } => cmd_anon(&file, out),
        Cmd::ToOtlp { file, output } => cmd_to_otlp(&file, output),
    }
}

/// Thin trampoline from the `Cmd::New` match arm into `cmd_new`.
/// Exists only so `main()` stays under the workspace
/// `clippy::too_many_lines` ceiling — by binding the whole variant
/// via `cmd @ Cmd::New { .. }` and destructuring here, `main`'s arm
/// collapses to a single source line.
fn dispatch_new(cmd: Cmd) -> Result<()> {
    let Cmd::New {
        out,
        template,
        task,
        force,
        created_at,
        recorder_agent,
        list_templates,
        describe_template,
        set,
    } = cmd
    else {
        unreachable!("dispatch_new only called with Cmd::New");
    };
    // Introspection flags short-circuit before any path validation.
    // clap's `conflicts_with_all` already rejects combinations with the
    // generation flags at parse time, so reaching here means exactly
    // one of `list_templates` / `describe_template` / generation-path
    // is active.
    if list_templates {
        cmd_new_list();
        return Ok(());
    }
    if let Some(id) = describe_template {
        cmd_new_describe(&id);
        return Ok(());
    }
    let Some(out) = out else {
        eprintln!("tape new: <out> is required (or use --list-templates / --describe-template)");
        std::process::exit(2);
    };
    let template_id = resolve_template_id(template);
    cmd_new(
        &out,
        &template_id,
        task,
        force,
        created_at,
        recorder_agent,
        set,
    )
}

/// Resolve the effective `--template` id (issue #190). The precedence
/// is `--template` CLI flag > `.taperc::new.default_template` >
/// `minimal` terminal fallback. The terminal fallback preserves the
/// pre-#190 implicit default so existing test fixtures and scripts
/// that invoked `tape new <out> --task ...` without `--template`
/// continue to land a `minimal` cassette — back-compat path (b) of
/// #190's acceptance criteria, documented in the PR body.
fn resolve_template_id(cli: Option<String>) -> String {
    if let Some(t) = cli {
        return t;
    }
    // Probe the workspace + user `.taperc` chain. Same locator the
    // redaction engine + `resolve_pricing_source` use so all three
    // stay in lockstep on path-discovery rules. Failed parse exits
    // 2 with the file path named; missing key falls through to the
    // `minimal` terminal default.
    if let Ok(cwd) = std::env::current_dir() {
        let taperc_path = tape_redact::config::TapeRcConfig::locate_workspace(&cwd)
            .or_else(tape_redact::config::TapeRcConfig::locate_user);
        if let Some(p) = taperc_path {
            match std::fs::read_to_string(&p) {
                Ok(yaml) => match tape_redact::config::TapeRcConfig::parse(&yaml) {
                    Ok(cfg) => {
                        if let Some(t) = cfg.new.default_template {
                            return t;
                        }
                    }
                    Err(e) => {
                        eprintln!("tape new: failed to parse {}: {e}", p.display());
                        std::process::exit(2);
                    }
                },
                Err(e) => {
                    eprintln!("tape new: failed to read {}: {e}", p.display());
                    std::process::exit(2);
                }
            }
        }
    }
    "minimal".to_owned()
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

/// Issue #149: `tape diff --judge <MODEL>`. Runs the existing
/// structural diff, loads `JudgeConfig` from `.taperc` (CLI `--judge`
/// value overrides `JudgeConfig::model` for this invocation —
/// matching the CLI > .taperc > built-in default resolution other
/// judge-using commands follow), then iterates substantive entries
/// in order and asks the judge to narrate each. Budget, truncation,
/// and defense-in-depth rules are enforced inside
/// `tape_diff::narrate::narrate_diff`.
fn cmd_diff_with_judge(
    a: &std::path::Path,
    b: &std::path::Path,
    all: bool,
    format: &str,
    model_override: String,
    budget: u32,
) -> Result<()> {
    // 1. Structural pass first — `--judge` is purely additive.
    let mut diff = tape_diff::compute(a, b)?;

    // 2. Load `.taperc::judge:` from cwd (walk-up) or `$HOME` fallback,
    //    same locator semantics `tape-redact` uses. Failing to find a
    //    `judge:` block is an explicit, actionable error per AC7 — the
    //    flag must not silently no-op.
    let cwd = std::env::current_dir()
        .map_err(|e| anyhow::anyhow!("could not resolve current working directory: {e}"))?;
    let Some(mut judge_config) = load_judge_config(&cwd)? else {
        anyhow::bail!(
            "tape diff --judge: no `judge:` block found in .taperc \
             (searched workspace ancestors of {} and $HOME). \
             Add one like:\n\n  judge:\n    model: gpt-4o\n\n\
             then set the API key env var named in `judge.api_key_env` \
             (default OPENAI_API_KEY).",
            cwd.display()
        );
    };

    // 3. CLI override: `--judge <MODEL>` wins over `.taperc::judge.model`.
    judge_config.model = model_override;

    // 4. Construct the client. Surfaces a clean "env var not set"
    //    error here rather than failing mid-narration on the first
    //    HTTP request.
    let max_input_chars = judge_config.max_input_chars;
    let client = tape_judge::JudgeClient::new(judge_config)
        .map_err(|e| anyhow::anyhow!("tape diff --judge: {e}"))?;

    // 5. Narrate. The async work runs on a fresh single-thread
    //    runtime — `tape diff` is otherwise sync, and the narration
    //    path doesn't share any state with a wider tokio context.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut budget = tape_diff::narrate::Budget::new(budget);
    rt.block_on(tape_diff::narrate::narrate_diff(
        &mut diff,
        &client,
        max_input_chars,
        &mut budget,
    ))
    .map_err(|e| anyhow::anyhow!("tape diff --judge: judge call failed: {e}"))?;

    // 6. Render. JSON path serializes the `judge_calls[]` audit rows
    //    via the `Diff` struct (AC6 — visible if the user redirects
    //    to a file; cassettes themselves are untouched).
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

/// Locate the workspace `.taperc` (walk from `cwd` up to `$HOME`), or
/// fall back to `$HOME/.taperc`. Read it as YAML and parse only the
/// `judge:` block. Returns `Ok(None)` when no file exists OR the file
/// has no `judge:` block — same shape as `tape_judge::JudgeConfig::from_taperc_yaml`.
fn load_judge_config(cwd: &std::path::Path) -> Result<Option<tape_judge::JudgeConfig>> {
    let path = locate_taperc(cwd);
    let Some(p) = path else {
        return Ok(None);
    };
    let yaml =
        std::fs::read_to_string(&p).map_err(|e| anyhow::anyhow!("read {}: {e}", p.display()))?;
    tape_judge::JudgeConfig::from_taperc_yaml(&yaml)
        .map_err(|e| anyhow::anyhow!("parse {}: {e}", p.display()))
}

/// Mirror of `tape_redact::config::TapeRcConfig::locate_workspace` +
/// `locate_user`, kept local so we don't bend `tape-redact`'s public
/// surface to leak a `.taperc` locator. If we ever add a third
/// consumer, factor this into a shared `tape-config` crate.
fn locate_taperc(cwd: &std::path::Path) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let mut current = Some(cwd.to_path_buf());
    while let Some(dir) = current {
        let candidate = dir.join(".taperc");
        if candidate.is_file() {
            return Some(candidate);
        }
        if home.as_deref() == Some(dir.as_path()) {
            return None;
        }
        current = dir.parent().map(std::path::Path::to_path_buf);
    }
    let candidate = home?.join(".taperc");
    candidate.is_file().then_some(candidate)
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

    pub const TEST_FIXTURE_VERSION: &str = "1";
    pub const TEST_FIXTURE_LINER: &str = include_str!("../templates/test-fixture/liner-notes.md");
    pub const TEST_FIXTURE_TRACKS: &str = include_str!("../templates/test-fixture/tracks.jsonl");

    pub const BUG_INVESTIGATION_VERSION: &str = "1";
    pub const BUG_INVESTIGATION_LINER: &str =
        include_str!("../templates/bug-investigation/liner-notes.md");
    pub const BUG_INVESTIGATION_TRACKS: &str =
        include_str!("../templates/bug-investigation/tracks.jsonl");
}

/// One built-in template entry — the trio of bytes the substitution
/// pass consumes plus the small set of properties the resolver needs.
/// Static so `resolve_template` can hand out `&'static` references
/// without cloning.
struct TemplateBundle {
    /// Stable id surfaced in `meta.new.template_id` and the
    /// `+new+<id>` recorder-agent suffix.
    id: &'static str,
    /// Version string surfaced in `meta.new.template_version`. Bump
    /// when the template's payload bytes change.
    version: &'static str,
    /// `liner-notes.md` source bytes (before substitution).
    liner: &'static str,
    /// `tracks.jsonl` source bytes (before substitution).
    tracks: &'static str,
    /// Whether `--task` is mandatory. `test-fixture` has it false
    /// because the template hardcodes a literal `"test fixture"` in
    /// the task event's payload; everything else needs the user's
    /// one-line headline.
    task_required: bool,
    /// Whether the rendered tracks/liner include a `{{task}}`
    /// placeholder. Used to decide whether to run the
    /// `String::replace("{{task}}", task)` pass.
    has_task_placeholder: bool,
    /// Sorted list of placeholder names that get filled in by the
    /// substitution pass. Lands in `meta.new.placeholders_filled`.
    /// Kept stable so the deterministic-byte property holds.
    placeholders_filled: &'static [&'static str],
    /// Default `meta.task` when the template hardcodes the task
    /// event's prompt and `--task` is not supplied. `Some(...)` iff
    /// `task_required` is false; `None` otherwise. `test-fixture`
    /// uses the literal `"test fixture"` so the cassette is
    /// internally consistent (meta.task equals tracks[0].prompt).
    default_meta_task: Option<&'static str>,
    /// One-line catalog description surfaced by
    /// `tape new --list-templates` / `--describe-template`. Source of
    /// truth is the `description:` field in the template's
    /// `template.yaml`; mirrored here as a `&'static str` to avoid a
    /// runtime YAML parse for introspection. (Issue #179.)
    description: &'static str,
}

/// Built-in template catalog. Order is documentation only; the
/// `resolve_template` lookup is by id.
const BUILTIN_TEMPLATES: &[TemplateBundle] = &[
    TemplateBundle {
        id: "minimal",
        version: templates::MINIMAL_VERSION,
        liner: templates::MINIMAL_LINER,
        tracks: templates::MINIMAL_TRACKS,
        task_required: true,
        has_task_placeholder: true,
        placeholders_filled: &["task"],
        default_meta_task: None,
        description: "Smallest valid v0 cassette — one task, one eject.",
    },
    TemplateBundle {
        id: "test-fixture",
        version: templates::TEST_FIXTURE_VERSION,
        liner: templates::TEST_FIXTURE_LINER,
        tracks: templates::TEST_FIXTURE_TRACKS,
        task_required: false,
        has_task_placeholder: false,
        placeholders_filled: &[],
        default_meta_task: Some("test fixture"),
        description: "Deterministic 5-track fixture; safe for regen tests.",
    },
    TemplateBundle {
        id: "bug-investigation",
        version: templates::BUG_INVESTIGATION_VERSION,
        liner: templates::BUG_INVESTIGATION_LINER,
        tracks: templates::BUG_INVESTIGATION_TRACKS,
        task_required: true,
        has_task_placeholder: true,
        placeholders_filled: &["task"],
        default_meta_task: None,
        description: "12-track bug-hunt archetype with annotations.",
    },
];

fn resolve_template(id: &str) -> Option<&'static TemplateBundle> {
    BUILTIN_TEMPLATES.iter().find(|t| t.id == id)
}

fn known_template_ids() -> Vec<&'static str> {
    BUILTIN_TEMPLATES.iter().map(|t| t.id).collect()
}

/// Count non-empty lines in a `tracks.jsonl` bundle. The runtime
/// equivalent of `wc -l` over the embedded string; surfaced by
/// `--describe-template` so a user sees the actual track count
/// rather than a hand-maintained property that could drift.
fn count_tracks_lines(jsonl: &str) -> usize {
    jsonl.lines().filter(|l| !l.trim().is_empty()).count()
}

/// `tape new --list-templates` body. One line per built-in template
/// in `BUILTIN_TEMPLATES` order: id, version, required-task marker,
/// description. Column widths pad to the longest id present so the
/// description column lines up. Pure-stdout; the caller exits 0.
fn cmd_new_list() {
    let id_w = BUILTIN_TEMPLATES
        .iter()
        .map(|t| t.id.len())
        .max()
        .unwrap_or(0);
    for t in BUILTIN_TEMPLATES {
        let task_flag = if t.task_required {
            "required-task"
        } else {
            "no-task      "
        };
        println!(
            "{:<id_w$}  v{}  {}  {}",
            t.id,
            t.version,
            task_flag,
            t.description,
            id_w = id_w,
        );
    }
}

/// `tape new --describe-template <id>` body. Prints the full
/// human-readable description block to stdout and returns. Unknown
/// ids exit 2 with stderr listing the valid ids.
fn cmd_new_describe(id: &str) {
    let Some(t) = resolve_template(id) else {
        eprintln!(
            "tape new: --describe-template: unknown template '{id}'; known: {}",
            known_template_ids().join(", ")
        );
        std::process::exit(2);
    };
    println!("template: {}", t.id);
    println!("version:  v{}", t.version);
    let required = if t.task_required { "--task" } else { "(none)" };
    println!("required: {required}");
    println!("optional: --created-at, --recorder-agent, --force");
    println!("tracks:   {}", count_tracks_lines(t.tracks));
    println!();
    println!("description:");
    println!("  {}", t.description);
    println!();
    println!("placeholders:");
    if t.placeholders_filled.is_empty() {
        if let Some(default) = t.default_meta_task {
            println!("  (none) \u{2014} default meta.task is the literal {default:?}");
        } else {
            println!("  (none)");
        }
    } else {
        for ph in t.placeholders_filled {
            let suffix = if t.task_required && *ph == "task" {
                "required"
            } else {
                "optional"
            };
            let blurb = match *ph {
                "task" => {
                    "fills meta.task, tracks[0].payload.prompt, and the liner-notes \"## Task\" section."
                }
                other => other,
            };
            println!("  {ph} ({suffix}) \u{2014} {blurb}");
        }
    }
    println!();
    println!("liner-notes preview:");
    for line in t.liner.lines() {
        println!("  {line}");
    }
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

/// Step-2 of issue #99 (#162). Materializes a new cassette from one
/// of the bundled templates via literal `{{...}}` substitution,
/// builds a fresh `Meta` with a `meta.new` provenance block, and
/// writes the result through `PendingTape::write_to`. `tape verify`
/// runs as a post-write gate so any template-content mistake is
/// caught before the file is left on disk.
/// Resolve `--template <id>` against the built-in catalog and validate
/// `--task` per the template's placeholder spec. Unknown ids exit `2`
/// with `NEW_TEMPLATE_NOT_FOUND`; templates with a required `task`
/// placeholder exit `2` with `NEW_MISSING_PLACEHOLDER` when `--task` is
/// absent. Extracted from `cmd_new` to keep that function under the
/// workspace `clippy::too_many_lines` ceiling and to give the
/// resolution/validation matrix a single test seam.
/// Resolved, override-aware template state passed through `cmd_new`.
/// The `bundle` is the `&'static` catalog entry; `placeholders_filled`
/// is the *effective* set after `--set` + `--task` resolution — it
/// may diverge from the bundle's static slice. Introduced by
/// issue #188 so `cmd_new` / `build_new_meta` consult one shape
/// regardless of whether overrides fired.
struct ResolvedTemplate {
    bundle: &'static TemplateBundle,
    placeholders_filled: Vec<&'static str>,
    task_value: Option<String>,
}

fn resolve_and_validate(
    template_id: &str,
    task: Option<String>,
    overrides: &[(String, String)],
) -> ResolvedTemplate {
    let Some(bundle) = resolve_template(template_id) else {
        eprintln!(
            "tape new: NEW_TEMPLATE_NOT_FOUND — unknown template {template_id:?} \
             (valid: {})",
            known_template_ids().join(", ")
        );
        std::process::exit(2);
    };

    let effective = apply_overrides(bundle, overrides);

    // `task_required` templates rely on the existing
    // `NEW_MISSING_PLACEHOLDER` surface; templates with no required
    // placeholders accept a missing `--task` and only run the
    // char-class validator when a value is supplied. Use the
    // *effective* `task_required` from the override-resolution above.
    let task_value: Option<String> = match (effective.task_required, task.as_deref()) {
        (_, Some(t)) => {
            validate_new_task(t);
            Some(t.to_owned())
        }
        (true, None) => {
            eprintln!(
                "tape new: NEW_MISSING_PLACEHOLDER — --task is required for template {:?}",
                bundle.id
            );
            std::process::exit(2);
        }
        (false, None) => None,
    };

    // `meta.new.placeholders_filled` reflects the post-override
    // effective set. For `minimal --set required-task=false` with no
    // `--task`, that's `[]`; with `--task "x"`, it's still
    // `["task"]`. Mirrors the issue body's determinism note.
    let placeholders_filled = if task_value.is_some() {
        bundle.placeholders_filled.to_vec()
    } else {
        // No --task supplied: drop "task" from the filled set.
        bundle
            .placeholders_filled
            .iter()
            .copied()
            .filter(|ph| *ph != "task")
            .collect::<Vec<_>>()
    };

    ResolvedTemplate {
        bundle,
        placeholders_filled,
        task_value,
    }
}

/// Effective (post-`--set`) template state. Mirrors only the
/// fields that can be overridden — the rest stay on the static
/// `TemplateBundle` and are reached through `bundle` on the
/// `ResolvedTemplate`.
struct EffectiveTemplate {
    task_required: bool,
}

/// clap value-parser for `--set KEY=VALUE`. Splits on the first `=`
/// (so values may contain further `=`). Rejects empty `KEY`,
/// missing `=`, and empty `VALUE`. All three failure modes surface
/// as plain clap usage errors (exit 2), per AC #6 / #7.
#[allow(clippy::unnecessary_wraps, clippy::needless_pass_by_value)]
fn parse_set_kv(s: &str) -> std::result::Result<(String, String), String> {
    let (key, value) = s
        .split_once('=')
        .ok_or_else(|| format!("--set expects KEY=VALUE (got {s:?})"))?;
    if key.is_empty() {
        return Err(format!("--set: KEY must not be empty (got {s:?})"));
    }
    if value.is_empty() {
        return Err(format!("--set: VALUE must not be empty (got {s:?})"));
    }
    Ok((key.to_owned(), value.to_owned()))
}

/// Apply `--set` overrides to the resolved template. Today only one
/// override key is recognized (`required-task=true|false`, and only
/// on templates whose `task_required` was `true` to begin with — we
/// still recognize it on those for symmetry / forward-compat).
/// Unknown keys exit 2 with `NEW_UNKNOWN_OVERRIDE_KEY`. Duplicate
/// keys: last-wins with a stderr warning.
fn apply_overrides(bundle: &TemplateBundle, overrides: &[(String, String)]) -> EffectiveTemplate {
    let known_keys = known_override_keys(bundle);
    // Detect duplicates for the last-wins warning, AC #8.
    let mut seen: std::collections::HashSet<&str> =
        std::collections::HashSet::with_capacity(overrides.len());
    let mut effective = EffectiveTemplate {
        task_required: bundle.task_required,
    };
    for (key, value) in overrides {
        if !known_keys.contains(&key.as_str()) {
            let known_str = if known_keys.is_empty() {
                "<none>".to_owned()
            } else {
                known_keys.join(", ")
            };
            eprintln!(
                "tape new: NEW_UNKNOWN_OVERRIDE_KEY — unknown override key {key:?} \
                 for template {:?} (known: {known_str})",
                bundle.id,
            );
            std::process::exit(2);
        }
        if !seen.insert(key.as_str()) {
            eprintln!("tape new: --set {key} specified twice; using last value");
        }
        match key.as_str() {
            "required-task" => match value.as_str() {
                "true" => effective.task_required = true,
                "false" => effective.task_required = false,
                other => {
                    eprintln!(
                        "tape new: --set required-task: expected 'true' or 'false', got {other:?}"
                    );
                    std::process::exit(2);
                }
            },
            // Unknown keys are rejected above; this match is exhaustive
            // over the known-keys list at the time of writing.
            _ => unreachable!("known_override_keys / apply_overrides disagree on {key:?}"),
        }
    }
    effective
}

/// Per-template known override keys. Empty `&[]` means `--set` has
/// no recognized keys for the template (e.g. `test-fixture`); any
/// `--set k=v` against it exits 2 with `NEW_UNKNOWN_OVERRIDE_KEY` +
/// `(known: <none>)` per AC #4.
fn known_override_keys(bundle: &TemplateBundle) -> &'static [&'static str] {
    match bundle.id {
        "minimal" | "bug-investigation" => &["required-task"],
        _ => &[],
    }
}

/// Substitution marker used when `--task` is omitted (only reachable
/// when effective `task_required` is false — via `--set
/// required-task=false`). SPEC §5.5.1 rejects an empty `task` event
/// prompt as `INVALID_PAYLOAD`, so the literal-empty substitution
/// suggested by the original #188 acceptance text would never pass
/// the post-write verify gate at step 7. The marker keeps the
/// rendered cassette valid and makes the "I didn't supply a task"
/// intent visible in the prompt. Mirrored verbatim by the
/// `tests/tape_new_set_overrides.rs::NO_TASK_MARKER` fixture
/// constant — keep the two in sync. (Cannot be shared via `pub` in
/// a library: `tape-cli` is a bin-only crate; the integration test
/// directory cannot import items from `main.rs`.)
const NO_TASK_MARKER: &str = "(no task supplied)";

fn cmd_new(
    out: &std::path::Path,
    template_id: &str,
    task: Option<String>,
    force: bool,
    created_at_override: Option<String>,
    recorder_agent_override: Option<String>,
    overrides: Vec<(String, String)>,
) -> Result<()> {
    // 1. Resolve the template + apply --set overrides + validate
    //    `--task` against the *effective* placeholder spec. Errors
    //    exit `2` with the appropriate `NEW_*` diagnostic code.
    let resolved = resolve_and_validate(template_id, task, &overrides);
    let bundle = resolved.bundle;
    let task_value = resolved.task_value;

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
        .unwrap_or_else(|| format!("tape-cli/{}+new+{}", env!("CARGO_PKG_VERSION"), bundle.id));

    // 4. Substitute template placeholders. Literal `String::replace`,
    //    no expression language — the rule is "grep '{{' templates/`
    //    should always show every active placeholder."
    //
    //    `NO_TASK_MARKER` (module scope, declared just above
    //    `cmd_new`) is used when `--task` is omitted; the rationale
    //    lives on its declaration site.
    let task_for_sub: &str = task_value.as_deref().unwrap_or(NO_TASK_MARKER);
    let liner_md = if bundle.has_task_placeholder {
        bundle.liner.replace("{{task}}", task_for_sub)
    } else {
        bundle.liner.to_owned()
    };
    let mut tracks_jsonl = bundle
        .tracks
        .replace("{{created_at}}", &created_at)
        .replace("{{ejected_at}}", &ejected_at);
    if bundle.has_task_placeholder {
        tracks_jsonl = tracks_jsonl.replace("{{task}}", task_for_sub);
    }

    // 5. Build the Meta. Extracted to `build_new_meta` so `cmd_new`
    //    stays under the workspace `clippy::too_many_lines` ceiling.
    //    Uses `resolved.placeholders_filled` (effective post-override)
    //    rather than the bundle's static slice — see #188 AC re:
    //    `meta.new.placeholders_filled` mirroring the post-`--set`
    //    state.
    let meta = build_new_meta(
        bundle,
        task_value.as_deref(),
        task_for_sub,
        &created_at,
        &ejected_at,
        &recorder_agent,
        &tracks_jsonl,
        &resolved.placeholders_filled,
    );
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

    println!("ok: wrote {} (template={})", out.display(), bundle.id);
    Ok(())
}

/// Synthesize the `Meta` block for `tape new`. The id is a
/// deterministic `UUIDv7` derived from
/// `(created_at, recorder_agent, task-or-empty)` so two runs with
/// pinned overrides produce byte-identical track / meta content (the
/// deterministic-output property in Principal's test plan). For
/// `test-fixture` the empty task string keeps the derivation total.
///
/// `meta.task` uses the user-supplied task when present; otherwise
/// the template's `default_meta_task` (set when the template
/// hardcodes the task event's prompt). Templates that require
/// `--task` would have failed `resolve_and_validate`'s gate, so the
/// `unwrap_or_default` fallback is reached only on the
/// `(task_required: false, default_meta_task: None)` shape — none of
/// the built-ins are wired that way today.
///
/// `meta.outcome` MUST match the eject event's `payload.outcome`
/// (SPEC §10.5 `OUTCOME_MISMATCH`) — we peek at the rendered tracks
/// before the verify gate so a template can declare a non-default
/// outcome (e.g. `test-fixture` ships `success`). Falls back to
/// `Unknown` if the eject can't be located, which keeps the
/// `minimal` template's existing shape working.
#[allow(clippy::too_many_arguments)]
fn build_new_meta(
    bundle: &TemplateBundle,
    task_value: Option<&str>,
    task_for_sub: &str,
    created_at: &str,
    ejected_at: &str,
    recorder_agent: &str,
    tracks_jsonl: &str,
    placeholders_filled: &[&'static str],
) -> tape_format::meta::Meta {
    let id = derive_uuid_v7(created_at, recorder_agent, task_for_sub);
    let meta_task = task_value
        .map(str::to_owned)
        .or_else(|| bundle.default_meta_task.map(str::to_owned))
        .unwrap_or_default();
    let outcome =
        outcome_from_rendered_tracks(tracks_jsonl).unwrap_or(tape_format::meta::Outcome::Unknown);
    tape_format::meta::Meta {
        tape_version: tape_format::TAPE_VERSION.into(),
        id,
        created_at: created_at.to_owned(),
        ejected_at: ejected_at.to_owned(),
        task: meta_task,
        recorder: tape_format::meta::Recorder {
            agent: recorder_agent.to_owned(),
            user: None,
        },
        outcome,
        models: vec![],
        tools: vec![],
        tool_budget: None,
        redaction_summary: None,
        label: None,
        recap: None,
        recaps: vec![],
        tags: vec![],
        relinernotes: vec![],
        new_block: Some(tape_format::meta::NewBlock {
            template_id: bundle.id.into(),
            template_version: bundle.version.into(),
            generated_at: created_at.to_owned(),
            placeholders_filled: placeholders_filled
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        }),
    }
}

/// Parse the eject event's `payload.outcome` from a rendered
/// `tracks.jsonl` body so the synthesized `meta.outcome` matches what
/// the template's eject declares (SPEC §10.5 `OUTCOME_MISMATCH`).
/// Best-effort — returns `None` if the eject can't be found or the
/// outcome value is missing / unknown. Cheap because the eject is
/// always the final line (`SPEC §5.4`); we scan the tail.
fn outcome_from_rendered_tracks(jsonl: &str) -> Option<tape_format::meta::Outcome> {
    let last_line = jsonl.lines().last()?.trim();
    if last_line.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(last_line).ok()?;
    if value.get("kind").and_then(|v| v.as_str()) != Some("eject") {
        return None;
    }
    let outcome_str = value
        .get("payload")
        .and_then(|p| p.get("outcome"))
        .and_then(|v| v.as_str())?;
    match outcome_str {
        "success" => Some(tape_format::meta::Outcome::Success),
        "failure" => Some(tape_format::meta::Outcome::Failure),
        "abandoned" => Some(tape_format::meta::Outcome::Abandoned),
        "unknown" => Some(tape_format::meta::Outcome::Unknown),
        _ => None,
    }
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

/// Lift the per-mode decision out of `cmd_recap` so the entry-point
/// function stays under the pedantic 100-line threshold. Returns the
/// `RecapKind` to record on the audit row plus an optional
/// `JudgeCallRecord` (populated only on the `--auto` path). Mutates
/// `meta.recap` in place so the surrounding write path picks up the
/// new value without a second match.
fn resolve_recap_edit(
    meta: &mut tape_format::meta::Meta,
    raw: &tape_format::reader::RawTape,
    out_path: &std::path::Path,
    set: Option<&str>,
    auto: bool,
    cli_model: Option<&str>,
) -> (
    tape_format::meta::RecapKind,
    Option<tape_judge::JudgeCallRecord>,
) {
    if auto {
        let (new_recap, record) = run_recap_auto(meta, raw, out_path, cli_model);
        meta.recap = Some(new_recap);
        return (tape_format::meta::RecapKind::Auto, Some(record));
    }
    if let Some(text) = set {
        if text.is_empty() {
            eprintln!("tape recap: --set text must be non-empty");
            std::process::exit(2);
        }
        if let Err(msg) = tape_format::meta::validate_recap_text(text) {
            eprintln!("tape recap: --set rejected: {msg}");
            std::process::exit(2);
        }
        meta.recap = Some(text.to_owned());
        return (tape_format::meta::RecapKind::Set, None);
    }
    // --clear
    meta.recap = None;
    (tape_format::meta::RecapKind::Clear, None)
}

#[allow(clippy::too_many_arguments)]
fn cmd_recap(
    file: &std::path::Path,
    set: Option<String>,
    clear: bool,
    list: bool,
    auto: bool,
    model: Option<String>,
    out: Option<std::path::PathBuf>,
) -> Result<()> {
    // 1. Verify exactly one mode flag is set. clap's `conflicts_with_all`
    //    handles the pairwise-exclusion side; this is the
    //    "at-least-one" half (clap can't model that cleanly when each
    //    flag also has a default like `bool: false` / `Option: None`).
    let mode_count = [set.is_some(), clear, list, auto]
        .iter()
        .filter(|b| **b)
        .count();
    if mode_count == 0 {
        eprintln!("tape recap: one of --set <text>, --clear, --list, --auto is required");
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

    // 5. Decide the new recap text and audit-row kind. `--auto` is async
    //    (the judge call) so it has its own driver function; `--set` /
    //    `--clear` stay in this synchronous body. The helper keeps
    //    `cmd_recap` under the 100-line pedantic threshold by lifting
    //    the per-mode decision out.
    let (kind, judge_call) = resolve_recap_edit(
        &mut meta,
        &raw,
        &out_path,
        set.as_deref(),
        auto,
        model.as_deref(),
    );

    let entry = tape_format::meta::RecapEntry {
        applied_at: chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
        kind,
        prior_recap,
        new_recap: meta.recap.clone(),
        judge_call,
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

    let action_label = if auto {
        "auto-set"
    } else if set.is_some() {
        "set"
    } else {
        "cleared"
    };
    println!("ok: {action_label} recap on {}", out_path.display());
    Ok(())
}

/// Step-1 of issue #93. Hand-managed `meta.tags[]` via `--add` /
/// `--remove` / `--list`. Same zip-rewrite strategy `cmd_recap` uses —
/// no eject pipeline, no audit trail (Step 2), no caps / closed-vocab
/// enforcement (Steps 2 & 3), no judge-model auto mode (Step 4). Set
/// semantics are enforced at the CLI: re-adding an existing tag or
/// removing an absent one is a no-op, and an invocation that produces
/// no net change skips the write entirely (`TAG_NO_CHANGE` on stderr).
///
/// `--in-place` reuses `PendingTape::write_to`'s built-in temp-file +
/// atomic rename, so the input is never observably half-written.
#[allow(clippy::too_many_arguments)]
fn cmd_tag(
    file: &std::path::Path,
    add: Vec<String>,
    remove: Vec<String>,
    list: bool,
    dry_run: bool,
    in_place: bool,
    out: Option<std::path::PathBuf>,
) -> Result<()> {
    // 1. `--list` is read-only — no validation of add/remove flags
    //    (clap's conflicts_with_all keeps them out), no output-path
    //    resolution. Empty list prints nothing and exits 0 (vs recap's
    //    exit-4-on-absent, which fits recap's semantics but not a
    //    plural-by-default field).
    if list {
        let raw = open_input(file, "tape tag");
        let meta = parse_meta(&raw, "tape tag");
        for tag in &meta.tags {
            println!("{tag}");
        }
        return Ok(());
    }

    // 2. At-least-one-mode check. Without --add or --remove there is
    //    nothing to do, and silently exiting 0 would hide a typo.
    if add.is_empty() && remove.is_empty() {
        eprintln!("tape tag: one of --add <tag>, --remove <tag>, --list is required");
        std::process::exit(2);
    }

    // 3. Validate each --add value. Empty strings make no sense as
    //    facet labels and would round-trip indistinguishable from
    //    "no tag" in any consumer; reject at the boundary. Length /
    //    character-class caps are Step 2.
    for t in &add {
        if t.is_empty() {
            eprintln!("tape tag: --add tag must be non-empty");
            std::process::exit(2);
        }
    }
    for t in &remove {
        if t.is_empty() {
            eprintln!("tape tag: --remove tag must be non-empty");
            std::process::exit(2);
        }
    }

    // 4. Resolve the output path. `--in-place` short-circuits to the
    //    input path (`PendingTape::write_to` does the temp + rename).
    let out_path = if in_place {
        file.to_path_buf()
    } else if let Some(p) = out {
        p
    } else {
        let stem = file
            .file_stem()
            .map_or_else(|| "tape".to_owned(), |s| s.to_string_lossy().into_owned());
        let parent = file.parent().unwrap_or_else(|| std::path::Path::new("."));
        parent.join(format!("{stem}.tagged.tape"))
    };
    if !in_place && same_path(file, &out_path) {
        eprintln!("tape tag: --out must differ from <file> (use --in-place for atomic rewrite)");
        std::process::exit(2);
    }

    // 5. Load the input and compute the new tag set + diff. Existing
    //    order is preserved for unchanged entries; new entries append
    //    in argv order. Set semantics: duplicates collapse silently.
    let raw = open_input(file, "tape tag");
    let mut meta = parse_meta(&raw, "tape tag");
    let prior: Vec<String> = meta.tags.clone();
    let remove_set: std::collections::HashSet<&str> = remove.iter().map(String::as_str).collect();

    let mut next: Vec<String> = prior
        .iter()
        .filter(|t| !remove_set.contains(t.as_str()))
        .cloned()
        .collect();
    let next_set_during: std::collections::HashSet<String> = next.iter().cloned().collect();
    let mut added_diff: Vec<String> = Vec::new();
    let mut seen_new = next_set_during;
    for t in &add {
        if seen_new.contains(t) {
            continue;
        }
        next.push(t.clone());
        added_diff.push(t.clone());
        seen_new.insert(t.clone());
    }
    let removed_diff: Vec<String> = prior
        .iter()
        .filter(|t| remove_set.contains(t.as_str()))
        .cloned()
        .collect();

    // 6. `--dry-run` prints the diff and exits 4 without touching disk.
    //    Treat empty diff the same way --dry-run on a no-op should: print
    //    the (empty) diff plus a note, exit 4.
    if dry_run {
        print_tag_diff(&prior, &next, &added_diff, &removed_diff);
        std::process::exit(4);
    }

    // 7. No-op suppression. If the diff is empty (every --add was a
    //    duplicate, every --remove was absent), exit 0 without writing.
    if added_diff.is_empty() && removed_diff.is_empty() {
        eprintln!("tape tag: TAG_NO_CHANGE — no tags added or removed");
        // Print the unchanged list for confirmation (mirrors --list).
        for t in &prior {
            println!("{t}");
        }
        return Ok(());
    }

    // 8. Apply and write. Re-uses `cmd_recap`'s zip-rewrite path
    //    (everything but meta.yaml passes through byte-identical).
    meta.tags = next;
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

    // 9. Post-write verify. `LEAKED_SECRET_IN_META` (SPEC §10.5) is the
    //    backstop that catches a secret-shaped tag value in Step 1, the
    //    way `cmd_recap` relies on it for `--set` text. Step-2 work will
    //    add a pre-write scan + the dedicated `TAG_LEAK` code, at which
    //    point this gate becomes belt-and-suspenders. On regression:
    //    remove the corrupt output and exit 3.
    let written = tape_format::reader::RawTape::open(&out_path)?;
    let report = tape_format::verify::verify(&written);
    if !report.is_valid() {
        let _ = std::fs::remove_file(&out_path);
        // Reconstruct the input path if --in-place obliterated it (the
        // post-rename file is gone; the caller can re-create from the
        // original copy they're presumably keeping under VCS).
        if in_place {
            // The atomic rename already replaced the input. Removing
            // the failed write leaves no cassette on disk; warn the
            // caller loudly so they know to restore.
            eprintln!(
                "tape tag: --in-place output failed tape verify; the input was \
                 already replaced and has been removed. Restore from backup."
            );
        }
        let codes: Vec<&'static str> = report.errors().map(|d| d.code.as_str()).collect();
        eprintln!(
            "tape tag: output failed tape verify ({}); removed {}",
            codes.join(","),
            out_path.display()
        );
        std::process::exit(3);
    }

    // 10. Success summary. Single-line stdout for scripting; verbose
    //     details land on stderr so `--list`-style stdout piping stays
    //     clean.
    eprintln!(
        "tape tag: +{} -{} on {}",
        added_diff.len(),
        removed_diff.len(),
        out_path.display()
    );
    println!("ok: tagged {}", out_path.display());
    Ok(())
}

/// Pretty-print a `--dry-run` diff. Distinct from the success path so
/// the format can evolve without affecting the write path. Read by tests
/// against stdout, so keep the order stable.
fn print_tag_diff(prior: &[String], next: &[String], added: &[String], removed: &[String]) {
    println!("prior: [{}]", prior.join(", "));
    println!("next:  [{}]", next.join(", "));
    if !added.is_empty() {
        println!("added: {}", added.join(", "));
    }
    if !removed.is_empty() {
        println!("removed: {}", removed.join(", "));
    }
    if added.is_empty() && removed.is_empty() {
        println!("(no change)");
    }
}

/// `--auto` driver. Builds the prompt, runs the judge call inside a
/// fresh tokio runtime, validates the response with the same
/// `validate_recap_text` rules `--set` uses, and propagates structured
/// exit codes for the two failure modes Principal called out in #151:
/// `RECAP_AUTO_INVALID_OUTPUT` (exit 2, validator rejected the model's
/// text) and `RECAP_AUTO_LEAK` (exit 6, defense-in-depth scanner inside
/// the judge client flagged the output). The original cassette is
/// preserved untouched on both error paths — the post-write `tape verify`
/// at step 7 is the next gate, so we never reach it if we exit here.
fn run_recap_auto(
    meta: &tape_format::meta::Meta,
    raw: &tape_format::reader::RawTape,
    out_path: &std::path::Path,
    cli_model: Option<&str>,
) -> (String, tape_judge::JudgeCallRecord) {
    // a. Load `.taperc::judge:` plus the `[recap]` block. Workspace-local
    //    takes precedence over `$HOME/.taperc`, matching the existing
    //    tape-judge consumer pattern.
    let (mut config, recap_cfg) = match load_judge_and_recap_config() {
        Ok(pair) => pair,
        Err(msg) => {
            eprintln!("tape recap: RECAP_AUTO_CONFIG — {msg}");
            std::process::exit(2);
        }
    };

    // a.1. Resolve the effective model: CLI `--model` > `.taperc::recap.default_model`
    //      > `judge.model` (already in `config.model`). Empty strings on
    //      either tier fall through — they're typo-prone and the next
    //      tier almost always has a real value.
    let effective_override = cli_model
        .map(str::to_owned)
        .filter(|s| !s.is_empty())
        .or_else(|| recap_cfg.default_model.clone().filter(|s| !s.is_empty()));
    if let Some(m) = effective_override {
        config.model = m;
    }

    // b. Build the prompt up front so a 0-byte tracks.jsonl can't
    //    silently feed the model an empty context.
    let prompt = render_recap_prompt(meta, raw);

    // c. Construct the client and run one judge call. `JudgeOpts::default()`
    //    inherits `max_tokens` from config; #151 forbids re-sampling on
    //    validator failure (let the client's own retry handle transients).
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("tape recap: RECAP_AUTO_CONFIG — build tokio runtime: {e}");
            std::process::exit(2);
        }
    };
    let result = rt.block_on(async move {
        let client = tape_judge::JudgeClient::new(config)?;
        client
            .complete(&prompt, tape_judge::JudgeOpts::default())
            .await
    });

    let out = match result {
        Ok(o) => o,
        Err(tape_judge::JudgeError::Rejected(hit)) => {
            // Defense-in-depth scanner inside the client flagged the
            // output before it crossed back into the caller. AC #5.
            // No cassette is written; the original at `file` is
            // already untouched and `out_path` was never created.
            let _ = out_path; // explicitly: nothing to clean up.
            eprintln!(
                "tape recap: RECAP_AUTO_LEAK — judge output rejected by defense-in-depth: {}",
                hit.rule_id
            );
            std::process::exit(6);
        }
        Err(e) => {
            eprintln!("tape recap: RECAP_AUTO_CONFIG — judge call failed: {e}");
            std::process::exit(2);
        }
    };

    // d. Validate the trimmed text against the same invariants
    //    `--set` enforces. The validator is the source of truth for
    //    "what fits in `meta.recap`"; a model that ignores the
    //    instructions still gets rejected here.
    let trimmed = out.text.trim().to_owned();
    if trimmed.is_empty() {
        eprintln!("tape recap: RECAP_AUTO_INVALID_OUTPUT — model returned empty text");
        std::process::exit(2);
    }
    if let Err(msg) = tape_format::meta::validate_recap_text(&trimmed) {
        eprintln!("tape recap: RECAP_AUTO_INVALID_OUTPUT — {msg}");
        std::process::exit(2);
    }

    (trimmed, out.record)
}

/// Locate `.taperc` (workspace first, user-level fallback), parse the
/// `judge:` block, and return the resolved [`tape_judge::JudgeConfig`]
/// alongside the parsed `[recap]` config block. The latter supplies
/// the `default_model` fallback layered between CLI `--model` and
/// `judge.model` per issue #198. A failure to read or parse the
/// `.taperc` itself surfaces an exit-2 error with the file path
/// named; an absent `[recap]` block returns a default-empty
/// `RecapConfig` rather than failing.
fn load_judge_and_recap_config(
) -> std::result::Result<(tape_judge::JudgeConfig, tape_redact::config::RecapConfig), String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    let path = tape_redact::config::TapeRcConfig::locate_workspace(&cwd)
        .or_else(tape_redact::config::TapeRcConfig::locate_user);
    let Some(p) = path else {
        return Err(".taperc not found (looked in workspace and $HOME); \
             needed for --auto to know the judge model + endpoint"
            .into());
    };
    let yaml = std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
    // Parse the `judge:` block via tape-judge's loader (existing path)
    // and the `[recap]` block via the redact-crate parser (post-#198).
    // Two parses against the same bytes; the cost is negligible (the
    // file is small enough that the second parse is a microsecond) and
    // the alternative would be reshaping the tape-judge loader to
    // surface `RecapConfig`, which crosses a crate boundary for one
    // field. Two parses keeps the change local.
    let judge_cfg = tape_judge::JudgeConfig::from_taperc_yaml(&yaml)
        .map_err(|e| format!("parse {}: {e}", p.display()))?
        .ok_or_else(|| {
            format!(
                "{}: no `judge:` block; add one (model + api_key_env) and re-run",
                p.display()
            )
        })?;
    let recap_cfg = tape_redact::config::TapeRcConfig::parse(&yaml)
        .map(|c| c.recap)
        .map_err(|e| format!("parse {}: {e}", p.display()))?;
    Ok((judge_cfg, recap_cfg))
}

/// Compose the prompt the judge model receives for `--auto`. Hardcoded
/// per Principal's pitfall callout — bundled templates are a follow-on
/// once two `--auto` consumers have shipped. The order is deliberate:
/// the instructions go first so an oversized tracks summary can't push
/// them out of the model's effective context.
fn render_recap_prompt(
    meta: &tape_format::meta::Meta,
    raw: &tape_format::reader::RawTape,
) -> String {
    use std::fmt::Write as _;

    let mut s = String::with_capacity(4096);
    s.push_str(
        "You are summarising one recording of an agent investigating a task. \
         Produce a 1–2 sentence recap suitable for pasting into a Slack message, \
         a Linear ticket, or a PR description. Hard constraints: \
         ≤280 characters, single line (no newline characters), plain UTF-8, no markdown. \
         Be concrete — name the user-visible outcome, not a meta description of the recording. \
         If the cassette ended with `outcome: failure` or `abandoned`, say so. \
         Output ONLY the recap text. Do not add quotes, prefixes, or trailing notes.\n\n",
    );
    // Writes into a `String` are infallible; the `let _ =` drops the
    // never-fired Err arm. Avoids the `format_push_string` allocation
    // per line that `push_str(&format!(...))` would incur.
    let _ = writeln!(s, "Task: {}", meta.task);
    let outcome = match meta.outcome {
        tape_format::meta::Outcome::Success => "success",
        tape_format::meta::Outcome::Failure => "failure",
        tape_format::meta::Outcome::Abandoned => "abandoned",
        tape_format::meta::Outcome::Unknown => "unknown",
    };
    let _ = writeln!(s, "Outcome: {outcome}");
    if let Some(label) = meta.label.as_deref() {
        let _ = writeln!(s, "Label: {label}");
    }
    s.push_str("\nTracks (one line per step):\n");
    s.push_str(&render_track_summary(raw, RECAP_TRACK_BUDGET));
    if let Some(liner) = raw.liner_md.as_deref() {
        if !liner.trim().is_empty() {
            s.push_str("\nLiner notes:\n");
            s.push_str(liner.trim());
            s.push('\n');
        }
    }
    s
}

/// 4 KiB cap on the tracks summary section of the recap prompt, per
/// Principal scoping in #151. Tracks above the cap are head+tail-
/// truncated so both ends of long investigations remain visible.
const RECAP_TRACK_BUDGET: usize = 4096;

/// Render one line per track in JSONL ordering. Each line carries the
/// step number, kind, and a compact payload hint extracted from the
/// JSON payload (`prompt` / `cmd` / `path` / `outcome` — whichever the
/// kind owns). Returns a `String` capped at [`RECAP_TRACK_BUDGET`] bytes;
/// if the rendered text exceeds the cap, it's head+tail-truncated with a
/// `… N tracks elided …` marker.
fn render_track_summary(raw: &tape_format::reader::RawTape, budget: usize) -> String {
    use std::fmt::Write as _;

    let Some(jsonl) = raw.tracks_jsonl.as_deref() else {
        return "(no tracks)\n".to_owned();
    };
    let Ok(tracks) = tape_format::tracks::parse_jsonl(jsonl) else {
        return "(tracks did not parse)\n".to_owned();
    };
    let lines: Vec<String> = tracks.iter().map(render_track_line).collect();
    // Fold the lines into one String with trailing newlines. Avoids the
    // per-line `format!` allocation that `.map(|l| format!("{l}\n"))`
    // would do before joining.
    let mut full = String::with_capacity(lines.iter().map(|l| l.len() + 1).sum());
    for l in &lines {
        let _ = writeln!(full, "{l}");
    }
    if full.len() <= budget {
        return full;
    }
    // Head+tail truncation. Reserve room for the elision marker; aim
    // for ~45% of the budget per side so both ends fit comfortably.
    let elide_marker = format!(
        "… {} tracks elided (budget {budget} bytes) …\n",
        lines.len()
    );
    let side = budget.saturating_sub(elide_marker.len()) / 2;
    let head = take_chars_bytes(&full, side);
    let tail = take_chars_bytes_from_end(&full, side);
    let mut out = String::with_capacity(budget);
    out.push_str(&head);
    out.push_str(&elide_marker);
    out.push_str(&tail);
    out
}

fn render_track_line(t: &tape_format::tracks::Track) -> String {
    let kind = match t.kind {
        tape_format::tracks::Kind::Task => "task",
        tape_format::tracks::Kind::ModelCall => "model_call",
        tape_format::tracks::Kind::McpCall => "mcp_call",
        tape_format::tracks::Kind::Shell => "shell",
        tape_format::tracks::Kind::FileRead => "file_read",
        tape_format::tracks::Kind::FileWrite => "file_write",
        tape_format::tracks::Kind::Annotation => "annotation",
        tape_format::tracks::Kind::Eject => "eject",
    };
    // `Kind` is a 1-byte `Copy` enum; pass by value (workspace convention)
    // to silence `clippy::trivially_copy_pass_by_ref`.
    let hint = recap_payload_hint(t.kind, &t.payload);
    if hint.is_empty() {
        format!("  {:>3}. {kind}", t.step)
    } else {
        format!("  {:>3}. {kind}: {hint}", t.step)
    }
}

fn recap_payload_hint(kind: tape_format::tracks::Kind, payload: &serde_json::Value) -> String {
    let key = match kind {
        tape_format::tracks::Kind::Task => "prompt",
        tape_format::tracks::Kind::ModelCall => "model",
        tape_format::tracks::Kind::McpCall => "tool",
        tape_format::tracks::Kind::Shell => "cmd",
        tape_format::tracks::Kind::FileRead | tape_format::tracks::Kind::FileWrite => "path",
        tape_format::tracks::Kind::Annotation => "note",
        tape_format::tracks::Kind::Eject => "outcome",
    };
    let raw = payload
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .replace(['\n', '\r'], " ");
    let truncated: String = raw.chars().take(120).collect();
    if raw.chars().count() > 120 {
        format!("{truncated}…")
    } else {
        truncated
    }
}

/// Take a UTF-8-safe prefix of `s` containing at most `byte_cap` bytes.
/// Walks character boundaries so we never split a multi-byte codepoint.
fn take_chars_bytes(s: &str, byte_cap: usize) -> String {
    if s.len() <= byte_cap {
        return s.to_owned();
    }
    let mut end = byte_cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}

/// Like [`take_chars_bytes`] but from the end.
fn take_chars_bytes_from_end(s: &str, byte_cap: usize) -> String {
    if s.len() <= byte_cap {
        return s.to_owned();
    }
    let mut start = s.len().saturating_sub(byte_cap);
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].to_owned()
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
/// Resolve the annotation body. clap's `required_unless_present` +
/// `conflicts_with` guarantees exactly one of `--note` / `--editor` is
/// set, so the caller passes the parsed `note` plus the `editor` flag
/// and receives:
///
/// - `Some(body)` — happy path, body ready to scan + persist.
/// - `None` — `--editor` was set and the user cancelled with an empty
///   body. The caller exits 0 with no output cassette; the cancel
///   message has already been printed.
///
/// Any other failure (editor spawn, non-UTF-8, oversize, non-zero
/// editor exit) prints a diagnostic and calls `std::process::exit(2)`
/// *after* `compose_note_via_editor`'s `tempfile::NamedTempFile` has
/// dropped — keeping issue #158 AC #6 / #8 / #9 (no temp-file leak)
/// satisfied.
fn resolve_note_body(
    file: &std::path::Path,
    note: Option<String>,
    editor: bool,
    import: Option<std::path::PathBuf>,
    by: &str,
    editor_override: Option<&str>,
) -> Option<String> {
    if editor {
        match compose_note_via_editor(file, by, editor_override) {
            Ok(Some(body)) => Some(body),
            Ok(None) => {
                eprintln!("tape annotate: nothing to annotate (empty body)");
                None
            }
            Err(EditorError::SpawnFailed(msg) | EditorError::EditorExitNonZero(msg)) => {
                eprintln!("tape annotate: {msg}");
                std::process::exit(2);
            }
            Err(EditorError::NonUtf8Body) => {
                eprintln!("tape annotate: editor produced non-UTF-8 body");
                std::process::exit(2);
            }
            Err(EditorError::OversizeBody(n)) => {
                eprintln!(
                    "tape annotate: body exceeds 16 KiB limit (got {n} bytes after comment-strip)"
                );
                std::process::exit(2);
            }
        }
    } else if let Some(path) = import {
        match compose_note_via_import(&path) {
            Ok(Some(body)) => Some(body),
            Ok(None) => {
                eprintln!("tape annotate: nothing to annotate (empty body)");
                None
            }
            Err(ImportError::ReadFailed(path, e)) => {
                eprintln!(
                    "tape annotate: failed to read --import file {}: {e}",
                    path.display()
                );
                std::process::exit(2);
            }
            Err(ImportError::NonUtf8(path)) => {
                eprintln!(
                    "tape annotate: --import file {} is not valid UTF-8",
                    path.display()
                );
                std::process::exit(2);
            }
            Err(ImportError::OversizeBody(path, n)) => {
                eprintln!(
                    "tape annotate: body exceeds 16 KiB limit (--import {} is {n} bytes)",
                    path.display()
                );
                std::process::exit(2);
            }
        }
    } else {
        Some(note.expect(
            "clap required_unless_present_any guarantees note is Some when editor/import unset",
        ))
    }
}

/// Build a sibling path next to `file` with the supplied filename
/// suffix (joined after the file stem with a `.`). Falls back to a
/// stem of `tape` and a parent of `.` when the input path has no
/// stem / no parent (e.g. a bare filename). Used by both the
/// `--in-place` temp path and the default `<stem>.annotated.tape`
/// output path; centralising the logic dodges the
/// `binding's name too similar` and `map(<f>).unwrap_or_else(<g>)`
/// clippy lints that the inline duplicate triggered.
fn sibling_path(file: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let dir = file.parent().unwrap_or_else(|| std::path::Path::new("."));
    let base = file.file_stem().map_or_else(
        || std::borrow::Cow::Borrowed("tape"),
        |s| s.to_string_lossy(),
    );
    dir.join(format!("{base}.{suffix}"))
}

/// Load the `.taperc::annotate` block, if any. Returns `Some((path,
/// cfg))` when a `.taperc` was found AND its parse succeeded; `None`
/// when no `.taperc` is in scope. A parse failure surfaces an
/// exit-2 diagnostic with the config path named and the binary
/// terminates (no original-cassette mutation happens before this
/// returns). The returned path is the `.taperc`'s location so the
/// `default_by`-validation diagnostic can name it. Issue #192.
fn load_annotate_config() -> Option<(std::path::PathBuf, tape_redact::config::AnnotateConfig)> {
    let cwd = std::env::current_dir().ok()?;
    let taperc = tape_redact::config::TapeRcConfig::locate_workspace(&cwd)
        .or_else(tape_redact::config::TapeRcConfig::locate_user)?;
    match std::fs::read_to_string(&taperc) {
        Ok(yaml) => match tape_redact::config::TapeRcConfig::parse(&yaml) {
            Ok(cfg) => Some((taperc, cfg.annotate)),
            Err(e) => {
                eprintln!("tape annotate: failed to parse {}: {e}", taperc.display());
                std::process::exit(2);
            }
        },
        Err(e) => {
            eprintln!("tape annotate: failed to read {}: {e}", taperc.display());
            std::process::exit(2);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_annotate(
    file: &std::path::Path,
    note: Option<String>,
    editor: bool,
    import: Option<std::path::PathBuf>,
    step: Option<u64>,
    actor: Option<String>,
    by: Option<String>,
    out: Option<std::path::PathBuf>,
    in_place: bool,
    ts: Option<String>,
    json: bool,
) -> Result<()> {
    // 0. Load `.taperc::annotate` (issue #192) for the three
    //    fallback fields (default_actor / default_by / editor).
    //    Failed parse exits 2 with the config path named; missing
    //    file / missing section falls through to defaults.
    let annotate_cfg = load_annotate_config();

    // Resolve `by`: CLI flag > .taperc::annotate.default_by >
    // `"human"`. Validate the *resolved* value against
    // `{"agent", "human"}` — clap already enforced the CLI flag
    // shape; this catches a typo in the config file.
    let by_resolved: String = match by {
        Some(v) => v,
        None => match annotate_cfg
            .as_ref()
            .and_then(|(_, c)| c.default_by.clone())
        {
            Some(v) => {
                if v != "agent" && v != "human" {
                    let path = annotate_cfg
                        .as_ref()
                        .map_or_else(|| "<unknown>".to_owned(), |(p, _)| p.display().to_string());
                    eprintln!(
                        "tape annotate: --by: {v:?} from {path} is not one of [\"agent\", \"human\"]",
                    );
                    std::process::exit(2);
                }
                v
            }
            None => "human".to_owned(),
        },
    };
    let by: &str = by_resolved.as_str();

    let editor_override = annotate_cfg.as_ref().and_then(|(_, c)| c.editor.as_deref());

    // 1a. Acquire the note body. clap already enforces the
    //     mutually-exclusive / required-unless-present-any set, so
    //     exactly one of note/editor/import fires.
    let Some(note) = resolve_note_body(file, note, editor, import, by, editor_override) else {
        // `None` is the empty-body cancel from `--editor`. The helper
        // already printed the cancel message; exit 0 with no output.
        return Ok(());
    };

    // 1b. Resolve the output path. `--in-place` overrides to a sibling
    //     temp path; the rename onto `file` happens after the verify
    //     gate at step 9. Default (neither flag set): sibling
    //     `<stem>.annotated.tape` per Phase 1.
    let final_path = file.to_path_buf();
    let out_path = if in_place {
        let pid = std::process::id();
        sibling_path(file, &format!("annotate-tmp-{pid}.tape"))
    } else {
        out.unwrap_or_else(|| sibling_path(file, "annotated.tape"))
    };
    if !in_place && same_path(file, &out_path) {
        eprintln!(
            "tape annotate: --out must differ from <file> (use --in-place for atomic rewrite)"
        );
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
    let note_hits = redact_engine.scan(&note);
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
        payload: serde_json::json!({"by": by, "note": &note}),
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

    // 10. `--in-place`: atomic rename the verified temp onto the input.
    //     `std::fs::rename` is atomic for same-filesystem targets on
    //     Unix, which holds for the sibling temp path we chose. If the
    //     rename itself fails (different filesystem, permissions, etc.)
    //     we leave the temp file in place and exit 2 with a clear
    //     message so the user can recover.
    let reported_path = if in_place {
        if let Err(e) = std::fs::rename(&out_path, &final_path) {
            eprintln!(
                "tape annotate: --in-place rename {} → {} failed: {e}; verified output left at {}",
                out_path.display(),
                final_path.display(),
                out_path.display(),
            );
            std::process::exit(2);
        }
        final_path.clone()
    } else {
        out_path.clone()
    };

    // `--actor` resolution (issue #192): CLI flag > .taperc::annotate.default_actor
    // > `$USER` > "unknown". Each link checks only its own
    // source; the next falls through unchanged from pre-#192.
    let actor_display = actor
        .or_else(|| {
            annotate_cfg
                .as_ref()
                .and_then(|(_, c)| c.default_actor.clone())
        })
        .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "unknown".to_owned()));

    if json {
        let mut payload = serde_json::json!({
            "schema_version": "1",
            "output_path": reported_path.to_string_lossy(),
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
        println!("ok: annotated {}", reported_path.display());
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

/// Recoverable failure from the editor helper. The caller maps each
/// variant to its stderr message + exit code **after** the helper
/// returns (and `tempfile::NamedTempFile` has dropped its scratch
/// file). This indirection is what guarantees the temp file is gone
/// before the process exits — `std::process::exit` skips destructors,
/// so calling it from inside the helper would leak the buffer per
/// issue #158 AC#6 / AC#8 / AC#9.
enum EditorError {
    SpawnFailed(String),
    EditorExitNonZero(String),
    NonUtf8Body,
    OversizeBody(usize),
}

/// `--editor` driver. Writes a comment-stubbed template to a temp file,
/// opens `$VISUAL` / `$EDITOR` / `vi` on it, blocks on the editor, then
/// reads the result. Returns:
///
/// - `Ok(Some(body))` — non-empty body after comment-strip + trim. The
///   16 KiB cap and UTF-8 validity are already verified.
/// - `Ok(None)` — empty body after comment-strip. The caller treats
///   this as a clean cancel and exits 0 with no output cassette.
/// - `Err(EditorError::*)` — recoverable failure variant; the caller
///   maps each to a stderr message + exit 2 after this function has
///   returned, ensuring `tempfile`'s Drop runs first.
///
/// The `tempfile::NamedTempFile` cleans up the buffer on drop, so a
/// panic / signal between launch and read still removes the scratch
/// file — and the explicit `Err(...)` return on each failure path
/// ensures the same Drop runs before the process exits. The
/// defense-in-depth scan runs on the returned body via the existing
/// call in `cmd_annotate`, identical to the `--note` path.
fn compose_note_via_editor(
    file: &std::path::Path,
    by: &str,
    editor_override: Option<&str>,
) -> std::result::Result<Option<String>, EditorError> {
    // 1. Resolve the editor. Precedence (issue #192):
    //    `.taperc::annotate.editor` (when supplied) > `$VISUAL` >
    //    `$EDITOR` > `vi`. Empty / unset env vars are treated as
    //    missing so an exported-but-empty `EDITOR=` doesn't try to
    //    spawn `""`. The `.taperc` value is consulted by the caller
    //    and threaded in via `editor_override`; this helper stays
    //    config-system-agnostic.
    let editor_cmd = editor_override
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| std::env::var("VISUAL").ok().filter(|s| !s.is_empty()))
        .or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "vi".to_owned());

    // 2. Materialise the template into a temp file. Comments start
    //    with `#` and are stripped after the editor exits.
    let template = format!(
        "\n\
         # tape annotate — write your annotation body below.\n\
         # Lines beginning with '#' are stripped before save.\n\
         # An empty body cancels the operation.\n\
         #\n\
         # File: {}\n\
         # By:   {}\n",
        file.display(),
        by,
    );
    let mut tmp = tempfile::NamedTempFile::new()
        .map_err(|e| EditorError::SpawnFailed(format!("create temp file: {e}")))?;
    {
        use std::io::Write as _;
        tmp.write_all(template.as_bytes())
            .map_err(|e| EditorError::SpawnFailed(format!("write template: {e}")))?;
        tmp.flush()
            .map_err(|e| EditorError::SpawnFailed(format!("flush template: {e}")))?;
    }

    // 3. Spawn the editor. Pass the temp path through a shell so
    //    multi-word EDITOR values like `code --wait` work. We use
    //    `/bin/sh -c "$EDITOR \"$0\"" <path>` to keep the path
    //    argument shell-safe.
    let path_arg = tmp.path().to_string_lossy().into_owned();
    let status = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("{editor_cmd} \"$0\""))
        .arg(&path_arg)
        .status();
    let status = status.map_err(|e| {
        EditorError::SpawnFailed(format!("failed to spawn editor {editor_cmd:?}: {e}"))
    })?;
    if !status.success() {
        let code = status.code().map_or("signal".to_owned(), |c| c.to_string());
        return Err(EditorError::EditorExitNonZero(format!(
            "editor {editor_cmd:?} exited with status {code}"
        )));
    }

    // 4. Read the result. Reject non-UTF-8 explicitly so a misbehaving
    //    editor that writes binary garbage doesn't produce a corrupt
    //    annotation payload. The temp file is dropped on the function
    //    return path; returning an `Err` here lets the caller surface
    //    the failure *after* Drop runs.
    let bytes = std::fs::read(tmp.path())
        .map_err(|e| EditorError::SpawnFailed(format!("read edited temp: {e}")))?;
    let Ok(body) = String::from_utf8(bytes) else {
        return Err(EditorError::NonUtf8Body);
    };

    // 5. Strip comment lines (any line whose first non-whitespace
    //    char is `#`) and trim surrounding blank lines. The body is
    //    bounded at 16 KiB after the strip per #74 §3.6.
    let stripped = strip_comments_and_trim(&body);
    if stripped.len() > 16 * 1024 {
        return Err(EditorError::OversizeBody(stripped.len()));
    }
    if stripped.is_empty() {
        return Ok(None);
    }
    Ok(Some(stripped))
    // `tmp` drops here on every Ok path; on the `Err(...)` returns
    // above it drops as the `?` / `return` unwinds the function frame,
    // *before* the caller maps the variant to `std::process::exit(2)`.
}

/// Recoverable failure from the import helper. Mirrors the
/// `EditorError` indirection so the caller can map each variant to its
/// AC-specified stderr message and exit code *after* the helper has
/// returned (cheap insurance against future helpers that hold scratch
/// resources). The owned `PathBuf` is the user-supplied `--import`
/// argument, surfaced verbatim in diagnostics per AC #3 / #4 / #7.
enum ImportError {
    ReadFailed(std::path::PathBuf, std::io::Error),
    NonUtf8(std::path::PathBuf),
    OversizeBody(std::path::PathBuf, usize),
}

/// `--import` driver. Reads a UTF-8 annotation body from disk,
/// trims trailing whitespace + newlines (AC #5), enforces the 16 KiB
/// cap (AC #7), and surfaces empty-after-trim as a clean cancel
/// (`Ok(None)`, AC #6). Unlike `compose_note_via_editor` there is
/// *no* `#`-prefixed comment stripping — `--import` is verbatim per
/// AC #5; the user's file is the body. Non-UTF-8 contents trip the
/// dedicated `NonUtf8` variant rather than the generic IO error
/// (AC #4) so the caller can print the explicit `is not valid UTF-8`
/// diagnostic the AC specifies. The import file is read-only — we
/// never modify or delete it, even on the redaction-leak failure
/// path that runs in the caller.
fn compose_note_via_import(
    path: &std::path::Path,
) -> std::result::Result<Option<String>, ImportError> {
    let body = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            return Err(ImportError::NonUtf8(path.to_path_buf()));
        }
        Err(e) => return Err(ImportError::ReadFailed(path.to_path_buf(), e)),
    };
    let trimmed = body
        .trim_end_matches(|c: char| c.is_whitespace())
        .to_owned();
    if trimmed.len() > 16 * 1024 {
        return Err(ImportError::OversizeBody(path.to_path_buf(), trimmed.len()));
    }
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed))
}

/// Strip lines whose first non-whitespace character is `#`, then trim
/// leading + trailing blank lines from the result. Mid-body blank
/// lines are preserved so paragraph breaks survive the edit.
fn strip_comments_and_trim(body: &str) -> String {
    let mut kept: Vec<&str> = Vec::with_capacity(body.lines().count());
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        kept.push(line);
    }
    // Trim leading + trailing blank lines.
    while kept.first().is_some_and(|l| l.trim().is_empty()) {
        kept.remove(0);
    }
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }
    kept.join("\n")
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
/// Resolve which pricing-file path (if any) `tape stats --with-cost`
/// should consume, applying the issue #186 precedence:
///
/// 1. The `--pricing-file` CLI flag — wins. The diagnostic prefix is
///    empty (the `PricingLoadError` already names the user-supplied
///    path).
/// 2. `.taperc::pricing.pricing_file` — second. Relative paths
///    resolve against the `.taperc`'s parent directory, not `cwd`,
///    so `cd subdir && tape stats ...` doesn't flip the resolved
///    path under the user. The diagnostic prefix is
///    `"(via <.taperc>): "` so a `PricingLoadError` names *both*
///    files per AC.
/// 3. `None` — the renderer falls back to the bundled table.
///
/// Returns `Some((resolved_path, diagnostic_prefix))` for branches 1
/// and 2; `None` for branch 3.
fn resolve_pricing_source(
    cli_flag: Option<&std::path::Path>,
) -> Option<(std::path::PathBuf, String)> {
    if let Some(p) = cli_flag {
        return Some((p.to_path_buf(), String::new()));
    }
    // Probe the workspace + user `.taperc` chain. Use the same
    // locator that the redaction engine uses so the two stay in
    // lockstep on path-discovery rules.
    let cwd = std::env::current_dir().ok()?;
    let taperc_path = tape_redact::config::TapeRcConfig::locate_workspace(&cwd)
        .or_else(tape_redact::config::TapeRcConfig::locate_user)?;
    // Surface read failures rather than silently falling through to the
    // bundled table: `locate_*` confirmed `is_file()`, so an `Err` here
    // is almost certainly an EACCES on a `.taperc` the user expects to
    // be consulted. Symmetry with the parse-error branch below — both
    // exit 2 with a diagnostic naming the `.taperc` path.
    let yaml = match std::fs::read_to_string(&taperc_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("tape stats: failed to read {}: {e}", taperc_path.display());
            std::process::exit(2);
        }
    };
    let cfg = match tape_redact::config::TapeRcConfig::parse(&yaml) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tape stats: failed to parse {}: {e}", taperc_path.display());
            std::process::exit(2);
        }
    };
    let pricing_file = cfg.pricing.pricing_file.as_deref()?;
    let configured = std::path::Path::new(pricing_file);
    let resolved = if configured.is_absolute() {
        configured.to_path_buf()
    } else {
        // `.taperc`'s parent — falls back to `.` if unparented (e.g.
        // a bare-filename test fixture, which won't actually happen
        // in production but is structurally honest).
        taperc_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(configured)
    };
    Some((resolved, format!("(via {}): ", taperc_path.display())))
}

fn cmd_stats(
    file: &std::path::Path,
    format: &str,
    with_cost: bool,
    pricing_file: Option<&std::path::Path>,
) -> Result<()> {
    // Phase-3 of #31 (issue #168): `--with-cost` is text-only for now.
    // The JSON schema would need a `1.1` bump to add `cost_usd`, which
    // is the Phase-4 follow-on. Rejecting up front (before any output)
    // mirrors `tape verify --json`'s no-partial-output posture.
    if with_cost && format == "json" {
        anyhow::bail!(
            "--with-cost is text-only in this release; JSON cost field lands in a follow-on (Phase 4 of issue #31)"
        );
    }
    // Step-4 of #31 (issue #181): `--pricing-file` without
    // `--with-cost` would silently load+validate a TOML file whose
    // contents the run never consults. Soft-warn so the user notices
    // the typo instead of debugging why their custom rates don't show
    // up in the output — but still proceed (the bundled table path
    // would have suppressed cost anyway, so this is informational).
    if pricing_file.is_some() && !with_cost {
        eprintln!(
            "tape stats: --pricing-file has no effect without --with-cost (the cost column is suppressed)"
        );
    }
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
    // Resolve the pricing table with the precedence documented in
    // issue #186: `--pricing-file` CLI flag > `.taperc::pricing.pricing_file`
    // > bundled. `render_stats` (no override) uses the bundled table
    // internally; the `_with_pricing` path is exercised whenever a
    // table override resolves, even when `--with-cost` is absent —
    // in that case the table is loaded for its validation side effect
    // (so `--pricing-file <bad>` still exits 2 even without
    // `--with-cost`).
    let resolved_pricing = resolve_pricing_source(pricing_file);
    let pricing_table = match resolved_pricing {
        Some((path, source_label)) => {
            match tape_play::pricing::PricingTable::load_from_file(&path) {
                Ok(t) => Some(t),
                Err(e) => {
                    eprintln!("tape stats: {source_label}{e}");
                    std::process::exit(2);
                }
            }
        }
        None => None,
    };
    match format {
        // Phase-1 byte-for-byte text. clap's value_parser already
        // rejects anything other than `text` / `json`, so a bare
        // `_` arm here would be dead code.
        "text" => {
            let rendered = match pricing_table {
                Some(t) => tape_play::render_stats_with_pricing(
                    &meta,
                    &tracks,
                    redactions_count,
                    with_cost,
                    &t,
                ),
                None => tape_play::render_stats(&meta, &tracks, redactions_count, with_cost),
            };
            print!("{rendered}");
        }
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

/// Step-1 of issue #8. Renders the cassette to GitHub-flavored
/// Markdown via `tape_export::render_markdown` and writes the result
/// to the resolved output path. No defense-in-depth re-scan (Step 3),
/// no HTML output (Step 2), no audience presets (Step 3).
///
/// The output path resolution mirrors `cmd_recap` / `cmd_annotate`:
/// explicit `-o` wins, otherwise `<basename>.md` next to the input,
/// refusing if it equals the input. Errors during render are reported
/// to stderr with `EXPORT_*` codes for forward-compatible stable
/// diagnostics; the writer itself can only fail with IO errors,
/// which `anyhow` carries up to `main`.
fn cmd_export(file: &std::path::Path, format: &str, out: Option<std::path::PathBuf>) -> Result<()> {
    // 1. Step-1 hard-blocks `html` / `both`. The flag accepts them at
    //    parse time so the CLI surface doesn't need to change when
    //    Step 2 lands — only this guard moves.
    match format {
        "md" => {}
        "html" | "both" => {
            eprintln!(
                "tape export: EXPORT_FORMAT_UNAVAILABLE — `--format {format}` lands in \
                 Step 2 (HTML renderer). Step 1 ships `--format md` only."
            );
            std::process::exit(2);
        }
        _ => {
            eprintln!("tape export: --format must be one of `md`, `html`, `both` (got {format:?})");
            std::process::exit(2);
        }
    }

    // 2. Resolve the output path. The default extension matches
    //    `--format md`; Step 2's HTML default will be `.html`.
    let out_path = if let Some(p) = out {
        p
    } else {
        let stem = file
            .file_stem()
            .map_or_else(|| "tape".to_owned(), |s| s.to_string_lossy().into_owned());
        let parent = file.parent().unwrap_or_else(|| std::path::Path::new("."));
        parent.join(format!("{stem}.md"))
    };
    if same_path(file, &out_path) {
        eprintln!("tape export: --out must differ from <file>");
        std::process::exit(2);
    }

    // 3. Load the cassette and render. `render_markdown` is pure;
    //    every error here is a malformed-input refusal (missing
    //    meta.yaml / tracks.jsonl, parse failures).
    let raw = open_input(file, "tape export");
    let md = match tape_export::render_markdown(&raw) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("tape export: cannot render {}: {e}", file.display());
            std::process::exit(2);
        }
    };

    // 4. Write. Parent-dir creation matches `cmd_recap`'s posture so a
    //    caller can point `-o` into a non-existent sub-directory and
    //    have it materialise.
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("create {}: {e}", parent.display()))?;
        }
    }
    std::fs::write(&out_path, md.as_bytes())
        .map_err(|e| anyhow::anyhow!("write {}: {e}", out_path.display()))?;

    println!("ok: wrote {}", out_path.display());
    Ok(())
}
fn cmd_relinernote(
    file: &std::path::Path,
    model_override: Option<String>,
    dry_run: bool,
    out: Option<std::path::PathBuf>,
    template: &str,
) -> Result<()> {
    // 0. Resolve the template against the bundled catalog (#196).
    //    Unknown names exit 2 with `RELINER_TEMPLATE_NOT_FOUND`
    //    (mirrors `NEW_TEMPLATE_NOT_FOUND`'s shape from #99).
    let template_bundle = match resolve_relinernote_template(template) {
        Some(t) => t,
        None => {
            let known: Vec<&'static str> = RELINERNOTE_TEMPLATES.iter().map(|t| t.id).collect();
            eprintln!(
                "tape relinernote: RELINER_TEMPLATE_NOT_FOUND — unknown template {template:?}; \
                 known: {}",
                known.join(", ")
            );
            std::process::exit(2);
        }
    };
    // 1. Resolve the output path. `--dry-run` doesn't write, so the
    //    path resolution only matters for the write branch — but we
    //    still validate it up front so `--dry-run -o <input>` fails
    //    fast with the same message a real run would emit.
    let out_path = if let Some(p) = out {
        p
    } else {
        let stem = file
            .file_stem()
            .map_or_else(|| "tape".to_owned(), |s| s.to_string_lossy().into_owned());
        let parent = file.parent().unwrap_or_else(|| std::path::Path::new("."));
        parent.join(format!("{stem}.relinernote.tape"))
    };
    if same_path(file, &out_path) {
        eprintln!("tape relinernote: --out must differ from <file>");
        std::process::exit(2);
    }

    // 2. Load the input. `meta.task` must be non-empty per AC #6 —
    //    a task-less cassette has nothing to narrate.
    let raw = open_input(file, "tape relinernote");
    let mut meta = parse_meta(&raw, "tape relinernote");
    if meta.task.trim().is_empty() {
        eprintln!("tape relinernote: RELINER_NO_TASK — meta.task is empty");
        std::process::exit(2);
    }
    let tracks_jsonl = raw.tracks_jsonl.as_deref().unwrap_or("");
    let prior_liner = raw.liner_md.as_deref().unwrap_or("").to_owned();
    let prior_sha = sha256_hex(prior_liner.as_bytes());

    // 3. Build the prompt. Template selects the instruction block;
    //    the cassette-context + track summary + prior-liner suffix
    //    segments are shared across all templates. Track summary is
    //    head+tail-truncated at RELINER_PROMPT_CAP bytes with an
    //    elision marker.
    let prompt = render_relinernote_prompt(template_bundle, &meta, tracks_jsonl, &prior_liner);

    // 4. `--dry-run` stops here — print the rendered prompt, exit 0,
    //    no judge call. Test asserts the client is never invoked.
    if dry_run {
        println!("{prompt}");
        return Ok(());
    }

    // 5. Load config and call the judge.
    let (model_id, judge_out) = match run_relinernote_judge(&prompt, model_override) {
        Ok(pair) => pair,
        Err(code) => std::process::exit(code),
    };

    // 6. Validate the output. Both validators must pass — missing or
    //    empty sections AND order. SPEC §4.1 is "in order"; the
    //    canonical four-section liner notes are what every reader
    //    (including `tape verify`) assumes.
    let new_liner = judge_out.text.trim_end().to_owned();
    let missing = tape_format::liner::missing_or_empty_sections(&new_liner);
    if !missing.is_empty() {
        eprintln!(
            "tape relinernote: RELINER_OUTPUT_INVALID — missing or empty sections: {}",
            missing.join(", ")
        );
        std::process::exit(2);
    }
    if !tape_format::liner::sections_in_order(&new_liner) {
        eprintln!(
            "tape relinernote: RELINER_OUTPUT_INVALID — required H2 sections are not in canonical order"
        );
        std::process::exit(2);
    }

    // 7. Append the audit entry. Hashes are over the canonical bytes
    //    we'll write (so a reader can verify the chain by re-hashing
    //    the on-disk body, not the original CR-terminated source).
    let new_sha = sha256_hex(new_liner.as_bytes());
    meta.relinernotes.push(tape_format::meta::RelinernoteEntry {
        applied_at: chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
        model: model_id,
        template_id: template_bundle.id.to_owned(),
        prior_liner_notes_sha256: prior_sha,
        new_liner_notes_sha256: new_sha,
        judge_call: judge_out.record,
    });

    // 8. Rewrite the zip. Everything but meta.yaml + liner-notes.md
    //    passes through byte-identical: tracks, redactions.json,
    //    artifacts. Same posture `cmd_recap` uses.
    let new_meta_yaml = meta
        .to_yaml()
        .map_err(|e| anyhow::anyhow!("re-serialize meta.yaml: {e}"))?;
    let pending = tape_format::writer::PendingTape {
        meta_yaml: new_meta_yaml,
        liner_md: new_liner,
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

    // 9. Post-write verify. `LEAKED_SECRET_IN_LINER` (SPEC §10.5)
    //    catches any secret-shaped content the defense-in-depth
    //    scanner missed; exit 3 + remove the corrupt output.
    let written = tape_format::reader::RawTape::open(&out_path)?;
    let report = tape_format::verify::verify(&written);
    if !report.is_valid() {
        let _ = std::fs::remove_file(&out_path);
        let codes: Vec<&'static str> = report.errors().map(|d| d.code.as_str()).collect();
        eprintln!(
            "tape relinernote: output failed tape verify ({}); removed {}",
            codes.join(","),
            out_path.display()
        );
        std::process::exit(3);
    }

    println!("ok: regenerated liner-notes.md on {}", out_path.display());
    Ok(())
}

/// Helper extracted from `cmd_relinernote` to keep the driver under the
/// workspace `clippy::too_many_lines` ceiling. Resolves the judge config,
/// applies `--model` if non-empty, drives a single `complete` call, and
/// translates the result into either `(model_id, JudgeOutput)` or the
/// structured exit code the caller should propagate. Diagnostics are
/// emitted to stderr before returning the `Err` arm.
fn run_relinernote_judge(
    prompt: &str,
    model_override: Option<String>,
) -> std::result::Result<(String, tape_judge::JudgeOutput), i32> {
    let (mut config, relinernote_cfg) = match load_judge_and_relinernote_config() {
        Ok(pair) => pair,
        Err(msg) => {
            eprintln!("tape relinernote: RELINER_CONFIG — {msg}");
            return Err(2);
        }
    };
    // Precedence (issue #194): CLI `--model` > `.taperc::relinernote.default_model`
    // > `judge.model`. The first non-empty value wins; the third
    // layer is the unmodified `config.model` from the `judge:`
    // block, used when neither flag nor `.taperc::relinernote` is set.
    let effective_override = model_override.filter(|s| !s.is_empty()).or_else(|| {
        relinernote_cfg
            .default_model
            .clone()
            .filter(|s| !s.is_empty())
    });
    if let Some(m) = effective_override {
        config.model = m;
    }
    let model_id = config.model.clone();

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("tape relinernote: RELINER_CONFIG — build tokio runtime: {e}");
            return Err(2);
        }
    };
    let prompt_owned = prompt.to_owned();
    let result = rt.block_on(async move {
        let client = tape_judge::JudgeClient::new(config)?;
        client
            .complete(&prompt_owned, tape_judge::JudgeOpts::default())
            .await
    });

    match result {
        Ok(o) => Ok((model_id, o)),
        Err(tape_judge::JudgeError::Rejected(hit)) => {
            eprintln!(
                "tape relinernote: RELINER_LEAK — judge output rejected by defense-in-depth: {}",
                hit.rule_id
            );
            Err(6)
        }
        Err(e) => {
            eprintln!("tape relinernote: RELINER_CONFIG — judge call failed: {e}");
            Err(2)
        }
    }
}

/// Locate `.taperc` (workspace first, user-level fallback), parse the
/// `judge:` block, and return the resolved [`tape_judge::JudgeConfig`]
/// alongside the parsed `[relinernote]` config block. The latter
/// supplies the `default_model` fallback layered between CLI `--model`
/// and `judge.model` per issue #194. A failure to read or parse the
/// `.taperc` itself surfaces an exit-2 error with the file path
/// named; an absent `[relinernote]` block returns a default-empty
/// `RelinernoteConfig` rather than failing.
fn load_judge_and_relinernote_config() -> std::result::Result<
    (
        tape_judge::JudgeConfig,
        tape_redact::config::RelinernoteConfig,
    ),
    String,
> {
    let cwd = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    let path = tape_redact::config::TapeRcConfig::locate_workspace(&cwd)
        .or_else(tape_redact::config::TapeRcConfig::locate_user);
    let Some(p) = path else {
        return Err(".taperc not found (looked in workspace and $HOME); \
             needed for relinernote to know the judge model + endpoint"
            .into());
    };
    let yaml = std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
    // Parse the `judge:` block via tape-judge's loader (existing path)
    // and the `[relinernote]` block via the redact-crate parser
    // (post-#194). Two parses against the same bytes; the cost is
    // negligible (the file is small enough that the second parse is
    // a microsecond) and the alternative would be reshaping the
    // tape-judge loader to surface `RelinernoteConfig`, which crosses
    // a crate boundary for one field. Two parses keeps the change
    // local.
    let judge_cfg = tape_judge::JudgeConfig::from_taperc_yaml(&yaml)
        .map_err(|e| format!("parse {}: {e}", p.display()))?
        .ok_or_else(|| {
            format!(
                "{}: no `judge:` block; add one (model + api_key_env) and re-run",
                p.display()
            )
        })?;
    let relinernote_cfg = tape_redact::config::TapeRcConfig::parse(&yaml)
        .map(|c| c.relinernote)
        .map_err(|e| format!("parse {}: {e}", p.display()))?;
    Ok((judge_cfg, relinernote_cfg))
}

/// One built-in prompt template the relinernote CLI knows about.
/// `id` is what the user passes to `--template`; `instructions` is
/// the prose prepended to the cassette context + track summary +
/// prior-liner suffix. All bundled templates require the same four
/// H2 sections (SPEC §4.1) so the output validators stay
/// template-agnostic. (Issue #196.)
struct RelinernoteTemplate {
    id: &'static str,
    instructions: &'static str,
}

/// Bundled relinernote-template catalog. Order is documentation
/// only — `resolve_relinernote_template` does a linear scan, so
/// adding or removing entries is a one-line edit. Grows one
/// template at a time per #71's rollout (#196 added `terse`;
/// `regulatory`/`pedagogical`/`merged` are the queued additions).
const RELINERNOTE_TEMPLATES: &[RelinernoteTemplate] = &[
    RelinernoteTemplate {
        id: "default",
        instructions: "You are regenerating the `liner-notes.md` case insert for one recorded \
             AI-agent investigation. Produce 200–500 words of Markdown. The output \
             MUST contain, in this exact order, these four level-2 headings, each \
             followed by at least one non-empty paragraph or list item before the next:\n\n\
             ## What I was asked to do\n\
             ## What I found\n\
             ## Suggested next step / fix\n\
             ## What I'm uncertain about\n\n\
             Plain Markdown — no front-matter, no code fences around the whole thing, \
             no other H1/H2 sections. Be concrete: name the user-visible outcome, not \
             a meta description of the recording. Do not include any secrets, API keys, \
             emails, or PII; if the source mentions them, refer abstractly.\n\n",
    },
    RelinernoteTemplate {
        id: "terse",
        instructions: "You are regenerating the `liner-notes.md` case insert for one recorded \
             AI-agent investigation. Produce 100–200 words of Markdown — terse and \
             scannable. The output MUST contain, in this exact order, these four \
             level-2 headings, each followed by a short bulleted list (use `-` as \
             the bullet marker; 1–4 bullets per section):\n\n\
             ## What I was asked to do\n\
             ## What I found\n\
             ## Suggested next step / fix\n\
             ## What I'm uncertain about\n\n\
             Bulleted, scannable, one or two short sentences per bullet. Plain \
             Markdown — no front-matter, no code fences, no other H1/H2 sections. \
             Lead each bullet with the concrete fact, not a meta-description. Do not \
             include any secrets, API keys, emails, or PII; if the source mentions \
             them, refer abstractly.\n\n",
    },
];

fn resolve_relinernote_template(id: &str) -> Option<&'static RelinernoteTemplate> {
    RELINERNOTE_TEMPLATES.iter().find(|t| t.id == id)
}

/// Build the prompt from the resolved template bundle. Instructions
/// first, then the cassette context, then the track summary, then
/// the existing liner notes. The order matters: an oversized tracks
/// summary should never push the instructions out of the model's
/// effective context.
fn render_relinernote_prompt(
    template: &RelinernoteTemplate,
    meta: &tape_format::meta::Meta,
    tracks_jsonl: &str,
    prior_liner: &str,
) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(8 * 1024);
    s.push_str(template.instructions);
    let _ = writeln!(s, "Task: {}", meta.task);
    let outcome = match meta.outcome {
        tape_format::meta::Outcome::Success => "success",
        tape_format::meta::Outcome::Failure => "failure",
        tape_format::meta::Outcome::Abandoned => "abandoned",
        tape_format::meta::Outcome::Unknown => "unknown",
    };
    let _ = writeln!(s, "Outcome: {outcome}");
    if let Some(label) = meta.label.as_deref() {
        let _ = writeln!(s, "Label: {label}");
    }
    s.push_str("\nTracks (one line per step):\n");
    s.push_str(&relinernote_track_summary(tracks_jsonl, RELINER_PROMPT_CAP));
    if !prior_liner.trim().is_empty() {
        s.push_str("\nPrior liner notes (for context — feel free to rewrite):\n");
        s.push_str(prior_liner.trim());
        s.push('\n');
    }
    s
}

/// 8 KiB cap on the tracks-summary slice of the prompt, per Principal
/// scoping in #71. Tracks above the cap are head+tail-truncated with
/// an explicit `… N tracks elided …` marker so both ends of long
/// investigations stay visible.
const RELINER_PROMPT_CAP: usize = 8 * 1024;

fn relinernote_track_summary(tracks_jsonl: &str, byte_cap: usize) -> String {
    let Ok(tracks) = tape_format::tracks::parse_jsonl(tracks_jsonl) else {
        return "(tracks did not parse)\n".to_owned();
    };
    let lines: Vec<String> = tracks.iter().map(relinernote_track_line).collect();
    let mut full = String::new();
    for l in &lines {
        full.push_str(l);
        full.push('\n');
    }
    if full.len() <= byte_cap {
        return full;
    }
    let elide = format!(
        "… {} tracks elided (budget {byte_cap} bytes) …\n",
        lines.len()
    );
    let side = byte_cap.saturating_sub(elide.len()) / 2;
    let head = char_safe_prefix(&full, side);
    let tail = char_safe_suffix(&full, side);
    let mut out = String::with_capacity(byte_cap);
    out.push_str(&head);
    out.push_str(&elide);
    out.push_str(&tail);
    out
}

fn relinernote_track_line(t: &tape_format::tracks::Track) -> String {
    let kind = match t.kind {
        tape_format::tracks::Kind::Task => "task",
        tape_format::tracks::Kind::ModelCall => "model_call",
        tape_format::tracks::Kind::McpCall => "mcp_call",
        tape_format::tracks::Kind::Shell => "shell",
        tape_format::tracks::Kind::FileRead => "file_read",
        tape_format::tracks::Kind::FileWrite => "file_write",
        tape_format::tracks::Kind::Annotation => "annotation",
        tape_format::tracks::Kind::Eject => "eject",
    };
    let hint = relinernote_payload_hint(t.kind, &t.payload);
    if hint.is_empty() {
        format!("  {:>3}. {kind}", t.step)
    } else {
        format!("  {:>3}. {kind}: {hint}", t.step)
    }
}

fn relinernote_payload_hint(
    kind: tape_format::tracks::Kind,
    payload: &serde_json::Value,
) -> String {
    let key = match kind {
        tape_format::tracks::Kind::Task => "prompt",
        tape_format::tracks::Kind::ModelCall => "model",
        tape_format::tracks::Kind::McpCall => "tool",
        tape_format::tracks::Kind::Shell => "command",
        tape_format::tracks::Kind::FileRead | tape_format::tracks::Kind::FileWrite => "path",
        tape_format::tracks::Kind::Annotation => "note",
        tape_format::tracks::Kind::Eject => "outcome",
    };
    let raw = payload
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .replace(['\n', '\r'], " ");
    let truncated: String = raw.chars().take(120).collect();
    if raw.chars().count() > 120 {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn char_safe_prefix(s: &str, byte_cap: usize) -> String {
    if s.len() <= byte_cap {
        return s.to_owned();
    }
    let mut end = byte_cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}

fn char_safe_suffix(s: &str, byte_cap: usize) -> String {
    if s.len() <= byte_cap {
        return s.to_owned();
    }
    let mut start = s.len().saturating_sub(byte_cap);
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].to_owned()
}

/// Lowercase-hex SHA-256 of a byte slice. Used for the `meta.relinernotes[]`
/// hash chain. blake3 is the workspace's preferred hash, but SPEC §4 / the
/// audit-row convention in `meta.relinernotes` calls for SHA-256 explicitly
/// (Principal AC #4 names `prior_liner_notes_sha256` / `new_liner_notes_sha256`).
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    use std::fmt::Write as _;
    let digest = sha2::Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest.as_slice() {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// `tape anon <file> [-o <path>]` — Phase 1 of issue #42 / #204.
///
/// Strip absolute `$HOME`-style file paths from a cassette and write a
/// NEW cassette next to the input. The input is never mutated. On a
/// successful run, prints one stderr line summarizing the replacement
/// count and any artifacts that were left untouched (Phase 1 does not
/// scan binary content — that's `--aggressive` in Phase 4).
///
/// Exit codes (per #204):
/// - `0` — anonymization succeeded; output cassette written.
/// - `2` — usage error (output path equals input, output already
///   exists, OS RNG failure deriving salt).
/// - `3` — input cassette failed to parse / open.
/// - `4` — defense-in-depth re-scan found a leftover identifier; no
///   output cassette is left on disk.
fn cmd_anon(file: &std::path::Path, out: Option<std::path::PathBuf>) -> Result<()> {
    // 1. Resolve output path (default: `<basename>.anon.tape` next to input).
    let out_path = if let Some(p) = out {
        p
    } else {
        let stem = file
            .file_stem()
            .map_or_else(|| "tape".to_owned(), |s| s.to_string_lossy().into_owned());
        let parent = file.parent().unwrap_or_else(|| std::path::Path::new("."));
        parent.join(format!("{stem}.anon.tape"))
    };

    // 2. Refuse if out == in (per #42 §3.1: anon NEVER writes back).
    if same_path(file, &out_path) {
        eprintln!("tape anon: --out must differ from input path (anon never writes in place)");
        std::process::exit(2);
    }

    // 3. Refuse if the output already exists (no `--force` in Phase 1).
    if out_path.exists() {
        eprintln!("tape anon: --out path already exists; refusing to overwrite");
        std::process::exit(2);
    }

    // 4. Run the anon engine. Exit codes mapped per ticket §"Exit codes".
    let opts = tape_anon::AnonOptions {
        in_path: file.to_path_buf(),
        out_path: out_path.clone(),
    };
    match tape_anon::run_anon(opts) {
        Ok(report) => {
            eprintln!(
                "tape anon: wrote {} ({} replacements via unix_home_path)",
                out_path.display(),
                report.n_replacements
            );
            if report.n_artifacts_skipped > 0 {
                eprintln!(
                    "tape anon: skipped {} artifacts (Phase 1; --aggressive will scan content in Phase 4)",
                    report.n_artifacts_skipped
                );
            }
            Ok(())
        }
        Err(tape_anon::AnonError::InputUnreadable(e)) => {
            eprintln!("tape anon: {e}");
            std::process::exit(3);
        }
        Err(e @ tape_anon::AnonError::PostAnonLeak { .. }) => {
            eprintln!("{e}");
            std::process::exit(4);
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}

// =====================================================================
// `tape to-otlp` — Phase 1 of issue #88 / #209.
//
// Pure data-shape transform: read a `.tape`, emit OpenTelemetry traces
// as OTLP/JSON to stdout (or --output). One span per track, flat walk.
// Out of scope for Phase 1: protobuf, gRPC, --endpoint, --include-kind /
// --exclude-kind / --max-tracks / --trace-id / semconv renaming /
// defense-in-depth re-scan / annotations-as-events / any format other
// than OTLP/JSON. See #88 for the full vision; this slice ships the
// load-bearing engine shape with the minimum surface.
//
// OTLP/JSON spec reference: https://opentelemetry.io/docs/specs/otlp/
// Hand-rolled via serde (per non-goal: no `opentelemetry` crate dep).
// =====================================================================

/// 4096-byte truncation cap per #88 §3.5. Attributes longer than this
/// get truncated with a sibling `<key>.truncated = true` co-attribute.
const OTLP_ATTRIBUTE_MAX_BYTES: usize = 4096;

#[derive(serde::Serialize)]
struct OtlpExport {
    #[serde(rename = "resourceSpans")]
    resource_spans: Vec<OtlpResourceSpans>,
}

#[derive(serde::Serialize)]
struct OtlpResourceSpans {
    resource: OtlpResource,
    #[serde(rename = "scopeSpans")]
    scope_spans: Vec<OtlpScopeSpans>,
}

#[derive(serde::Serialize)]
struct OtlpResource {
    attributes: Vec<OtlpAttribute>,
}

#[derive(serde::Serialize)]
struct OtlpScopeSpans {
    scope: OtlpScope,
    spans: Vec<OtlpSpan>,
}

#[derive(serde::Serialize)]
struct OtlpScope {
    name: String,
    version: String,
}

#[derive(serde::Serialize)]
struct OtlpSpan {
    #[serde(rename = "traceId")]
    trace_id: String,
    #[serde(rename = "spanId")]
    span_id: String,
    #[serde(rename = "parentSpanId", skip_serializing_if = "Option::is_none")]
    parent_span_id: Option<String>,
    name: String,
    /// `SPAN_KIND_INTERNAL` for every Phase-1 span — Phase 2+ may
    /// distinguish CLIENT (model_call) / SERVER (mcp_call) / etc.
    kind: u32,
    #[serde(rename = "startTimeUnixNano")]
    start_time_unix_nano: String,
    #[serde(rename = "endTimeUnixNano")]
    end_time_unix_nano: String,
    attributes: Vec<OtlpAttribute>,
}

#[derive(serde::Serialize)]
struct OtlpAttribute {
    key: String,
    value: OtlpAnyValue,
}

/// OTLP `AnyValue` (one-of). We only emit the four variants the
/// Phase-1 flattener produces; the OTLP spec defines bytes/array/kvlist
/// too but Phase 1 has no use for them.
#[derive(serde::Serialize)]
#[serde(untagged)]
enum OtlpAnyValue {
    String {
        #[serde(rename = "stringValue")]
        string_value: String,
    },
    Bool {
        #[serde(rename = "boolValue")]
        bool_value: bool,
    },
    Int {
        #[serde(rename = "intValue")]
        int_value: String,
    },
    Double {
        #[serde(rename = "doubleValue")]
        double_value: f64,
    },
}

/// `tape to-otlp <cassette> [--output <path>]` — Phase 1 of #88.
fn cmd_to_otlp(file: &std::path::Path, output: Option<std::path::PathBuf>) -> Result<()> {
    // 1. Reject `--output == file` before any work (cheap guard).
    if let Some(ref out) = output {
        if same_path(file, out) {
            eprintln!("tape to-otlp: --output must differ from input path");
            std::process::exit(2);
        }
    }

    // 2. Open cassette + parse tracks/meta. Reuses the existing
    //    helpers so exit codes match the other read-only consumers.
    let raw = open_input(file, "tape to-otlp");
    let meta = parse_meta(&raw, "tape to-otlp");
    let tracks = match raw.tracks_jsonl.as_deref() {
        Some(jsonl) => match tape_format::tracks::parse_jsonl(jsonl) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("tape to-otlp: tracks.jsonl parse failed: {e}");
                std::process::exit(2);
            }
        },
        None => Vec::new(),
    };

    // 3. Build the OTLP document. Deterministic span ids derived from
    //    a cassette-stable digest + step number (AC #5: re-runs of
    //    the same cassette emit identical span ids). trace id is
    //    fresh-random per invocation.
    let cassette_digest = cassette_digest_for_span_ids(&raw);
    let trace_id = random_trace_id_hex();
    let export = build_otlp_export(&meta, &tracks, &cassette_digest, &trace_id);

    // 4. Serialize. Pretty-print so consumers can grep / eyeball.
    let mut json = serde_json::to_string_pretty(&export)
        .map_err(|e| anyhow::anyhow!("serialize OTLP/JSON: {e}"))?;
    json.push('\n');

    // 5. Write.
    if let Some(out_path) = output {
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow::anyhow!("create {}: {e}", parent.display()))?;
            }
        }
        std::fs::write(&out_path, json.as_bytes())
            .map_err(|e| anyhow::anyhow!("write {}: {e}", out_path.display()))?;
        eprintln!("tape to-otlp: wrote {}", out_path.display());
    } else {
        use std::io::Write as _;
        let stdout = std::io::stdout();
        let mut h = stdout.lock();
        h.write_all(json.as_bytes())
            .map_err(|e| anyhow::anyhow!("write stdout: {e}"))?;
    }
    Ok(())
}

/// Cassette-stable digest used as the span-id seed. Hashes the
/// canonical input bytes the writer would have produced if we
/// re-rendered: `meta.yaml` + `tracks.jsonl`. This keeps the seed
/// invariant under zip-level re-write quirks (compression level,
/// timestamps inside the archive) — two cassettes that parse to the
/// same Meta + Tracks produce identical span ids.
fn cassette_digest_for_span_ids(raw: &tape_format::reader::RawTape) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    if let Some(m) = raw.meta_yaml.as_deref() {
        hasher.update(m.as_bytes());
    }
    hasher.update(&[0x1F]); // unit separator
    if let Some(t) = raw.tracks_jsonl.as_deref() {
        hasher.update(t.as_bytes());
    }
    *hasher.finalize().as_bytes()
}

/// 16-byte trace id, hex-encoded (32 chars). Fresh-random per
/// invocation; the deterministic-output AC (#5) excludes this.
fn random_trace_id_hex() -> String {
    use std::fmt::Write as _;
    let mut bytes = [0u8; 16];
    // getrandom already a workspace dep via #204's tape-anon.
    getrandom::getrandom(&mut bytes).expect("OS RNG must produce 16 random bytes");
    let mut out = String::with_capacity(32);
    for b in &bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Deterministic 8-byte span id derived from `(cassette_digest, step)`.
/// `BLAKE3(cassette_digest || step.to_be_bytes())[..8]`, hex-encoded
/// (16 chars).
fn span_id_for(cassette_digest: &[u8; 32], step: u64) -> String {
    use std::fmt::Write as _;
    let mut hasher = blake3::Hasher::new();
    hasher.update(cassette_digest);
    hasher.update(&step.to_be_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(16);
    for b in &digest.as_bytes()[..8] {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn kind_to_name(k: tape_format::tracks::Kind) -> &'static str {
    use tape_format::tracks::Kind;
    match k {
        Kind::Task => "task",
        Kind::ModelCall => "model_call",
        Kind::McpCall => "mcp_call",
        Kind::Shell => "shell",
        Kind::FileRead => "file_read",
        Kind::FileWrite => "file_write",
        Kind::Annotation => "annotation",
        Kind::Eject => "eject",
    }
}

/// Parse an RFC 3339 timestamp into a nanos-since-epoch string. OTLP
/// JSON requires `*UnixNano` fields as strings (64-bit values that
/// would lose precision under JSON's number type — same reason
/// protobuf encodes them as int64).
fn ts_to_nanos_str(ts: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .and_then(|dt| dt.timestamp_nanos_opt())
        .map_or_else(|| "0".to_owned(), |n| n.to_string())
}

/// Flatten `track.payload` into OTLP attributes. Top-level scalars
/// become typed attributes (string/bool/int/double); nested
/// objects/arrays serialize to JSON strings. Anything over 4096 bytes
/// is truncated and gets a `<key>.truncated = true` co-attribute.
fn payload_to_attributes(payload: &serde_json::Value) -> Vec<OtlpAttribute> {
    let mut out = Vec::new();
    let Some(obj) = payload.as_object() else {
        // Non-object payload (rare; would be a SPEC violation but we
        // don't enforce that here). Emit the whole thing as one
        // `payload` string attribute.
        let s = payload.to_string();
        push_attr_with_truncation(&mut out, "payload", &s);
        return out;
    };
    for (k, v) in obj {
        match v {
            serde_json::Value::String(s) => push_attr_with_truncation(&mut out, k, s),
            serde_json::Value::Bool(b) => out.push(OtlpAttribute {
                key: k.clone(),
                value: OtlpAnyValue::Bool { bool_value: *b },
            }),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    out.push(OtlpAttribute {
                        key: k.clone(),
                        value: OtlpAnyValue::Int {
                            int_value: i.to_string(),
                        },
                    });
                } else if let Some(f) = n.as_f64() {
                    out.push(OtlpAttribute {
                        key: k.clone(),
                        value: OtlpAnyValue::Double { double_value: f },
                    });
                } else {
                    // u64 outside i64 range — emit as string.
                    push_attr_with_truncation(&mut out, k, &n.to_string());
                }
            }
            serde_json::Value::Null => {
                // OTLP `AnyValue` permits the empty variant for null;
                // simpler: emit as an empty string attribute, which
                // preserves the key.
                push_attr_with_truncation(&mut out, k, "");
            }
            // Nested objects/arrays → serialized JSON, then truncated.
            serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                let s = v.to_string();
                push_attr_with_truncation(&mut out, k, &s);
            }
        }
    }
    out
}

fn push_attr_with_truncation(out: &mut Vec<OtlpAttribute>, key: &str, value: &str) {
    if value.len() <= OTLP_ATTRIBUTE_MAX_BYTES {
        out.push(OtlpAttribute {
            key: key.to_owned(),
            value: OtlpAnyValue::String {
                string_value: value.to_owned(),
            },
        });
        return;
    }
    // UTF-8-safe truncation: walk back to the last char boundary
    // at-or-before the byte cap.
    let mut cut = OTLP_ATTRIBUTE_MAX_BYTES;
    while cut > 0 && !value.is_char_boundary(cut) {
        cut -= 1;
    }
    out.push(OtlpAttribute {
        key: key.to_owned(),
        value: OtlpAnyValue::String {
            string_value: value[..cut].to_owned(),
        },
    });
    out.push(OtlpAttribute {
        key: format!("{key}.truncated"),
        value: OtlpAnyValue::Bool { bool_value: true },
    });
}

/// Assemble the full OTLP/JSON document from parsed meta + tracks +
/// span-id seed + trace-id.
fn build_otlp_export(
    meta: &tape_format::meta::Meta,
    tracks: &[tape_format::tracks::Track],
    cassette_digest: &[u8; 32],
    trace_id: &str,
) -> OtlpExport {
    // Pre-resolve span ids so parent_step lookups work.
    let span_id_by_step: std::collections::HashMap<u64, String> = tracks
        .iter()
        .map(|t| (t.step, span_id_for(cassette_digest, t.step)))
        .collect();

    let mut spans = Vec::with_capacity(tracks.len());
    for (i, t) in tracks.iter().enumerate() {
        let start_ns = ts_to_nanos_str(&t.ts);
        // End time = start of the next track; for the final track,
        // reuse start (zero-duration point-in-time event).
        let end_ns = tracks
            .get(i + 1)
            .map_or_else(|| start_ns.clone(), |next| ts_to_nanos_str(&next.ts));
        let parent = t
            .parent_step
            .and_then(|ps| span_id_by_step.get(&ps).cloned());
        spans.push(OtlpSpan {
            trace_id: trace_id.to_owned(),
            span_id: span_id_by_step.get(&t.step).cloned().unwrap_or_default(),
            parent_span_id: parent,
            name: kind_to_name(t.kind).to_owned(),
            kind: 1, // SPAN_KIND_INTERNAL
            start_time_unix_nano: start_ns,
            end_time_unix_nano: end_ns,
            attributes: payload_to_attributes(&t.payload),
        });
    }

    OtlpExport {
        resource_spans: vec![OtlpResourceSpans {
            resource: OtlpResource {
                attributes: vec![
                    OtlpAttribute {
                        key: "service.name".to_owned(),
                        value: OtlpAnyValue::String {
                            string_value: "tape".to_owned(),
                        },
                    },
                    OtlpAttribute {
                        key: "tape.cassette.task".to_owned(),
                        value: OtlpAnyValue::String {
                            string_value: meta.task.clone(),
                        },
                    },
                ],
            },
            scope_spans: vec![OtlpScopeSpans {
                scope: OtlpScope {
                    name: "tape".to_owned(),
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                },
                spans,
            }],
        }],
    }
}

#[cfg(test)]
mod to_otlp_tests {
    use super::*;

    fn fixed_digest() -> [u8; 32] {
        [0x42; 32]
    }

    #[test]
    fn span_id_is_deterministic_for_same_inputs() {
        let a = span_id_for(&fixed_digest(), 1);
        let b = span_id_for(&fixed_digest(), 1);
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn span_id_differs_per_step() {
        let a = span_id_for(&fixed_digest(), 1);
        let b = span_id_for(&fixed_digest(), 2);
        assert_ne!(a, b);
    }

    #[test]
    fn span_id_differs_per_cassette_digest() {
        let a = span_id_for(&[0x01; 32], 1);
        let b = span_id_for(&[0x02; 32], 1);
        assert_ne!(a, b);
    }

    #[test]
    fn random_trace_id_is_32_hex_chars() {
        let t = random_trace_id_hex();
        assert_eq!(t.len(), 32);
        assert!(t
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn ts_to_nanos_rfc3339_roundtrip() {
        let ns = ts_to_nanos_str("2026-05-16T00:00:00Z");
        // Sanity-check: the produced value matches chrono's own
        // computation for the same timestamp (avoids hand-pinning a
        // magic constant that depends on which calendar reform you
        // count).
        let expected = chrono::DateTime::parse_from_rfc3339("2026-05-16T00:00:00Z")
            .unwrap()
            .timestamp_nanos_opt()
            .unwrap()
            .to_string();
        assert_eq!(ns, expected);
    }

    #[test]
    fn ts_to_nanos_bad_input_returns_zero() {
        assert_eq!(ts_to_nanos_str("not-a-timestamp"), "0");
    }

    #[test]
    fn payload_flattens_scalars_to_typed_attrs() {
        let payload = serde_json::json!({
            "s": "hello",
            "b": true,
            "i": 42,
            "f": 2.5,
        });
        let attrs = payload_to_attributes(&payload);
        // Order is not stable (serde_json::Value::as_object yields a
        // BTreeMap-ish iteration in serde_json 1.x); look up by key.
        let by_key: std::collections::HashMap<_, _> =
            attrs.iter().map(|a| (a.key.as_str(), &a.value)).collect();
        assert!(matches!(by_key["s"], OtlpAnyValue::String { .. }));
        assert!(matches!(
            by_key["b"],
            OtlpAnyValue::Bool { bool_value: true }
        ));
        assert!(matches!(by_key["i"], OtlpAnyValue::Int { .. }));
        assert!(matches!(by_key["f"], OtlpAnyValue::Double { .. }));
    }

    #[test]
    fn payload_flattens_nested_to_json_string() {
        let payload = serde_json::json!({
            "nested": {"a": 1, "b": 2},
            "list": [1, 2, 3],
        });
        let attrs = payload_to_attributes(&payload);
        for a in &attrs {
            assert!(
                matches!(a.value, OtlpAnyValue::String { .. }),
                "expected nested object/array → string, got key={}",
                a.key
            );
        }
    }

    #[test]
    fn truncation_caps_at_4096_bytes_with_co_attr() {
        let big = "x".repeat(5000);
        let payload = serde_json::json!({"big": big});
        let attrs = payload_to_attributes(&payload);
        // Find the big attr + the truncated co-attr.
        let mut by_key: std::collections::HashMap<_, _> =
            attrs.iter().map(|a| (a.key.as_str(), &a.value)).collect();
        let big_val = by_key.remove("big").expect("big attr");
        if let OtlpAnyValue::String { string_value } = big_val {
            assert!(
                string_value.len() <= OTLP_ATTRIBUTE_MAX_BYTES,
                "big attr value not truncated; got {} bytes",
                string_value.len()
            );
        } else {
            panic!("big attr should be a String variant");
        }
        let truncated_marker = by_key
            .remove("big.truncated")
            .expect("co-attr big.truncated must be present");
        assert!(matches!(
            truncated_marker,
            OtlpAnyValue::Bool { bool_value: true }
        ));
    }

    #[test]
    fn truncation_skips_short_attrs_no_co_attr() {
        let payload = serde_json::json!({"small": "abc"});
        let attrs = payload_to_attributes(&payload);
        let keys: Vec<&str> = attrs.iter().map(|a| a.key.as_str()).collect();
        assert_eq!(keys, vec!["small"]); // no `small.truncated`
    }

    #[test]
    fn build_otlp_export_links_parents_correctly() {
        let meta = tape_format::meta::Meta::parse(
            "tape_version: \"tape/v0\"\n\
             id: \"01h8xy00-0000-7000-b8aa-000000000209\"\n\
             created_at: \"2026-05-16T00:00:00Z\"\n\
             ejected_at: \"2026-05-16T00:00:30Z\"\n\
             task: \"investigate\"\n\
             recorder:\n  agent: \"test/0.0.1\"\n\
             outcome: success\n",
        )
        .unwrap();
        let tracks = vec![
            tape_format::tracks::Track {
                step: 1,
                kind: tape_format::tracks::Kind::Task,
                ts: "2026-05-16T00:00:00Z".into(),
                payload: serde_json::json!({"prompt": "investigate"}),
                parent_step: None,
                refs: Vec::new(),
                annotations: Vec::new(),
            },
            tape_format::tracks::Track {
                step: 2,
                kind: tape_format::tracks::Kind::Annotation,
                ts: "2026-05-16T00:00:05Z".into(),
                payload: serde_json::json!({"by": "agent", "note": "thinking"}),
                parent_step: Some(1),
                refs: Vec::new(),
                annotations: Vec::new(),
            },
        ];
        let export = build_otlp_export(
            &meta,
            &tracks,
            &fixed_digest(),
            "0123456789abcdef0123456789abcdef",
        );
        let spans = &export.resource_spans[0].scope_spans[0].spans;
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].name, "task");
        assert!(spans[0].parent_span_id.is_none(), "root span has no parent");
        assert_eq!(spans[1].name, "annotation");
        assert_eq!(
            spans[1].parent_span_id.as_deref(),
            Some(spans[0].span_id.as_str()),
            "child's parentSpanId must match parent's spanId"
        );
        // End time of span 1 = start time of span 2 (1 → 2 transition).
        assert_eq!(spans[0].end_time_unix_nano, spans[1].start_time_unix_nano);
        // Final span is zero-duration.
        assert_eq!(spans[1].start_time_unix_nano, spans[1].end_time_unix_nano);
    }
}
