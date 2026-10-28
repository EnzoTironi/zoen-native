//! Loro block storage, rich-text marks and public block projections.
use super::{
    err, markdown, Attrs, Block, BlockEdit, Container, DocError, HashMap, Kind, LoroMap, LoroText,
    LoroValue, Run, Span, ValueOrContainer, MARKS,
};
use loro::TextDelta;

pub(super) fn get_str(m: &LoroMap, key: &str) -> String {
    match m.get(key) {
        Some(ValueOrContainer::Value(LoroValue::String(s))) => s.to_string(),
        _ => String::new(),
    }
}

fn text_of(m: &LoroMap) -> Option<LoroText> {
    match m.get("text") {
        Some(ValueOrContainer::Container(Container::Text(t))) => Some(t),
        _ => None,
    }
}

pub(super) fn write_block(m: &LoroMap, b: &Block, imported: bool) -> Result<(), DocError> {
    let kind = serde_json::to_string(&b.kind).map_err(err)?;
    m.insert("id", b.id.as_str()).map_err(err)?;
    m.insert("kind", kind).map_err(err)?;
    if imported {
        m.insert("src", b.src.as_str()).map_err(err)?;
        m.insert("tail", b.tail.as_str()).map_err(err)?;
        m.insert("lead", b.lead.as_str()).map_err(err)?;
        m.insert("fp", b.fp.as_str()).map_err(err)?;
    }
    let t = m.insert_container("text", LoroText::new()).map_err(err)?;
    // All text first, then marks: inserting after a marked range would extend the mark.
    let all: String = b.runs.iter().map(|r| r.text.as_str()).collect();
    if !all.is_empty() {
        t.insert(0, &all).map_err(err)?;
    }
    let mut pos = 0usize;
    for r in &b.runs {
        let n = r.text.chars().count();
        for (k, v) in attr_marks(&r.attrs) {
            if n > 0 {
                t.mark(pos..pos + n, k, v).map_err(err)?;
            }
        }
        pos += n;
    }
    Ok(())
}

fn attr_marks(a: &Attrs) -> Vec<(&'static str, LoroValue)> {
    let mut v = Vec::new();
    if a.bold {
        v.push(("b", LoroValue::Bool(true)));
    }
    if a.italic {
        v.push(("i", LoroValue::Bool(true)));
    }
    if a.strike {
        v.push(("s", LoroValue::Bool(true)));
    }
    if a.code {
        v.push(("c", LoroValue::Bool(true)));
    }
    if let Some(u) = &a.link {
        v.push(("a", LoroValue::from(u.as_str())));
    }
    if a.hard_break {
        v.push(("hb", LoroValue::Bool(true)));
    }
    v
}

pub(super) fn read_block(m: &LoroMap) -> Block {
    let kind = serde_json::from_str(&get_str(m, "kind")).unwrap_or(Kind::Paragraph);
    let mut runs = Vec::new();
    if let Some(t) = text_of(m) {
        for d in t.to_delta() {
            if let TextDelta::Insert { insert, attributes } = d {
                let mut a = Attrs::default();
                if let Some(attrs) = attributes {
                    for (k, v) in attrs {
                        let on = !matches!(v, LoroValue::Null | LoroValue::Bool(false));
                        match k.as_str() {
                            "b" => a.bold = on,
                            "i" => a.italic = on,
                            "s" => a.strike = on,
                            "c" => a.code = on,
                            "hb" => a.hard_break = on,
                            "a" => {
                                if let LoroValue::String(u) = v {
                                    a.link = Some(u.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                runs.push(Run {
                    text: insert,
                    attrs: a,
                });
            }
        }
    }
    Block {
        id: get_str(m, "id"),
        kind,
        runs,
        src: get_str(m, "src"),
        tail: get_str(m, "tail"),
        lead: get_str(m, "lead"),
        fp: get_str(m, "fp"),
    }
}

/// UTF-16 offset → Unicode scalar offset in `s`.
pub(super) fn u16_to_char(s: &str, u16: usize) -> usize {
    let mut n16 = 0;
    for (i, c) in s.chars().enumerate() {
        if n16 >= u16 {
            return i;
        }
        n16 += c.len_utf16();
    }
    s.chars().count()
}

/// Per-character marks of a block, from runs (`Some(value)` when set).
pub(super) fn char_marks(runs: &[Run]) -> HashMap<&'static str, Vec<Option<String>>> {
    let total: usize = runs.iter().map(|r| r.text.chars().count()).sum();
    let mut out: HashMap<&'static str, Vec<Option<String>>> = HashMap::new();
    for (k, _) in MARKS {
        out.insert(k, vec![None; total]);
    }
    let mut pos = 0;
    for r in runs {
        let n = r.text.chars().count();
        for (k, v) in attr_marks(&r.attrs) {
            let val = match v {
                LoroValue::String(s) => s.to_string(),
                _ => "1".into(),
            };
            if let Some(slot) = out.get_mut(k) {
                for x in &mut slot[pos..pos + n] {
                    *x = Some(val.clone());
                }
            }
        }
        pos += n;
    }
    out
}

pub(super) fn apply_edit(map: &LoroMap, edit: &BlockEdit) -> Result<(), DocError> {
    let kind_json = serde_json::to_string(&edit.kind).map_err(err)?;
    if get_str(map, "kind") != kind_json {
        map.insert("kind", kind_json).map_err(err)?;
    }
    let text = match text_of(map) {
        Some(text) => text,
        None => map.insert_container("text", LoroText::new()).map_err(err)?,
    };
    if text.to_string() != edit.text {
        text.update(&edit.text, loro::UpdateOptions::default())
            .map_err(|_| DocError("text update timed out".into()))?;
    }
    // Marks: compare per character and fix only the differences.
    let current = read_block(map);
    let have = char_marks(&current.runs);
    let char_count = edit.text.chars().count();
    let mut want: HashMap<&'static str, Vec<Option<String>>> = HashMap::new();
    for (key, _) in MARKS {
        want.insert(key, vec![None; char_count]);
    }
    for span in &edit.spans {
        let start = u16_to_char(&edit.text, span.start as usize).min(char_count);
        let end = u16_to_char(&edit.text, span.end as usize).min(char_count);
        if let Some(slot) = want.get_mut(span.key.as_str()) {
            let value = if span.key == "a" {
                span.value.clone()
            } else {
                "1".into()
            };
            for mark in &mut slot[start..end] {
                *mark = Some(value.clone());
            }
        }
    }
    for (key, _) in MARKS {
        let current_marks = &have[key];
        let wanted_marks = &want[key];
        let mut index = 0;
        while index < char_count {
            if current_marks.get(index) == wanted_marks.get(index) {
                index += 1;
                continue;
            }
            let start = index;
            let target = wanted_marks[index].clone();
            while index < char_count
                && current_marks.get(index) != wanted_marks.get(index)
                && wanted_marks[index] == target
            {
                index += 1;
            }
            match &target {
                None => text.unmark(start..index, key).map_err(err)?,
                Some(value) => {
                    let val = if key == "a" {
                        LoroValue::from(value.as_str())
                    } else {
                        LoroValue::Bool(true)
                    };
                    text.mark(start..index, key, val).map_err(err)?;
                }
            }
        }
    }
    Ok(())
}

/// UTF-16 spans of a block's runs, for editors.
#[must_use]
pub fn spans_of(runs: &[Run]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut pos = 0u32;
    for r in runs {
        let n: u32 = r.text.encode_utf16().fold(0_u32, |units, _| units + 1);
        for (k, v) in attr_marks(&r.attrs) {
            out.push(Span {
                start: pos,
                end: pos + n,
                key: k.to_string(),
                value: match v {
                    LoroValue::String(s) => s.to_string(),
                    _ => String::new(),
                },
            });
        }
        pos += n;
    }
    out
}

/// Markdown for one block's text in house style (used for "Editar como texto").
#[must_use]
pub fn block_markdown(b: &Block) -> String {
    markdown::inline_md(&b.runs)
}
