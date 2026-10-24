//! Verify the actual pinned writer's plain paragraph/run grammar, not general WordML.
//! All non-body bytes and namespaces must equal the real --seed document.
use super::{valid_text, INVALID};
const EMPTY: &str = "<w:p></w:p>";
const OPEN: &str = "<w:t xml:space=\"preserve\">";
fn escaped(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '\"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            _ => c.to_string(),
        })
        .collect()
}
pub(super) fn text(bytes: &[u8], seed: &[u8]) -> Result<String, &'static str> {
    let seed = std::str::from_utf8(seed).map_err(|_| INVALID)?;
    let (prefix, suffix) = seed.split_once(EMPTY).ok_or(INVALID)?;
    let xml = std::str::from_utf8(bytes).map_err(|_| INVALID)?;
    let mut body = xml
        .strip_prefix(prefix)
        .and_then(|s| s.strip_suffix(suffix))
        .ok_or(INVALID)?;
    let mut paragraphs = Vec::new();
    let mut total = 0usize;
    while !body.is_empty() {
        let rest = body.strip_prefix("<w:p>").ok_or(INVALID)?;
        let (paragraph, after) = rest.split_once("</w:p>").ok_or(INVALID)?;
        body = after;
        let text = if paragraph.is_empty() {
            String::new()
        } else {
            run(paragraph)?
        };
        total = total
            .checked_add(text.len() + usize::from(!paragraphs.is_empty()))
            .ok_or(INVALID)?;
        if total > super::MAX_TEXT {
            return Err(INVALID);
        }
        paragraphs.push(text);
    }
    if paragraphs.is_empty() {
        return Err(INVALID);
    }
    let text = paragraphs.join("\n");
    if !valid_text(&text) {
        return Err(INVALID);
    }
    Ok(text)
}
fn run(paragraph: &str) -> Result<String, &'static str> {
    let mut rest = paragraph
        .strip_prefix("<w:r>")
        .and_then(|s| s.strip_suffix("</w:r>"))
        .ok_or(INVALID)?;
    if rest.is_empty() {
        return Err(INVALID);
    }
    let mut result = String::new();
    let mut previous_text = false;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<w:tab/>") {
            result.push('\t');
            rest = after;
            previous_text = false;
        } else {
            if previous_text {
                return Err(INVALID);
            }
            let content = rest.strip_prefix(OPEN).ok_or(INVALID)?;
            let (raw, after) = content.split_once("</w:t>").ok_or(INVALID)?;
            let decoded = quick_xml::escape::unescape(raw).map_err(|_| INVALID)?;
            if decoded.is_empty()
                || decoded.contains(['\n', '\t'])
                || !valid_text(&decoded)
                || escaped(&decoded) != raw
            {
                return Err(INVALID);
            }
            result.push_str(&decoded);
            rest = after;
            previous_text = true;
        }
        if result.len() > super::MAX_TEXT {
            return Err(INVALID);
        }
    }
    Ok(result)
}
