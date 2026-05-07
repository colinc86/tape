//! `.taperc` configuration. See SPEC.md §9.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TapeRcConfig {
    #[serde(default)]
    pub redact: RedactConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
            engine.add_rule(crate::Rule {
                id: format!("custom:{}", custom.id),
                regex,
                replacement,
                validator: None,
                default_enabled: true,
            });
        }
        Ok(())
    }
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
}
