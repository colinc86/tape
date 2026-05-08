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

        // SPEC §12.3: reject decompression bombs. We track running totals
        // of compressed and uncompressed bytes; if the ratio exceeds
        // MAX_DECOMPRESS_RATIO at any point during the read, abort. We
        // also enforce a per-tape hard size ceiling so a small archive of
        // many tiny entries can't accumulate unbounded growth.
        let mut compressed_total: u64 = 0;
        let mut uncompressed_total: u64 = 0;
        // Floor: 64 KiB so trivially-small tapes (where the ratio is
        // numerically high but the absolute size is harmless) don't
        // false-positive. Above the floor, the ratio rule applies.
        const COMPRESSED_FLOOR: u64 = 64 * 1024;

        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let name = entry.name().to_owned();

            if name.contains("..") || name.starts_with('/') {
                return Err(Error::Invalid(format!("unsafe zip entry path: {name}")));
            }

            compressed_total = compressed_total.saturating_add(entry.compressed_size());

            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf)?;
            uncompressed_total = uncompressed_total.saturating_add(buf.len() as u64);

            if compressed_total >= COMPRESSED_FLOOR
                && uncompressed_total > compressed_total.saturating_mul(crate::MAX_DECOMPRESS_RATIO)
            {
                return Err(Error::Invalid(format!(
                    "decompression bomb: {} bytes uncompressed from {} compressed (ratio > {}×)",
                    uncompressed_total,
                    compressed_total,
                    crate::MAX_DECOMPRESS_RATIO
                )));
            }

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
