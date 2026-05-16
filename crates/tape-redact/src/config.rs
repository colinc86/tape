//! `.taperc` configuration. See SPEC.md §9.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TapeRcConfig {
    #[serde(default)]
    pub redact: RedactConfig,
    /// `[pricing]` block. Currently single-field; gains options as later
    /// `tape stats --with-cost` slices land. Issue #186.
    #[serde(default)]
    pub pricing: PricingConfig,
    /// `[new]` block. Currently single-field; gains options as later
    /// `tape new` slices land. Issue #190.
    #[serde(default)]
    pub new: NewConfig,
    /// `[annotate]` block. Three optional fallback fields for the
    /// `tape annotate` flag surface. Issue #192.
    #[serde(default)]
    pub annotate: AnnotateConfig,
    /// `[relinernote]` block. Currently single-field; gains options
    /// as later `tape relinernote` slices land. Issue #194.
    #[serde(default)]
    pub relinernote: RelinernoteConfig,
    /// `[recap]` block. Currently single-field; gains options as
    /// later `tape recap` slices land. Issue #198.
    #[serde(default)]
    pub recap: RecapConfig,
}

/// `.taperc::pricing` block. One field today: `pricing_file`, the
/// default `--pricing-file` path consumed by `tape stats --with-cost`.
/// Relative paths in this field resolve against the `.taperc`'s
/// parent directory, not the user's cwd — see `cmd_stats` for the
/// resolver wiring. (Issue #186 / Step-5 of #31.)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricingConfig {
    /// Path to a TOML pricing table. Same schema as `--pricing-file`
    /// (issue #181). When set, `tape stats --with-cost` falls back to
    /// this path if the `--pricing-file` flag is not supplied.
    #[serde(default)]
    pub pricing_file: Option<String>,
}

/// `.taperc::new` block. One field today: `default_template`, the
/// template id consumed by `tape new` when `--template` is not
/// supplied. (Issue #190 / Step-5 of #99.)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewConfig {
    /// Default template id (e.g. `minimal`, `test-fixture`,
    /// `bug-investigation`). Validated against the built-in
    /// template catalog by `dispatch_new`; an unknown id surfaces
    /// the same `NEW_TEMPLATE_NOT_FOUND` diagnostic that
    /// `--template <unknown>` already emits.
    #[serde(default)]
    pub default_template: Option<String>,
}

/// `.taperc::annotate` block. Three optional fallbacks consumed by
/// `tape annotate` when the corresponding CLI flags / env vars are
/// absent. CLI flags still win; the config is a per-user defaults
/// layer between the flag and the binary's built-in defaults.
/// (Issue #192 / Step-4a of #74.)
///
/// `default_kind` / `default_pin` / `strict_kind` from #74 §3.11 are
/// **deliberately absent** — they depend on the not-yet-shipped
/// `--kind` / `--pin` payload-fields slice. `deny_unknown_fields`
/// keeps that boundary load-bearing: a user who tries to set them
/// today gets a clean typo-style error rather than a silent no-op.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotateConfig {
    /// Default `--actor` value. Resolution order:
    /// CLI `--actor` > this field > `$USER` > `"unknown"`.
    #[serde(default)]
    pub default_actor: Option<String>,
    /// Default `--by` value. Validation set: `{"agent", "human"}`,
    /// applied to the *resolved* value (CLI flag if present; else
    /// this field; else `"human"`). An invalid resolved value
    /// surfaces an exit-2 diagnostic naming the `.taperc` path.
    /// Resolution order: CLI `--by` > this field > `"human"`.
    #[serde(default)]
    pub default_by: Option<String>,
    /// Editor command consumed by `tape annotate --editor`. When
    /// set, takes precedence over `$VISUAL` / `$EDITOR` / `vi`.
    /// Resolution order: this field > `$VISUAL` > `$EDITOR` > `vi`.
    /// Dormant when `--editor` is not passed.
    #[serde(default)]
    pub editor: Option<String>,
}

/// `.taperc::relinernote` block. One field today: `default_model`,
/// which overrides the `judge:` block's `model` field for `tape
/// relinernote` only (the other tape-judge consumers — `tape diff
/// --judge` / `tape recap --auto` — are unchanged). Resolution
/// order: CLI `--model` > this field > `judge.model`.
///
/// `default_template_id` / `default_temperature` / `default_max_tokens`
/// / `default_report` are deliberately absent — they depend on
/// the not-yet-shipped `--template-id` / `--temperature` /
/// `--max-tokens` / `--report` flags from #71. `deny_unknown_fields`
/// keeps that boundary load-bearing: a user who tries to set them
/// today gets a clean typo-style error rather than a silent no-op.
/// (Issue #194 / Step-2 of #71.)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelinernoteConfig {
    /// Default judge-model id. When set, takes precedence over the
    /// `judge:` block's `model` field for `tape relinernote` only.
    /// CLI `--model` still wins.
    #[serde(default)]
    pub default_model: Option<String>,
}

/// `.taperc::recap` block. One field today: `default_model`, which
/// overrides the `judge:` block's `model` field for `tape recap
/// --auto` only (the other tape-judge consumers — `tape diff
/// --judge`, `tape relinernote` — are unchanged). Resolution
/// order: CLI `--model` > this field > `judge.model`.
///
/// `default_template_id` / `default_temperature` / `default_max_tokens`
/// / `default_report` are deliberately absent — they depend on
/// not-yet-shipped flags from #105's Phase-3+ rollout.
/// `deny_unknown_fields` keeps that boundary load-bearing: a user
/// who tries to set them today gets a clean typo-style error
/// rather than a silent no-op. (Issue #198 / Step-3 of #105.)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecapConfig {
    /// Default judge-model id. When set, takes precedence over the
    /// `judge:` block's `model` field for `tape recap --auto`
    /// only. CLI `--model` still wins.
    #[serde(default)]
    pub default_model: Option<String>,
}

/// SPEC §9.1: unknown keys under `redact:` MUST cause a config-load failure.
/// `TapeRcConfig` stays permissive at the top level (forward-compat) but this
/// struct denies typos so users learn fast when `disable_default` becomes
/// `disabled_default`. (Issue #36.)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactConfig {
    #[serde(default)]
    pub custom: Vec<CustomRule>,
    #[serde(default)]
    pub enable_optional: Vec<String>,
    #[serde(default)]
    pub disable_default: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRule {
    pub id: String,
    pub pattern: String,
    #[serde(default)]
    pub replacement: Option<String>,
}

impl TapeRcConfig {
    pub fn parse(yaml: &str) -> anyhow::Result<Self> {
        Ok(serde_yaml::from_str(yaml)?)
    }

    /// Walk from `cwd` up to the user's home, returning the first `.taperc`
    /// found, or None.
    pub fn locate_workspace(cwd: &std::path::Path) -> Option<std::path::PathBuf> {
        let mut current = Some(cwd.to_path_buf());
        let home = dirs_home();
        while let Some(dir) = current {
            let candidate = dir.join(".taperc");
            if candidate.is_file() {
                return Some(candidate);
            }
            if home.as_deref() == Some(dir.as_path()) {
                return None;
            }
            current = dir.parent().map(|p| p.to_path_buf());
        }
        None
    }

    pub fn locate_user() -> Option<std::path::PathBuf> {
        let home = dirs_home()?;
        let candidate = home.join(".taperc");
        candidate.is_file().then_some(candidate)
    }

    /// Apply this config to an engine: enable opt-in built-ins, disable
    /// defaults, append custom rules.
    pub fn apply(&self, engine: &mut crate::Engine) -> anyhow::Result<()> {
        // SPEC §9.2: `disable_default` only targets the built-in rule set.
        // Unknown ids were a silent no-op (issue #45) — symmetric with
        // `enable_optional` below, which already rejects them.
        let known_ids: std::collections::HashSet<String> =
            crate::rules::built_in().into_iter().map(|r| r.id).collect();
        for id in &self.redact.disable_default {
            if !known_ids.contains(id) {
                anyhow::bail!("disable_default references unknown rule: {id}");
            }
            engine.remove_rule(id);
        }
        // For each enable_optional, find its definition in built_in() and add.
        let all_built_in = crate::rules::built_in();
        for id in &self.redact.enable_optional {
            if let Some(rule) = all_built_in.iter().find(|r| r.id == *id) {
                if !engine.rule_ids().iter().any(|x| x == id) {
                    engine.add_rule(rule.clone());
                }
            } else {
                anyhow::bail!("enable_optional references unknown rule: {id}");
            }
        }
        for custom in &self.redact.custom {
            let regex = regex::Regex::new(&custom.pattern)?;
            let replacement = custom
                .replacement
                .clone()
                .unwrap_or_else(|| format!("<CUSTOM:{}>", custom.id));
            // P1 #2: SPEC §6.2 — replacement MUST be a typed placeholder of the
            // form `<TYPE>` or `<TYPE:subtype>`, never the original or a hash.
            // Validate at config load so a malicious or mistaken `.taperc`
            // can't bypass the redaction invariant.
            if !is_typed_placeholder(&replacement) {
                anyhow::bail!(
                    "custom rule {:?}: replacement {:?} is not a typed placeholder (expected <TYPE> or <TYPE:subtype>)",
                    custom.id,
                    replacement
                );
            }
            engine.add_rule(crate::Rule {
                id: format!("custom:{}", custom.id),
                regex,
                replacement,
                validator: None,
                default_enabled: true,
                target_capture: None,
            });
        }
        Ok(())
    }
}

/// SPEC §6.2 typed-placeholder check. Accepts `<TYPE>` or `<TYPE:subtype>`
/// where TYPE is uppercase letters/digits/underscore and subtype is
/// alphanumeric / `_` / `-`. Rejects everything else, including bare strings,
/// hashes, and the original secret.
fn is_typed_placeholder(s: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re =
        RE.get_or_init(|| regex::Regex::new(r"^<[A-Z][A-Z0-9_]*(?::[A-Za-z0-9_-]+)?>$").unwrap());
    re.is_match(s)
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

/// Build a redaction engine seeded with default rules, with `.taperc` overlay
/// applied if present. Search order (SPEC §9): walk from `cwd` up to `$HOME`,
/// then `$HOME/.taperc` as fallback. CWD wins — no merge.
///
/// If a `.taperc` is found but fails to read, parse, or apply, the error is
/// returned. The caller MUST abort the recording rather than silently fall
/// back to defaults — otherwise a user's custom redaction rules would be
/// invisibly skipped.
pub fn engine_with_taperc(cwd: &std::path::Path) -> anyhow::Result<crate::Engine> {
    let mut engine = crate::Engine::with_default_rules();
    let path = TapeRcConfig::locate_workspace(cwd).or_else(TapeRcConfig::locate_user);
    if let Some(p) = path {
        let yaml = std::fs::read_to_string(&p)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", p.display()))?;
        let cfg = TapeRcConfig::parse(&yaml)
            .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", p.display()))?;
        cfg.apply(&mut engine)
            .map_err(|e| anyhow::anyhow!("failed to apply {}: {e}", p.display()))?;
    }
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let yaml = r"
redact:
  custom:
    - id: pii_customer
      pattern: 'CUST-\d{6}'
";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert_eq!(cfg.redact.custom.len(), 1);
        assert_eq!(cfg.redact.custom[0].id, "pii_customer");
    }

    #[test]
    fn applies_custom_rule_to_engine() {
        let yaml = r"
redact:
  custom:
    - id: pii_customer
      pattern: 'CUST-\d{6}'
";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        let mut engine = crate::Engine::with_default_rules();
        cfg.apply(&mut engine).unwrap();
        let mut s = "see CUST-447139 for details".to_string();
        let records = engine.redact_string(&mut s);
        assert!(s.contains("<CUSTOM:pii_customer>"), "got: {s}");
        assert!(records.iter().any(|(id, _)| id == "custom:pii_customer"));
    }

    #[test]
    fn enable_optional_activates_ipv4_private() {
        let yaml = r#"
redact:
  enable_optional: ["ipv4_private"]
"#;
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        let mut engine = crate::Engine::with_default_rules();
        cfg.apply(&mut engine).unwrap();
        let mut s = "host 10.0.0.1 here".to_string();
        engine.redact_string(&mut s);
        assert!(s.contains("<IP:private>"), "got: {s}");
    }

    #[test]
    fn rejects_non_typed_placeholder() {
        // P1 #2: SPEC §6.2 forbids replacements that aren't typed placeholders.
        let yaml = r"
redact:
  custom:
    - id: leaky
      pattern: 'CUST-\d{6}'
      replacement: 'literal_secret_value'
";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        let mut engine = crate::Engine::with_default_rules();
        let err = cfg.apply(&mut engine).unwrap_err();
        assert!(
            err.to_string().contains("typed placeholder"),
            "expected typed-placeholder error, got: {err}"
        );
    }

    #[test]
    fn accepts_typed_placeholder_subtype() {
        let yaml = r"
redact:
  custom:
    - id: pii
      pattern: 'CUST-\d{6}'
      replacement: '<CUST_ID:internal>'
";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        let mut engine = crate::Engine::with_default_rules();
        cfg.apply(&mut engine).unwrap(); // no error
    }

    #[test]
    fn rejects_replacement_that_is_a_hash() {
        let yaml = r"
redact:
  custom:
    - id: leaky2
      pattern: 'CUST-\d{6}'
      replacement: 'sha256:abcdef'
";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        let mut engine = crate::Engine::with_default_rules();
        assert!(cfg.apply(&mut engine).is_err());
    }

    #[test]
    fn disable_default_removes_email() {
        let yaml = r#"
redact:
  disable_default: ["email"]
"#;
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        let mut engine = crate::Engine::with_default_rules();
        cfg.apply(&mut engine).unwrap();
        let mut s = "alice@example.com".to_string();
        engine.redact_string(&mut s);
        assert_eq!(s, "alice@example.com", "email should NOT be redacted");
    }

    /// SPEC §9.1: unknown keys under `redact:` MUST cause a config-load
    /// failure. Each entry below is a realistic typo the user might make.
    /// (Issue #36.)
    #[test]
    fn typo_under_redact_rejects() {
        for bad in [
            // wrong key entirely
            "redact:\n  customs:\n    - id: x\n      pattern: 'y'\n",
            // plural / case typos for the documented fields
            "redact:\n  disabled_default: [\"email\"]\n",
            "redact:\n  enable_optionals: [\"ipv4_private\"]\n",
            "redact:\n  enableOptional: [\"ipv4_private\"]\n",
            // entirely made-up section
            "redact:\n  disable: [\"email\"]\n",
        ] {
            let err = TapeRcConfig::parse(bad).err();
            assert!(
                err.is_some(),
                "expected typo to fail config-load; parsed clean: {bad}"
            );
        }
    }

    /// SPEC §9.1: unknown TOP-LEVEL keys are ignored for forward-compat.
    /// Make sure the new `deny_unknown_fields` attribute didn't leak up.
    #[test]
    fn unknown_top_level_key_still_accepted() {
        let yaml = r#"
some_future_section:
  foo: bar
redact:
  disable_default: ["email"]
"#;
        let cfg = TapeRcConfig::parse(yaml).expect("top-level forward-compat");
        assert_eq!(cfg.redact.disable_default, vec!["email"]);
    }

    /// Issue #45: `disable_default` used to silently accept unknown rule
    /// names. Now it rejects them at apply time, matching `enable_optional`.
    #[test]
    fn disable_default_rejects_unknown_rule_name() {
        let yaml = r#"
redact:
  disable_default: ["emial"]
"#;
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        let mut engine = crate::Engine::with_default_rules();
        let err = cfg.apply(&mut engine).unwrap_err();
        assert!(
            err.to_string()
                .contains("disable_default references unknown rule"),
            "expected unknown-rule error; got: {err}"
        );
        // The `email` rule must still be enabled, since the typo'd disable
        // never took effect.
        let mut s = "alice@example.com".to_string();
        engine.redact_string(&mut s);
        assert_eq!(s, "<EMAIL>");
    }

    /// Both list fields should reject unknown ids identically. Symmetric
    /// contract test alongside #36's `typo_under_redact_rejects`.
    #[test]
    fn enable_optional_and_disable_default_have_symmetric_error_shape() {
        let cases = [
            (
                "enable_optional",
                "redact:\n  enable_optional: [\"nope\"]\n",
            ),
            (
                "disable_default",
                "redact:\n  disable_default: [\"nope\"]\n",
            ),
        ];
        for (field, yaml) in cases {
            let cfg = TapeRcConfig::parse(yaml).unwrap();
            let mut engine = crate::Engine::with_default_rules();
            let err = cfg.apply(&mut engine).unwrap_err();
            assert!(
                err.to_string().contains(field) && err.to_string().contains("nope"),
                "{field}: expected error to mention field and id; got: {err}"
            );
        }
    }

    // --- Issue #186: `pricing:` block parse tests ---

    #[test]
    fn pricing_section_with_pricing_file_parses() {
        let yaml = r"
pricing:
  pricing_file: ./prices.toml
";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert_eq!(cfg.pricing.pricing_file.as_deref(), Some("./prices.toml"));
    }

    #[test]
    fn missing_pricing_section_is_default() {
        // No `pricing:` block at all → field is None, redact still parses.
        let yaml = r#"
redact:
  disable_default: ["email"]
"#;
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert!(cfg.pricing.pricing_file.is_none());
        assert_eq!(cfg.redact.disable_default, vec!["email"]);
    }

    // --- Issue #190: `new:` block parse tests ---

    #[test]
    fn new_section_with_default_template_parses() {
        let yaml = r"
new:
  default_template: bug-investigation
";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert_eq!(
            cfg.new.default_template.as_deref(),
            Some("bug-investigation")
        );
    }

    #[test]
    fn missing_new_section_is_default() {
        let yaml = r#"
redact:
  disable_default: ["email"]
"#;
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert!(cfg.new.default_template.is_none());
        assert_eq!(cfg.redact.disable_default, vec!["email"]);
    }

    #[test]
    fn typo_under_new_rejects() {
        // `#[serde(deny_unknown_fields)]` on NewConfig: typos fail
        // config-load so a user notices immediately.
        for bad in [
            "new:\n  default-template: minimal\n",
            "new:\n  template: minimal\n",
            "new:\n  default_templates: minimal\n",
            "new:\n  defaultTemplate: minimal\n",
            "new:\n  template_id: minimal\n",
        ] {
            assert!(
                TapeRcConfig::parse(bad).is_err(),
                "expected typo to fail config-load: {bad}"
            );
        }
    }

    // --- Issue #192: `annotate:` block parse tests ---

    #[test]
    fn annotate_section_with_all_three_fields_parses() {
        let yaml = r"
annotate:
  default_actor: alice
  default_by: human
  editor: nvim
";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert_eq!(cfg.annotate.default_actor.as_deref(), Some("alice"));
        assert_eq!(cfg.annotate.default_by.as_deref(), Some("human"));
        assert_eq!(cfg.annotate.editor.as_deref(), Some("nvim"));
    }

    #[test]
    fn annotate_section_with_partial_fields_parses() {
        let yaml = "annotate:\n  default_actor: alice\n";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert_eq!(cfg.annotate.default_actor.as_deref(), Some("alice"));
        assert!(cfg.annotate.default_by.is_none());
        assert!(cfg.annotate.editor.is_none());
    }

    #[test]
    fn missing_annotate_section_is_default() {
        let yaml = "redact:\n  disable_default: [\"email\"]\n";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert!(cfg.annotate.default_actor.is_none());
        assert!(cfg.annotate.default_by.is_none());
        assert!(cfg.annotate.editor.is_none());
        assert_eq!(cfg.redact.disable_default, vec!["email"]);
    }

    #[test]
    fn typo_under_annotate_rejects() {
        // `#[serde(deny_unknown_fields)]` boundary: every entry here
        // is either a realistic typo or a deferred-field name from
        // #74 §3.11. The future slice that adds default_kind /
        // default_pin / strict_kind extends `AnnotateConfig` and
        // those names start parsing cleanly — until then they fail
        // load (intentional, per the issue body).
        for bad in [
            "annotate:\n  default-actor: alice\n",
            "annotate:\n  defaultActor: alice\n",
            "annotate:\n  default_actors: alice\n",
            "annotate:\n  default_by_kind: human\n",
            "annotate:\n  default_kind: finding\n",
            "annotate:\n  default_pin: false\n",
            "annotate:\n  strict_kind: true\n",
            "annotate:\n  editors: nvim\n",
            "annotate:\n  editor_cmd: nvim\n",
        ] {
            assert!(
                TapeRcConfig::parse(bad).is_err(),
                "expected typo to fail config-load: {bad}"
            );
        }
    }

    // --- Issue #194: `relinernote:` block parse tests ---

    #[test]
    fn relinernote_section_with_default_model_parses() {
        let yaml = "relinernote:\n  default_model: claude-haiku-4-5\n";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert_eq!(
            cfg.relinernote.default_model.as_deref(),
            Some("claude-haiku-4-5")
        );
    }

    #[test]
    fn missing_relinernote_section_is_default() {
        let yaml = "redact:\n  disable_default: [\"email\"]\n";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert!(cfg.relinernote.default_model.is_none());
        assert_eq!(cfg.redact.disable_default, vec!["email"]);
    }

    #[test]
    fn typo_under_relinernote_rejects() {
        // `#[serde(deny_unknown_fields)]` boundary: every entry here
        // is either a realistic typo or a deferred-field name from
        // #71's Phase-2 follow-ons (template-id / temperature /
        // max-tokens / report). The future slice that adds them
        // extends `RelinernoteConfig` and those names start parsing
        // cleanly — until then they fail load (intentional, per the
        // issue body).
        for bad in [
            "relinernote:\n  default-model: claude-haiku-4-5\n",
            "relinernote:\n  defaultModel: claude-haiku-4-5\n",
            "relinernote:\n  model: claude-haiku-4-5\n",
            "relinernote:\n  template_id: default\n",
            "relinernote:\n  default_template_id: default\n",
            "relinernote:\n  default_template: default\n",
            "relinernote:\n  default_temperature: 0.5\n",
            "relinernote:\n  temperature: 0.5\n",
            "relinernote:\n  default_max_tokens: 1024\n",
            "relinernote:\n  max_tokens: 1024\n",
            "relinernote:\n  default_report: ./report.json\n",
            "relinernote:\n  report: ./report.json\n",
            "relinernote:\n  dry_run: true\n",
            "relinernote:\n  default_out: ./out.tape\n",
            "relinernote:\n  out_dir: ./out\n",
        ] {
            assert!(
                TapeRcConfig::parse(bad).is_err(),
                "expected typo to fail config-load: {bad}"
            );
        }
    }

    // --- Issue #198: `recap:` block parse tests ---

    #[test]
    fn recap_section_with_default_model_parses() {
        let yaml = "recap:\n  default_model: claude-haiku-4-5\n";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert_eq!(cfg.recap.default_model.as_deref(), Some("claude-haiku-4-5"));
    }

    #[test]
    fn missing_recap_section_is_default() {
        let yaml = "redact:\n  disable_default: [\"email\"]\n";
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert!(cfg.recap.default_model.is_none());
        assert_eq!(cfg.redact.disable_default, vec!["email"]);
    }

    #[test]
    fn typo_under_recap_rejects() {
        // `#[serde(deny_unknown_fields)]` boundary: realistic typos
        // and deferred-field names from #105's Phase-3+ rollout
        // (template / temperature / max-tokens / report). Future
        // slices that add them extend `RecapConfig` and they start
        // parsing cleanly — until then, fail load.
        for bad in [
            "recap:\n  default-model: claude-haiku-4-5\n",
            "recap:\n  defaultModel: claude-haiku-4-5\n",
            "recap:\n  model: claude-haiku-4-5\n",
            "recap:\n  default_template: short\n",
            "recap:\n  default_template_id: short\n",
            "recap:\n  template_id: short\n",
            "recap:\n  default_temperature: 0.5\n",
            "recap:\n  temperature: 0.5\n",
            "recap:\n  default_max_tokens: 256\n",
            "recap:\n  max_tokens: 256\n",
            "recap:\n  default_report: ./report.json\n",
            "recap:\n  report: ./report.json\n",
            "recap:\n  dry_run: true\n",
            "recap:\n  default_out: ./out.tape\n",
            "recap:\n  out_dir: ./out\n",
        ] {
            assert!(
                TapeRcConfig::parse(bad).is_err(),
                "expected typo to fail config-load: {bad}"
            );
        }
    }

    #[test]
    fn typo_under_pricing_rejects() {
        // `#[serde(deny_unknown_fields)]` on PricingConfig: each of these
        // should fail config-load so a user catches the typo immediately.
        for bad in [
            "pricing:\n  pricing_path: ./prices.toml\n",
            "pricing:\n  file: ./prices.toml\n",
            "pricing:\n  pricingFile: ./prices.toml\n",
            "pricing:\n  pricing_file_path: ./prices.toml\n",
        ] {
            assert!(
                TapeRcConfig::parse(bad).is_err(),
                "expected typo to fail config-load: {bad}"
            );
        }
    }

    /// Regression: a well-formed config with all three documented keys still
    /// parses without complaint.
    #[test]
    fn all_documented_keys_still_parse() {
        let yaml = r#"
redact:
  custom:
    - id: cust_id
      pattern: 'CUST-\d{6}'
      replacement: '<CUST_ID>'
  enable_optional: ["ipv4_private"]
  disable_default: ["email"]
"#;
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert_eq!(cfg.redact.custom.len(), 1);
        assert_eq!(cfg.redact.enable_optional, vec!["ipv4_private"]);
        assert_eq!(cfg.redact.disable_default, vec!["email"]);
    }
}
