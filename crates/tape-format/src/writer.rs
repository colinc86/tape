//! Write a tape directory to a zip on disk.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

use crate::Result;

/// In-memory tape ready to be zipped.
#[derive(Default)]
pub struct PendingTape {
    pub meta_yaml: String,
    pub liner_md: String,
    pub tracks_jsonl: String,
    pub redactions_json: Option<String>,
    /// Map from artifact zip-entry path → bytes. Use BTreeMap for deterministic order.
    pub artifacts: BTreeMap<String, Vec<u8>>,
}

impl PendingTape {
    /// Write to a path. Performs an atomic rename via a sibling temp file.
    pub fn write_to<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let tmp = tempfile::NamedTempFile::new_in(parent)?;
        {
            let mut zip = zip::ZipWriter::new(tmp.reopen()?);
            let opts = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .unix_permissions(0o644);

            zip.start_file("meta.yaml", opts)?;
            zip.write_all(self.meta_yaml.as_bytes())?;

            zip.start_file("liner-notes.md", opts)?;
            zip.write_all(self.liner_md.as_bytes())?;

            zip.start_file("tracks.jsonl", opts)?;
            zip.write_all(self.tracks_jsonl.as_bytes())?;

            if let Some(r) = &self.redactions_json {
                zip.start_file("redactions.json", opts)?;
                zip.write_all(r.as_bytes())?;
            }

            for (path, bytes) in &self.artifacts {
                zip.start_file(path, opts)?;
                zip.write_all(bytes)?;
            }

            zip.finish()?;
        }
        tmp.persist(path).map_err(|e| crate::Error::Io(e.error))?;
        Ok(())
    }
}
