//! Read Claude Code's session JSONL transcript and convert it to `tape/v0`
//! tracks. See DECISIONS.md §D2 for why this exists.
//!
//! **Minimum tested Claude Code version:** 2.1.129. The parser is permissive
//! (`serde(deny_unknown_fields = false)`), so newer versions should work as
//! long as the `type` discriminator stays compatible. Unknown event types
//! map to `RawEntry::Skip` and increment a warnings counter.

pub mod convert;
pub mod discovery;
pub mod parser;

pub use convert::{to_tracks, ConvertReport};
pub use discovery::{find_active_session, TranscriptHandle};
pub use parser::{parse_jsonl, ParseReport, RawEntry};
