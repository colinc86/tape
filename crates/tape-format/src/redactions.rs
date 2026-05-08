//! `redactions.json` schema. See SPEC.md §6.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Redaction {
    pub step: u64,
    pub field_path: String,
    pub rule_id: String,
    pub replacement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_range: Option<[u64; 2]>,
}

pub fn parse(content: &str) -> crate::Result<Vec<Redaction>> {
    let records: Vec<Redaction> = serde_json::from_str(content)?;
    for (i, r) in records.iter().enumerate() {
        if !is_valid_jsonpath(&r.field_path) {
            return Err(crate::Error::Invalid(format!(
                "redactions[{i}].field_path {:?} is not a recognized JSONPath",
                r.field_path
            )));
        }
    }
    Ok(records)
}

pub fn to_json(records: &[Redaction]) -> crate::Result<String> {
    Ok(serde_json::to_string_pretty(records)?)
}

/// Cheap JSONPath syntax check. Accepts a subset that's sufficient for our
/// use: `$`, `$.name`, `$.name.foo`, `$[0]`, `$.foo[3].bar`, `$["weird key"]`.
/// The format isn't strict JSONPath (we don't recognize filters/wildcards
/// because we never produce them); this rejects obviously-broken values.
pub fn is_valid_jsonpath(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes[0] != b'$' {
        return false;
    }
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                i += 1;
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                if i == start {
                    return false; // `..` or `.` at end of string
                }
            }
            b'[' => {
                let close = match s[i..].find(']') {
                    Some(off) => i + off,
                    None => return false,
                };
                let inside = &s[i + 1..close];
                let inside_ok = inside.bytes().all(|b| b.is_ascii_digit())
                    || (inside.starts_with('"') && inside.ends_with('"') && inside.len() >= 2);
                if !inside_ok {
                    return false;
                }
                i = close + 1;
            }
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_jsonpaths() {
        for s in [
            "$",
            "$.foo",
            "$.foo.bar",
            "$.foo[3]",
            "$[0]",
            "$.foo[3].bar",
            r#"$["weird key"]"#,
            "$.tracks[0].payload",
        ] {
            assert!(is_valid_jsonpath(s), "should be valid: {s}");
        }
    }

    #[test]
    fn invalid_jsonpaths() {
        for s in [
            "",
            "foo.bar",
            "$.",
            "$..foo",
            "$[",
            "$.foo[",
            "$.foo[abc]",
            "$.foo[\"unterminated",
        ] {
            assert!(!is_valid_jsonpath(s), "should be invalid: {s}");
        }
    }
}
