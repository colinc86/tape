//! `tape/v0` format implementation: read, write, verify.

pub mod artifact;
pub mod liner;
pub mod meta;
pub mod reader;
pub mod redactions;
pub mod secret_scan;
pub mod tracks;
pub mod verify;
pub mod writer;

pub use meta::{Meta, Outcome, Recorder, RedactionSummary};
pub use tracks::{Kind, Track};
pub use verify::{Diagnostic, DiagnosticCode, VerifyReport};

/// Wire-format version literal that MUST appear in every `meta.yaml`.
pub const TAPE_VERSION: &str = "tape/v0";

/// Maximum size in bytes of any single inline payload field.
/// Larger values must be spilled to artifacts/.
pub const PAYLOAD_INLINE_MAX: usize = 4096;

/// Compression-bomb guard: maximum decompressed-to-compressed ratio.
pub const MAX_DECOMPRESS_RATIO: u64 = 100;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid tape: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
