//! In-flight recording session. Events accumulate here; eject reads them.

use std::sync::Arc;
use std::sync::Mutex;

use serde_json::Value;
use tape_format::tracks::{Annotation, Kind, Track};

#[derive(Debug, Clone)]
pub struct Session {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug)]
struct Inner {
    /// `meta.task` — the headline of what was asked.
    task: String,
    /// Recorder agent string for `meta.recorder.agent`.
    recorder_agent: String,
    /// Tracks accumulated so far. `step` is assigned at append time.
    tracks: Vec<Track>,
    /// `meta.created_at`.
    created_at: chrono::DateTime<chrono::Utc>,
}

impl Session {
    /// Start a session at the current wall clock. Emits a `task` event as step 1.
    pub fn start(task: impl Into<String>, recorder_agent: impl Into<String>) -> Self {
        Self::start_at(task, recorder_agent, chrono::Utc::now())
    }

    /// Start a session at an explicit timestamp. Used by `tape.snapshot` to
    /// align `meta.created_at` with the first user prompt's actual time
    /// rather than "now"; otherwise replaying an older transcript would
    /// produce a tape whose meta.created_at is in the future relative to
    /// its first track.
    pub fn start_at(
        task: impl Into<String>,
        recorder_agent: impl Into<String>,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let task_text = task.into();
        let task_event = Track {
            step: 1,
            kind: Kind::Task,
            ts: format_ts(started_at),
            payload: serde_json::json!({"prompt": task_text}),
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        };
        Self {
            inner: Arc::new(Mutex::new(Inner {
                task: task_text,
                recorder_agent: recorder_agent.into(),
                tracks: vec![task_event],
                created_at: started_at,
            })),
        }
    }

    /// Append an event to the session. `step` is assigned automatically.
    /// Returns the assigned step number.
    pub fn append(&self, kind: Kind, payload: Value) -> u64 {
        self.append_at(kind, payload, chrono::Utc::now())
    }

    /// Append an event at an explicit timestamp.
    ///
    /// Used by replay paths (`tape.snapshot`, future transcript-import) where
    /// the event's real timestamp is already known from the source data and
    /// overriding it with `Utc::now()` would collapse the entire conversation
    /// into a single instant. (Issue #5.)
    pub fn append_at(&self, kind: Kind, payload: Value, ts: chrono::DateTime<chrono::Utc>) -> u64 {
        let mut g = self.inner.lock().expect("session mutex poisoned");
        let step = (g.tracks.len() as u64) + 1;
        g.tracks.push(Track {
            step,
            kind,
            ts: format_ts(ts),
            payload,
            parent_step: None,
            refs: vec![],
            annotations: vec![],
        });
        step
    }

    /// Replay-path entry point. Append a fully-formed `Track`, preserving
    /// every field on it (`kind`, `ts`, `payload`, `parent_step`, `refs`,
    /// `annotations`) and only reassigning `step` to match the session's
    /// current position.
    ///
    /// Used by `tape.eject` and `tape.snapshot` when copying loaded/converted
    /// tracks into a fresh `Session`. The live-recording paths (`append` /
    /// `append_at`) only have a kind/payload/ts at the call site and would
    /// strand `parent_step` / `refs` / `annotations` as their defaults, which
    /// silently drops e.g. `refs` -> orphan artifacts on round-trip (issue
    /// #49) and `parent_step` -> lost annotation linkage. Use this method
    /// whenever the caller already has a `Track`.
    pub fn append_track(&self, mut track: Track) -> u64 {
        let mut g = self.inner.lock().expect("session mutex poisoned");
        let step = (g.tracks.len() as u64) + 1;
        track.step = step;
        g.tracks.push(track);
        step
    }

    /// Append an annotation. Convenience over `append`.
    pub fn annotate(&self, by: &str, note: impl Into<String>) -> u64 {
        self.append(
            Kind::Annotation,
            serde_json::json!({"by": by, "note": note.into()}),
        )
    }

    /// Snapshot the current track list. Cheap clone.
    pub fn snapshot(&self) -> SessionSnapshot {
        let g = self.inner.lock().expect("session mutex poisoned");
        SessionSnapshot {
            task: g.task.clone(),
            recorder_agent: g.recorder_agent.clone(),
            created_at: g.created_at,
            tracks: g.tracks.clone(),
        }
    }

    /// Number of tracks currently recorded.
    pub fn track_count(&self) -> usize {
        self.inner
            .lock()
            .expect("session mutex poisoned")
            .tracks
            .len()
    }
}

#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub task: String,
    pub recorder_agent: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub tracks: Vec<Track>,
}

pub fn format_ts(t: chrono::DateTime<chrono::Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

#[allow(dead_code)] // reserved for future use; suppressed until consumed
fn _annotation_unused() -> Annotation {
    Annotation {
        by: String::new(),
        note: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_starts_with_task_event() {
        let s = Session::start("hello", "test/0.0.1");
        let snap = s.snapshot();
        assert_eq!(snap.tracks.len(), 1);
        assert_eq!(snap.tracks[0].kind, Kind::Task);
        assert_eq!(snap.tracks[0].step, 1);
    }

    #[test]
    fn session_appends_steps_monotonically() {
        let s = Session::start("hello", "test/0.0.1");
        let a = s.append(
            Kind::ModelCall,
            serde_json::json!({"vendor": "anthropic", "model": "x"}),
        );
        let b = s.append(
            Kind::ModelCall,
            serde_json::json!({"vendor": "anthropic", "model": "x"}),
        );
        assert_eq!(a, 2);
        assert_eq!(b, 3);
        assert_eq!(s.track_count(), 3);
    }

    /// `append_at` must preserve the caller-supplied timestamp verbatim — not
    /// substitute "now". Regression test for issue #5.
    #[test]
    fn append_at_preserves_caller_supplied_ts() {
        let start = chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let later = chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:06Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let s = Session::start_at("hi", "test/0.0.1", start);
        s.append_at(
            Kind::ModelCall,
            serde_json::json!({"vendor": "x", "model": "x"}),
            later,
        );

        let snap = s.snapshot();
        assert_eq!(snap.tracks[0].ts, format_ts(start));
        assert_eq!(snap.tracks[1].ts, format_ts(later));
    }
}
