//! Proof of a literal selection from the existing Markdown viewer. Source coordinates are
//! half-open UTF-16 units. This is deliberately not the page editor's normalized text.
use crate::DocError;
use comrak::{nodes::NodeValue, parse_document, Arena};

/// Zoen application bounds, independent of Telegram server configuration.
pub const MAX_QUOTE_UTF16: usize = 4096;
pub const MAX_QUOTE_SPANS: usize = 128;

fn invalid() -> DocError {
    DocError("selected text is unavailable for this Markdown range".into())
}

fn byte_at(text: &str, offset: u32) -> Option<usize> {
    let mut units = 0_u32;
    for (byte, c) in text.char_indices() {
        if units == offset {
            return Some(byte);
        }
        units = units.checked_add(u32::try_from(c.len_utf16()).ok()?)?;
        if units > offset {
            return None;
        }
    }
    (units == offset).then_some(text.len())
}

/// Validate real native source runs against Comrak textual leaves. Syntax gaps (bold,
/// emphasis, link delimiters) are allowed; omitted visible text is not. Entities,
/// normalized code, HTML/media and synthetic paragraph/break layout fail closed.
/// # Errors
/// Refuses malformed, unsupported, noncontiguous or mismatching selections.
pub fn validate(md: &str, spans: &[(u32, u32)], excerpt: &str) -> Result<(), DocError> {
    if spans.is_empty()
        || spans.len() > MAX_QUOTE_SPANS
        || excerpt.is_empty()
        || excerpt.encode_utf16().count() > MAX_QUOTE_UTF16
    {
        return Err(invalid());
    }
    let mut ranges = Vec::with_capacity(spans.len());
    let mut previous = 0;
    let mut joined = String::new();
    for &(start, end) in spans {
        if start >= end || start < previous {
            return Err(invalid());
        }
        let a = byte_at(md, start).ok_or_else(invalid)?;
        let b = byte_at(md, end).ok_or_else(invalid)?;
        joined.push_str(md.get(a..b).ok_or_else(invalid)?);
        if joined.len() > MAX_QUOTE_UTF16 * 4 {
            return Err(invalid());
        }
        ranges.push((a, b));
        previous = end;
    }
    if joined != excerpt {
        return Err(invalid());
    }
    let first = ranges[0].0;
    let last = ranges[ranges.len() - 1].1;
    let LiteralRanges { leaves, blocked } = literal_ranges(md);
    if blocked.iter().any(|&(a, b)| a < last && b > first) {
        return Err(invalid());
    }
    // Every selected byte must come from an actual literal leaf; every intervening
    // visible leaf must be fully represented. Absolute ranges disambiguate repetitions.
    for &(a, b) in &ranges {
        if !leaves.iter().any(|&(x, y)| x <= a && b <= y) {
            return Err(invalid());
        }
    }
    for &(a, b) in &leaves {
        let a = a.max(first);
        let b = b.min(last);
        if a < b {
            let covered: usize = ranges
                .iter()
                .map(|&(x, y)| y.min(b).saturating_sub(x.max(a)))
                .sum();
            if covered != b - a {
                return Err(invalid());
            }
        }
    }
    // The viewer synthesizes block separators and normalizes soft breaks; do not
    // pretend those characters have literal source anchors.
    if md
        .get(first..last)
        .is_none_or(|s| s.contains('\n') || s.contains('\r'))
    {
        return Err(invalid());
    }
    Ok(())
}

struct LiteralRanges {
    leaves: Vec<(usize, usize)>,
    blocked: Vec<(usize, usize)>,
}
fn literal_ranges(md: &str) -> LiteralRanges {
    let lines: Vec<_> = std::iter::once(0)
        .chain(md.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let arena = Arena::new();
    let root = parse_document(&arena, md, &crate::markdown::options());
    let mut leaves = Vec::new();
    let mut blocked = Vec::new();
    for node in root.descendants() {
        if node.ancestors().skip(1).any(|parent| {
            matches!(
                parent.data.borrow().value,
                NodeValue::Image(_) | NodeValue::HtmlBlock(_) | NodeValue::HtmlInline(_)
            )
        }) {
            continue;
        }
        let data = node.data.borrow();
        let pos = data.sourcepos;
        let Some(a) = pos
            .start
            .line
            .checked_sub(1)
            .and_then(|i| lines.get(i))
            .and_then(|line| line.checked_add(pos.start.column.checked_sub(1)?))
        else {
            continue;
        };
        let Some(b) = pos
            .end
            .line
            .checked_sub(1)
            .and_then(|i| lines.get(i))
            .and_then(|line| line.checked_add(pos.end.column))
        else {
            continue;
        };
        let candidate = match &data.value {
            NodeValue::Text(text) => Some((a, b, text.as_ref())),
            NodeValue::Code(code) => a
                .checked_add(code.num_backticks)
                .zip(b.checked_sub(code.num_backticks))
                .map(|(a, b)| (a, b, code.literal.as_str())),
            _ => None,
        };
        if matches!(
            data.value,
            NodeValue::Image(_) | NodeValue::HtmlInline(_) | NodeValue::HtmlBlock(_)
        ) {
            blocked.push((a, b));
        }
        if let Some((a, b, text)) = candidate {
            if md.get(a..b) == Some(text) {
                leaves.push((a, b));
            } else {
                blocked.push((a, b));
            }
        }
    }
    LiteralRanges { leaves, blocked }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn units(s: &str) -> u32 {
        s.encode_utf16().fold(0_u32, |n, _| n + 1)
    }
    #[test]
    fn formatted_link_and_repeated_unicode_are_absolute_and_contiguous() -> Result<(), DocError> {
        let md = "😀 one **two** [three](https://private.invalid) 😀 one";
        let ranges = [
            (units("😀 "), units("😀 one ")),
            (units("😀 one **"), units("😀 one **two")),
            (units("😀 one **two**"), units("😀 one **two** ")),
            (units("😀 one **two** ["), units("😀 one **two** [three")),
        ];
        validate(md, &ranges, "one two three")?;
        let start = units("😀 one **two** [three](https://private.invalid) ");
        validate(md, &[(start, units(md))], "😀 one")?;
        assert!(validate(md, &[(0, 1)], "😀").is_err());
        assert!(validate(
            md,
            &[
                (0, 2),
                (units("😀 one **two** ["), units("😀 one **two** [three"))
            ],
            "😀three"
        )
        .is_err());
        assert!(validate(
            md,
            &[(
                units("😀 one **two** [three]("),
                units("😀 one **two** [three](https://private.invalid")
            )],
            "https://private.invalid"
        )
        .is_err());
        Ok(())
    }
    #[test]
    fn normalization_layout_and_markers_are_unavailable() {
        for (md, start, end, text) in [
            ("**bold**", 0, 8, "**bold**"),
            ("a &amp; b", 0, 9, "a &amp; b"),
            ("one\n\ntwo", 0, 3, "wrong"),
            ("` a  b `", 1, 7, " a  b "),
        ] {
            assert!(validate(md, &[(start, end)], text).is_err());
        }
        assert!(validate("one\n\ntwo", &[(0, 3), (5, 8)], "onetwo").is_err());
    }
    #[test]
    fn inline_code_literal_is_supported() -> Result<(), DocError> {
        validate("a `code` b", &[(3, 7)], "code")
    }
    #[test]
    fn explicit_quote_and_span_limits_preserve_valid_boundary() -> Result<(), DocError> {
        let source = "a".repeat(MAX_QUOTE_UTF16);
        validate(&source, &[(0, 4096)], &source)?;
        let too_long = "a".repeat(MAX_QUOTE_UTF16 + 1);
        assert!(validate(&too_long, &[(0, 4097)], &too_long).is_err());
        let source = "a".repeat(MAX_QUOTE_SPANS);
        let spans: Vec<_> = (0_u32..128).map(|i| (i, i + 1)).collect();
        validate(&source, &spans, &source)?;
        let source = "a".repeat(MAX_QUOTE_SPANS + 1);
        let spans: Vec<_> = (0_u32..129).map(|i| (i, i + 1)).collect();
        assert!(validate(&source, &spans, &source).is_err());
        Ok(())
    }
}
