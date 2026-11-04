//! Exact hashtag matching over the canonical message index, before page limits.
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

fn tag_character(value: char) -> bool {
    value.is_alphanumeric() || value == '_' || is_combining_mark(value)
}

fn boundary(value: char) -> bool {
    value.is_whitespace() || "([{<\"',;!?—–*_~|".contains(value)
}

fn folded(value: &str) -> String {
    value.nfc().flat_map(char::to_lowercase).nfc().collect()
}

/// Only a complete entity opts into exact matching. Other queries retain the
/// existing keyword semantics (for example a pasted Markdown heading).
pub(crate) fn query(value: &str) -> Option<String> {
    let tag = value.trim().strip_prefix('#')?;
    (tag.chars().any(char::is_alphanumeric) && tag.chars().all(tag_character)).then(|| folded(tag))
}

pub(crate) fn contains(text: &str, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let mut previous = None;
    let mut chars = text.char_indices().peekable();
    while let Some((offset, character)) = chars.next() {
        if character == '#' && previous.is_none_or(boundary) {
            let start = offset + 1;
            let mut end = start;
            while let Some(&(next, character)) = chars.peek() {
                if !tag_character(character) {
                    break;
                }
                end = next + character.len_utf8();
                chars.next();
            }
            if end > start && folded(&text[start..end]) == expected {
                return true;
            }
            previous = text[start..end].chars().last().or(Some('#'));
        } else {
            previous = Some(character);
        }
    }
    false
}
