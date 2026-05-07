//! `liner-notes.md` validation. See SPEC.md §4.

/// The four required H2 sections in order. The validator checks they all
/// appear (any order is technically allowed by §4.1's "in order" SHOULD,
/// but the spec text is "in order" — we enforce order to be strict).
pub const REQUIRED_SECTIONS: &[&str] = &[
    "What I was asked to do",
    "What I found",
    "Suggested next step / fix",
    "What I'm uncertain about",
];

/// Section validation: returns names of sections that are missing OR empty.
/// An empty section is one whose body before the next H2 (or EOF) contains no
/// non-whitespace characters.
pub fn missing_or_empty_sections(content: &str) -> Vec<String> {
    let mut found: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut current: Option<(String, String)> = None;

    for line in content.lines() {
        if let Some(stripped) = line.strip_prefix("## ") {
            if let Some((k, v)) = current.take() {
                found.insert(k, v);
            }
            current = Some((stripped.trim().to_owned(), String::new()));
        } else if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some((k, v)) = current {
        found.insert(k, v);
    }

    REQUIRED_SECTIONS
        .iter()
        .filter(|sect| match found.get(**sect) {
            None => true,
            Some(body) => body.trim().is_empty(),
        })
        .map(|s| (*s).to_owned())
        .collect()
}

/// Order validation: returns true iff the four required sections appear in
/// the canonical order (other H2s between them are allowed but discouraged).
pub fn sections_in_order(content: &str) -> bool {
    let mut req_iter = REQUIRED_SECTIONS.iter().peekable();
    for line in content.lines() {
        if let Some(stripped) = line.strip_prefix("## ") {
            let h2 = stripped.trim();
            if let Some(expected) = req_iter.peek() {
                if h2 == **expected {
                    req_iter.next();
                }
            }
        }
    }
    req_iter.peek().is_none()
}
