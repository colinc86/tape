//! `.taperc` configuration. See SPEC.md §9.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TapeRcConfig {
    #[serde(default)]
    pub redact: RedactConfig,
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
        for id in &self.redact.disable_default {
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
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"^<[A-Z][A-Z0-9_]*(?::[A-Za-z0-9_-]+)?>$").unwrap()
    });
    re.is_match(s)
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let yaml = r#"
redact:
  custom:
    - id: pii_customer
      pattern: 'CUST-\d{6}'
"#;
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        assert_eq!(cfg.redact.custom.len(), 1);
        assert_eq!(cfg.redact.custom[0].id, "pii_customer");
    }

    #[test]
    fn applies_custom_rule_to_engine() {
        let yaml = r#"
redact:
  custom:
    - id: pii_customer
      pattern: 'CUST-\d{6}'
"#;
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
        let yaml = r#"
redact:
  custom:
    - id: leaky
      pattern: 'CUST-\d{6}'
      replacement: 'literal_secret_value'
"#;
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
        let yaml = r#"
redact:
  custom:
    - id: pii
      pattern: 'CUST-\d{6}'
      replacement: '<CUST_ID:internal>'
"#;
        let cfg = TapeRcConfig::parse(yaml).unwrap();
        let mut engine = crate::Engine::with_default_rules();
        cfg.apply(&mut engine).unwrap(); // no error
    }

    #[test]
    fn rejects_replacement_that_is_a_hash() {
        let yaml = r#"
redact:
  custom:
    - id: leaky2
      pattern: 'CUST-\d{6}'
      replacement: 'sha256:abcdef'
"#;
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
