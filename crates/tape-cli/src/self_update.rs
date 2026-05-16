//! `tape self-update --check` — Phase 1 of #108 (carved per #234).
//!
//! Read-only version comparison against the GitHub Releases API. No
//! download, no checksum, no rollback — that's Phase 2+. Network
//! failures collapse to `CheckOutcome::Unknown` and exit 0 (the
//! `--check` UX must not break onboarding scripts behind flaky
//! networks).

use std::time::Duration;

pub const GITHUB_API_URL: &str = "https://api.github.com/repos/colinc86/tape/releases/latest";

/// Output format for the check report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Outcome of the version check. Pure data shape so the formatters
/// can be unit-tested without any network at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    UpToDate {
        version: String,
    },
    UpdateAvailable {
        current: String,
        latest: String,
        release_url: String,
    },
    /// Surfaced for any network/parse failure. Exit code stays 0 —
    /// `--check` is informational, must not break onboarding scripts.
    Unknown {
        current: String,
        reason: String,
    },
}

/// Minimal subset of the GitHub Releases API JSON envelope.
#[derive(Debug, serde::Deserialize)]
struct LatestRelease {
    tag_name: String,
    html_url: String,
}

/// Synchronous entry point. Builds a fresh single-threaded tokio
/// runtime to host the reqwest call (the binary's `fn main` is
/// sync — same posture as `cmd_record` / `cmd_changelog` etc.).
/// Returns the process exit code (always 0 in Phase 1).
pub fn check(format: OutputFormat, api_url: &str) -> i32 {
    let current = env!("CARGO_PKG_VERSION").to_owned();
    let outcome = fetch_outcome(api_url, &current);
    let rendered = match format {
        OutputFormat::Text => format_text(&outcome),
        OutputFormat::Json => format_json(&outcome),
    };
    print!("{rendered}");
    0
}

fn fetch_outcome(api_url: &str, current: &str) -> CheckOutcome {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            return CheckOutcome::Unknown {
                current: current.to_owned(),
                reason: format!("tokio runtime: {e}"),
            };
        }
    };
    match rt.block_on(async { fetch_latest(api_url).await }) {
        Ok(release) => {
            let latest = release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name);
            if latest == current {
                CheckOutcome::UpToDate {
                    version: current.to_owned(),
                }
            } else {
                CheckOutcome::UpdateAvailable {
                    current: current.to_owned(),
                    latest: latest.to_owned(),
                    release_url: release.html_url,
                }
            }
        }
        Err(reason) => CheckOutcome::Unknown {
            current: current.to_owned(),
            reason,
        },
    }
}

async fn fetch_latest(api_url: &str) -> std::result::Result<LatestRelease, String> {
    let ua = format!("tape-self-update/{}", env!("CARGO_PKG_VERSION"));
    let client = reqwest::Client::builder()
        .user_agent(ua)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("client build: {e}"))?;
    let resp = client
        .get(api_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    resp.json::<LatestRelease>()
        .await
        .map_err(|e| format!("parse: {e}"))
}

/// Render the four-line text report. Trailing newline included.
pub fn format_text(outcome: &CheckOutcome) -> String {
    match outcome {
        CheckOutcome::UpToDate { version } => format!(
            "current:  {version}\nlatest:   {version}\nstatus:   up-to-date\n"
        ),
        CheckOutcome::UpdateAvailable {
            current,
            latest,
            release_url,
        } => format!(
            "current:  {current}\nlatest:   {latest}\nstatus:   update available\nrelease:  {release_url}\n"
        ),
        CheckOutcome::Unknown { current, reason } => format!(
            "current:  {current}\nlatest:   unknown\nstatus:   unknown ({reason})\n"
        ),
    }
}

/// Render the JSON report (pretty-printed + trailing newline, matching
/// the `tape verify --json` / `tape stats --format json` convention).
pub fn format_json(outcome: &CheckOutcome) -> String {
    let value = match outcome {
        CheckOutcome::UpToDate { version } => serde_json::json!({
            "schema_version": "1.0",
            "current": version,
            "latest": version,
            "status": "up_to_date",
            "release_url": serde_json::Value::Null,
        }),
        CheckOutcome::UpdateAvailable {
            current,
            latest,
            release_url,
        } => serde_json::json!({
            "schema_version": "1.0",
            "current": current,
            "latest": latest,
            "status": "update_available",
            "release_url": release_url,
        }),
        CheckOutcome::Unknown { current, reason } => serde_json::json!({
            "schema_version": "1.0",
            "current": current,
            "latest": serde_json::Value::Null,
            "status": "unknown",
            "release_url": serde_json::Value::Null,
            "error": reason,
        }),
    };
    let mut out = serde_json::to_string_pretty(&value).expect("json render");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_up_to_date() {
        let out = format_text(&CheckOutcome::UpToDate {
            version: "0.2.1".to_owned(),
        });
        assert_eq!(
            out,
            "current:  0.2.1\nlatest:   0.2.1\nstatus:   up-to-date\n"
        );
    }

    #[test]
    fn text_update_available_includes_release_line() {
        let out = format_text(&CheckOutcome::UpdateAvailable {
            current: "0.2.1".to_owned(),
            latest: "0.2.2".to_owned(),
            release_url: "https://github.com/colinc86/tape/releases/tag/v0.2.2".to_owned(),
        });
        assert_eq!(
            out,
            "current:  0.2.1\n\
             latest:   0.2.2\n\
             status:   update available\n\
             release:  https://github.com/colinc86/tape/releases/tag/v0.2.2\n"
        );
    }

    #[test]
    fn text_unknown_carries_reason() {
        let out = format_text(&CheckOutcome::Unknown {
            current: "0.2.1".to_owned(),
            reason: "request: timed out".to_owned(),
        });
        assert!(
            out.contains("status:   unknown (request: timed out)"),
            "{out}"
        );
    }

    #[test]
    fn json_up_to_date_has_schema_version_and_null_release_url() {
        let out = format_json(&CheckOutcome::UpToDate {
            version: "0.2.1".to_owned(),
        });
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["schema_version"], "1.0");
        assert_eq!(v["status"], "up_to_date");
        assert_eq!(v["current"], "0.2.1");
        assert_eq!(v["latest"], "0.2.1");
        assert!(v["release_url"].is_null());
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn json_update_available_has_release_url_string() {
        let out = format_json(&CheckOutcome::UpdateAvailable {
            current: "0.2.1".to_owned(),
            latest: "0.2.2".to_owned(),
            release_url: "https://x".to_owned(),
        });
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["status"], "update_available");
        assert_eq!(v["release_url"], "https://x");
    }

    #[test]
    fn json_unknown_emits_error_field() {
        let out = format_json(&CheckOutcome::Unknown {
            current: "0.2.1".to_owned(),
            reason: "HTTP 503".to_owned(),
        });
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["status"], "unknown");
        assert_eq!(v["error"], "HTTP 503");
        assert!(v["latest"].is_null());
        assert!(v["release_url"].is_null());
    }
}
