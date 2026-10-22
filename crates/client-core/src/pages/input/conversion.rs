//! Complete captured-text conversion using the original input validators and Page parser.

use super::*;

/// Maps the native semantic ranges directly. No before/after text inference occurs.
pub(in crate::pages) fn page_edits(binding: &Binding, capture: &Capture) -> R<Vec<PageBlockDto>> {
    if binding.mode == PageInputMode::Markdown {
        return super::super::parse_editor_markdown(&text(&binding.mode, &capture.payload)?.text);
    }
    if binding.mode == PageInputMode::LosslessText {
        let t = text(&binding.mode, &capture.payload)?;
        let Some(TaggedBlock::Page(base)) = &binding.base_block else {
            return Err(invalid("lossless text requires an exact Page base block"));
        };
        let mut b = base.clone();
        b.text = t.text;
        b.spans = t.spans.unwrap_or_default();
        return Ok(vec![b]);
    }
    let r = rich(&capture.payload)?;
    if r.inline
        .iter()
        .any(|i| ["underline", "spoiler"].contains(&i.kind.as_str()))
    {
        return Err(invalid(
            "this rich mark has no document representation; capture retained",
        ));
    }
    // Rich editing cannot represent the fixed metadata of these existing block kinds.
    if let Some(TaggedBlock::Page(base)) = &binding.base_block {
        if !["paragraph", "heading", "bullet", "numbered"].contains(&base.kind.as_str()) {
            return Err(invalid(
                "rich input cannot replace this block kind losslessly",
            ));
        }
    }
    let mut out = Vec::new();
    let mut emit = |start: u32, end: u32, text: &str, claim: Option<&RichBlock>| -> R<()> {
        let id = if out.is_empty() {
            match &binding.target {
                PageInputTarget::Block { block_id } => block_id.clone(),
                _ => types::new_id("block"),
            }
        } else {
            types::new_id("block")
        };
        let mut b = empty_block(id);
        // Page's text codec uses U+2028 for an in-paragraph line break. Its
        // UTF-16 width matches UIKit's newline, so native span offsets stay exact.
        b.text = text.replace('\n', "\u{2028}");
        if let Some(v) = claim {
            match v.kind {
                0 => {}
                1..=6 => {
                    b.kind = "heading".into();
                    b.level = v.kind;
                }
                7 => {
                    if v.level > 5 {
                        return Err(invalid("unsupported list depth"));
                    }
                    b.kind = "bullet".into();
                    b.indent = v.level;
                }
                8 => {
                    if v.level > 5 {
                        return Err(invalid("unsupported list depth"));
                    }
                    b.kind = "numbered".into();
                    b.indent = v.level;
                    b.number = v.ordinal;
                }
                _ => unreachable!(),
            }
        }
        for i in &r.inline {
            let from = i.start.max(start);
            let stop = i.end.min(end);
            if from < stop {
                let key = match i.kind.as_str() {
                    "strong" => "b",
                    "emphasis" => "i",
                    "strikethrough" => "s",
                    "link" => "a",
                    _ => return Err(invalid("unsupported rich inline mark")),
                };
                b.spans.push(TextSpanDto {
                    start: from - start,
                    end: stop - start,
                    key: key.into(),
                    value: if key == "a" {
                        i.url.clone().unwrap_or_default()
                    } else {
                        String::new()
                    },
                });
            }
        }
        out.push(b);
        Ok(())
    };

    // The native serializer preserves a single newline as a soft break. Empty
    // lines separate paragraphs; headings/list prefixes apply to individual lines.
    // A line-by-line conversion would change one paragraph into several blocks.
    let mut offset = 0u32;
    let mut byte_offset = 0usize;
    let mut paragraph: Option<(u32, u32, usize, usize)> = None;
    for line in r.text.split('\n') {
        let end = offset + line.encode_utf16().count() as u32;
        let byte_end = byte_offset + line.len();
        let claims: Vec<_> = r
            .blocks
            .iter()
            .filter(|v| {
                v.kind != 0
                    && v.start <= offset
                    && v.end >= end
                    && (!line.is_empty() || v.start == offset)
            })
            .collect();
        if claims.len() > 1
            || r.blocks.iter().any(|v| {
                v.kind != 0
                    && v.start < end
                    && v.end > offset
                    && !(v.start <= offset && v.end >= end)
            })
        {
            return Err(invalid("rich block is not line aligned"));
        }
        let claim = claims.first().copied();
        let blank = line.chars().all(|c| matches!(c, ' ' | '\t'));
        if claim.is_some() || blank {
            if let Some((from, to, a, b)) = paragraph.take() {
                emit(from, to, &r.text[a..b], None)?;
            }
            if claim.is_some() {
                emit(offset, end, line, claim)?;
            }
        } else {
            paragraph = Some(match paragraph {
                Some((from, _, a, _)) => (from, end, a, byte_end),
                None => (offset, end, byte_offset, byte_end),
            });
        }
        offset = end + 1;
        byte_offset = byte_end + 1;
    }
    if let Some((from, to, a, b)) = paragraph {
        emit(from, to, &r.text[a..b], None)?;
    }
    Ok(out)
}
