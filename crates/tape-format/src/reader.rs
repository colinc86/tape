//! Read a `.tape` (zip) file from disk into memory.

use std::collections::HashMap;
use std::io::{Read, Seek};
use std::path::Path;

use crate::{Error, Result};

/// In-memory representation of a tape, bytes-only. Validation happens
/// elsewhere (see `verify`); this module just performs the IO.
#[derive(Debug)]
pub struct RawTape {
    pub meta_yaml: Option<String>,
    pub liner_md: Option<String>,
    pub tracks_jsonl: Option<String>,
    pub redactions_json: Option<String>,
    /// Map from artifact zip-entry path → bytes.
    pub artifacts: HashMap<String, Vec<u8>>,
    /// Any zip entry not in the recognized set, kept for diagnostics.
    pub unknown_entries: Vec<String>,
}

impl RawTape {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path.as_ref())?;
        Self::from_reader(file)
    }

    pub fn from_reader<R: Read + Seek>(reader: R) -> Result<Self> {
        let mut zip = zip::ZipArchive::new(reader)?;
        let mut out = Self {
            meta_yaml: None,
            liner_md: None,
            tracks_jsonl: None,
            redactions_json: None,
            artifacts: HashMap::new(),
            unknown_entries: Vec::new(),
        };

        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let name = entry.name().to_owned();

            if name.contains("..") || name.starts_with('/') {
                return Err(Error::Invalid(format!("unsafe zip entry path: {name}")));
            }

            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf)?;

            match name.as_str() {
                "meta.yaml" => {
                    out.meta_yaml = Some(String::from_utf8(buf).map_err(|e| {
                        Error::Invalid(format!("meta.yaml not valid UTF-8: {e}"))
                    })?);
                }
                "liner-notes.md" => {
                    out.liner_md = Some(String::from_utf8(buf).map_err(|e| {
                        Error::Invalid(format!("liner-notes.md not valid UTF-8: {e}"))
                    })?);
                }
                "tracks.jsonl" => {
                    out.tracks_jsonl = Some(String::from_utf8(buf).map_err(|e| {
                        Error::Invalid(format!("tracks.jsonl not valid UTF-8: {e}"))
                    })?);
                }
                "redactions.json" => {
                    out.redactions_json = Some(String::from_utf8(buf).map_err(|e| {
                        Error::Invalid(format!("redactions.json not valid UTF-8: {e}"))
                    })?);
                }
                _ if name.starts_with("artifacts/") && name.ends_with(".bin") => {
                    out.artifacts.insert(name, buf);
                }
                _ if name.ends_with('/') => {
                    // directory entry, ignore
                }
                _ => {
                    out.unknown_entries.push(name);
                }
            }
        }

        Ok(out)
    }
}
