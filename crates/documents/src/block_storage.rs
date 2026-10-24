//! Loro block storage, rich-text marks and public block projections.
use super::*;
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
    m.insert("id", b.id.as_str()).map_err(err)?;
    m.insert("kind", serde_json::to_string(&b.kind).unwrap_or_default())
        .map_err(err)?;
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

pub(super) fn apply_edit(m: &LoroMap, e: &BlockEdit) -> Result<(), DocError> {
    let kind_json = serde_json::to_string(&e.kind).unwrap_or_default();
    if get_str(m, "kind") != kind_json {
        m.insert("kind", kind_json).map_err(err)?;
    }
    let t = match text_of(m) {
        Some(t) => t,
        None => m.insert_container("text", LoroText::new()).map_err(err)?,
    };
    if t.to_string() != e.text {
        t.update(&e.text, Default::default())
            .map_err(|_| DocError("text update timed out".into()))?;
    }
    // Marks: compare per character and fix only the differences.
    let current = read_block(m);
    let have = char_marks(&current.runs);
    let n = e.text.chars().count();
    let mut want: HashMap<&'static str, Vec<Option<String>>> = HashMap::new();
    for (k, _) in MARKS {
        want.insert(k, vec![None; n]);
    }
    for s in &e.spans {
        let a = u16_to_char(&e.text, s.start as usize).min(n);
        let z = u16_to_char(&e.text, s.end as usize).min(n);
        if let Some((k, _)) = MARKS.iter().find(|(k, _)| *k == s.key) {
            let v = if *k == "a" {
                s.value.clone()
            } else {
                "1".into()
            };
            for x in &mut want.get_mut(k).unwrap()[a..z] {
                *x = Some(v.clone());
            }
        }
    }
    for (k, _) in MARKS {
        let h = &have[k];
        let w = &want[k];
        let mut i = 0;
        while i < n {
            if h.get(i) == w.get(i) {
                i += 1;
                continue;
            }
            let start = i;
            let target = w[i].clone();
            while i < n && h.get(i) != w.get(i) && w[i] == target {
                i += 1;
            }
            match &target {
                None => t.unmark(start..i, k).map_err(err)?,
                Some(v) => {
                    let val = if k == "a" {
                        LoroValue::from(v.as_str())
                    } else {
                        LoroValue::Bool(true)
                    };
                    t.mark(start..i, k, val).map_err(err)?
                }
            }
        }
    }
    Ok(())
}

/// UTF-16 spans of a block's runs, for editors.
pub fn spans_of(runs: &[Run]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut pos = 0u32;
    for r in runs {
        let n: u32 = r.text.chars().map(|c| c.len_utf16() as u32).sum();
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
pub fn block_markdown(b: &Block) -> String {
    markdown::inline_md(&b.runs)
}
