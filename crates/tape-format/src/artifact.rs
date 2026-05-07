//! Artifact path layout and reference parsing. See SPEC.md §5.6.

/// Compute the canonical artifact zip-entry path for a given blake3 hex hash.
/// Layout: `artifacts/<aa>/<bb>/<full-hash>.bin` where `aa`/`bb` are the first
/// two and next two hex chars.
pub fn artifact_path(hash_hex: &str) -> String {
    debug_assert!(hash_hex.len() >= 4);
    let aa = &hash_hex[..2];
    let bb = &hash_hex[2..4];
    format!("artifacts/{aa}/{bb}/{hash_hex}.bin")
}

/// A `refs` entry: `"sha:<hex>"`.
pub fn parse_ref(s: &str) -> Option<&str> {
    s.strip_prefix("sha:")
}

/// Compute the blake3 hex digest of a byte slice.
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_path_format() {
        let h = "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
        assert_eq!(
            artifact_path(h),
            "artifacts/aa/bb/aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899.bin"
        );
    }

    #[test]
    fn parse_ref_strips_prefix() {
        assert_eq!(parse_ref("sha:abc"), Some("abc"));
        assert_eq!(parse_ref("md5:abc"), None);
    }

    #[test]
    fn blake3_hex_is_64_chars() {
        let h = blake3_hex(b"hello");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
