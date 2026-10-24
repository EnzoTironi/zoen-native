//! Markdown in and out (ADR 0040 §3).
//!
//! Import splits the source into blocks by line. Every byte of the source belongs to exactly
//! one block (its `src` plus the blank lines after it, `tail`), or to the page's leading
//! blank lines. Export writes an untouched block's `src` back verbatim, so a page nobody
//! edited exports byte-identical. An edited block is written in a fixed house style that
//! keeps its original list marker, heading style, quote prefix and code fence.

use comrak::nodes::{AstNode, ListType, NodeValue};
use comrak::{parse_document, Arena, Options};

use crate::{Attrs, Block, Kind, Run};

mod exporting;
pub(crate) use exporting::{export, inline_md};

pub(crate) fn options() -> Options<'static> {
    let mut o = Options::default();
    o.extension.strikethrough = true;
    o.extension.table = true;
    o.extension.tasklist = true;
    o.extension.autolink = true;
    o.extension.footnotes = true;
    o.extension.front_matter_delimiter = Some("---".into());
    o
}

/// A block found in the source, before slicing.
struct Unit {
    /// 1-based first and last line of the block's own content.
    start: usize,
    end: usize,
    kind: Kind,
    runs: Vec<Run>,
    /// Bytes on the first line before the content (list marker, `## `, `> `).
    lead: String,
}

pub(crate) struct Imported {
    pub lead: String,
    pub blocks: Vec<Block>,
}

pub(crate) fn import(md: &str) -> Imported {
    let arena = Arena::new();
    let root = parse_document(&arena, md, &options());
    let lines: Vec<&str> = md.split_inclusive('\n').collect();
    let mut units = Vec::new();
    for child in root.children() {
        top_level(child, &lines, &mut units, 0);
    }
    units.sort_by_key(|u| u.start);
    // Drop units that overlap an earlier one (defensive: the slicing must partition).
    let mut clean: Vec<Unit> = Vec::new();
    for u in units {
        if clean.last().is_some_and(|p| u.start <= p.end) {
            continue;
        }
        clean.push(u);
    }
    let units = clean;

    let n = lines.len();
    let mut out = Vec::new();
    // Lines before the first block: blank ones become the page's lead, others a raw block.
    let first = units.first().map(|u| u.start).unwrap_or(n + 1);
    let mut page_lead = String::new();
    let mut raw_before = Vec::new();
    {
        let mut i = 1;
        while i < first && is_blank(lines[i - 1]) {
            page_lead.push_str(lines[i - 1]);
            i += 1;
        }
        if i < first {
            raw_before.push(raw_block(&lines, i, first - 1));
        }
    }
    out.extend(raw_before);
    for (idx, u) in units.iter().enumerate() {
        let next = units.get(idx + 1).map(|v| v.start).unwrap_or(n + 1);
        let end = u.end.min(next - 1).max(u.start);
        let src: String = lines[u.start - 1..end].concat();
        // After the block: blank lines are its tail; anything else (link reference
        // definitions, which the parser keeps out of the tree) becomes a raw block.
        let mut j = end + 1;
        let mut tail = String::new();
        while j < next && is_blank(lines[j - 1]) {
            tail.push_str(lines[j - 1]);
            j += 1;
        }
        let mut b = Block::new(u.kind.clone(), u.runs.clone());
        b.lead = u.lead.clone();
        b.src = src;
        b.tail = tail;
        b.fp = b.fingerprint();
        out.push(b);
        if j < next {
            let r = raw_block(&lines, j, next - 1);
            out.push(r);
        }
    }
    Imported {
        lead: page_lead,
        blocks: out,
    }
}

fn is_blank(l: &str) -> bool {
    l.trim().is_empty()
}

fn raw_block(lines: &[&str], from: usize, mut to: usize) -> Block {
    let mut tail = String::new();
    let mut tail_lines = Vec::new();
    while to > from && is_blank(lines[to - 1]) {
        tail_lines.push(lines[to - 1]);
        to -= 1;
    }
    for l in tail_lines.iter().rev() {
        tail.push_str(l);
    }
    let src: String = lines[from - 1..to].concat();
    let mut b = Block::new(Kind::Raw, vec![Run::plain(&block_text(&src))]);
    b.src = src;
    b.tail = tail;
    b.fp = b.fingerprint();
    b
}

/// Source text as a block's text: line breaks inside a block are U+2028, so one block is
/// always one paragraph in the editor.
pub fn block_text(src: &str) -> String {
    src.strip_suffix('\n')
        .unwrap_or(src)
        .replace("\r\n", "\u{2028}")
        .replace('\n', "\u{2028}")
}

fn sp(node: &AstNode) -> (usize, usize, usize) {
    let d = node.data.borrow();
    (
        d.sourcepos.start.line,
        d.sourcepos.end.line,
        d.sourcepos.start.column,
    )
}

fn raw_unit(node: &AstNode, lines: &[&str]) -> Unit {
    let (s, e, _) = sp(node);
    let e = e.max(s).min(lines.len());
    Unit {
        start: s,
        end: e,
        kind: Kind::Raw,
        runs: vec![Run::plain(&block_text(&lines[s - 1..e].concat()))],
        lead: String::new(),
    }
}

/// The bytes of `line` before 1-based byte column `col`.
fn lead_of(lines: &[&str], line: usize, col: usize) -> String {
    let l = lines.get(line - 1).copied().unwrap_or("");
    let cut = col.saturating_sub(1).min(l.len());
    if l.is_char_boundary(cut) {
        l[..cut].to_string()
    } else {
        String::new()
    }
}

fn top_level<'a>(node: &'a AstNode<'a>, lines: &[&str], out: &mut Vec<Unit>, indent: u32) {
    let value = node.data.borrow().value.clone();
    let (s, e, _) = sp(node);
    if s == 0 {
        return;
    }
    match value {
        NodeValue::Paragraph => {
            // A paragraph that is only an image is an image block.
            let kids: Vec<_> = node.children().collect();
            if kids.len() == 1 {
                if let NodeValue::Image(link) = &kids[0].data.borrow().value {
                    if let Some(alt) = plain_text(kids[0]) {
                        out.push(Unit {
                            start: s,
                            end: e,
                            kind: Kind::Image {
                                url: link.url.clone(),
                                alt,
                                title: link.title.clone(),
                            },
                            runs: vec![],
                            lead: String::new(),
                        });
                        return;
                    }
                }
            }
            match inlines(node) {
                Some(runs) => out.push(Unit {
                    start: s,
                    end: e,
                    kind: Kind::Paragraph,
                    runs,
                    lead: String::new(),
                }),
                None => out.push(raw_unit(node, lines)),
            }
        }
        NodeValue::Heading(h) => match inlines(node) {
            Some(runs) => {
                let lead = if h.setext {
                    String::new()
                } else {
                    // `## ` up to the first content byte.
                    let l = lines.get(s - 1).copied().unwrap_or("");
                    let hashes = l.trim_start().chars().take_while(|c| *c == '#').count();
                    let pre = l.len() - l.trim_start().len();
                    let after = l[pre + hashes..]
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .map(char::len_utf8)
                        .sum::<usize>();
                    l[..pre + hashes + after].to_string()
                };
                out.push(Unit {
                    start: s,
                    end: e,
                    kind: Kind::Heading {
                        level: h.level as u32,
                        setext: h.setext,
                    },
                    runs,
                    lead,
                });
            }
            None => out.push(raw_unit(node, lines)),
        },
        NodeValue::BlockQuote => {
            let kids: Vec<_> = node.children().collect();
            let simple =
                kids.len() == 1 && matches!(kids[0].data.borrow().value, NodeValue::Paragraph);
            match (simple, simple.then(|| inlines(kids[0])).flatten()) {
                (true, Some(runs)) => {
                    let (ps, _, col) = sp(kids[0]);
                    let lead = lead_of(lines, ps, col);
                    let lead = if lead.trim_start().starts_with('>') {
                        lead
                    } else {
                        "> ".into()
                    };
                    out.push(Unit {
                        start: s,
                        end: e,
                        kind: Kind::Quote,
                        runs,
                        lead,
                    })
                }
                _ => out.push(raw_unit(node, lines)),
            }
        }
        NodeValue::List(list) => {
            for (number, item) in (list.start as u32..).zip(node.children()) {
                list_item(item, lines, out, indent, list.list_type, number);
            }
        }
        NodeValue::CodeBlock(cb) => {
            let lead = if cb.fenced {
                lines.get(s - 1).copied().unwrap_or("").to_string()
            } else {
                String::new()
            };
            out.push(Unit {
                start: s,
                end: e,
                kind: Kind::Code {
                    lang: cb.info.split_whitespace().next().unwrap_or("").to_string(),
                },
                runs: vec![Run::plain(&block_text(&cb.literal))],
                lead,
            });
        }
        NodeValue::ThematicBreak => out.push(Unit {
            start: s,
            end: e,
            kind: Kind::Divider,
            runs: vec![],
            lead: String::new(),
        }),
        _ => out.push(raw_unit(node, lines)),
    }
}

fn list_item<'a>(
    item: &'a AstNode<'a>,
    lines: &[&str],
    out: &mut Vec<Unit>,
    indent: u32,
    list_type: ListType,
    number: u32,
) {
    let value = item.data.borrow().value.clone();
    let (s, e, _) = sp(item);
    let checked = match &value {
        NodeValue::TaskItem(t) => Some(t.symbol.is_some()),
        NodeValue::Item(_) => None,
        _ => {
            out.push(raw_unit(item, lines));
            return;
        }
    };
    let kids: Vec<_> = item.children().collect();
    // Supported: an optional first paragraph, then only nested lists.
    let (first, rest) = match kids.first() {
        Some(k) if matches!(k.data.borrow().value, NodeValue::Paragraph) => (Some(*k), &kids[1..]),
        _ => (None, &kids[..]),
    };
    let ok_rest = rest
        .iter()
        .all(|k| matches!(k.data.borrow().value, NodeValue::List(_)));
    let runs = match first {
        Some(p) => inlines(p),
        None => Some(vec![]),
    };
    let (Some(runs), true) = (runs, ok_rest) else {
        let mut u = raw_unit(item, lines);
        u.end = e.max(s);
        out.push(u);
        return;
    };
    let (own_end, lead) = match first {
        Some(p) => {
            let (_, pe, pcol) = sp(p);
            (pe, lead_of(lines, s, pcol))
        }
        None => (
            s,
            lines
                .get(s - 1)
                .copied()
                .unwrap_or("")
                .trim_end_matches(['\n', '\r'])
                .to_string(),
        ),
    };
    let kind = match checked {
        Some(c) => Kind::Task { indent, checked: c },
        None if list_type == ListType::Ordered => Kind::Numbered { indent, number },
        None => Kind::Bullet { indent },
    };
    out.push(Unit {
        start: s,
        end: own_end.max(s),
        kind,
        runs,
        lead,
    });
    for k in rest {
        top_level(k, lines, out, indent + 1);
    }
}

/// The text of an image's alt, or `None` if it has formatting we would lose.
fn plain_text<'a>(node: &'a AstNode<'a>) -> Option<String> {
    let mut s = String::new();
    for c in node.children() {
        match &c.data.borrow().value {
            NodeValue::Text(t) => s.push_str(t),
            _ => return None,
        }
    }
    Some(s)
}

/// A block's inline content as runs, or `None` if it holds something we can't edit
/// faithfully (inline HTML, images inside text, footnote references, math…).
fn inlines<'a>(node: &'a AstNode<'a>) -> Option<Vec<Run>> {
    let mut out = Vec::new();
    walk(node, &Attrs::default(), &mut out)?;
    Some(out)
}

fn walk<'a>(node: &'a AstNode<'a>, attrs: &Attrs, out: &mut Vec<Run>) -> Option<()> {
    for c in node.children() {
        let v = c.data.borrow().value.clone();
        match v {
            NodeValue::Text(t) => out.push(Run {
                text: t.to_string(),
                attrs: attrs.clone(),
            }),
            NodeValue::SoftBreak => out.push(Run {
                text: "\u{2028}".into(),
                attrs: attrs.clone(),
            }),
            NodeValue::LineBreak => out.push(Run {
                text: "\u{2028}".into(),
                attrs: Attrs {
                    hard_break: true,
                    ..attrs.clone()
                },
            }),
            NodeValue::Code(code) => out.push(Run {
                text: code.literal.clone(),
                attrs: Attrs {
                    code: true,
                    ..attrs.clone()
                },
            }),
            NodeValue::Emph => walk(
                c,
                &Attrs {
                    italic: true,
                    ..attrs.clone()
                },
                out,
            )?,
            NodeValue::Strong => walk(
                c,
                &Attrs {
                    bold: true,
                    ..attrs.clone()
                },
                out,
            )?,
            NodeValue::Strikethrough => walk(
                c,
                &Attrs {
                    strike: true,
                    ..attrs.clone()
                },
                out,
            )?,
            NodeValue::Link(l)
                if l.title.is_empty() && attrs.link.is_none() && c.children().next().is_some() =>
            {
                walk(
                    c,
                    &Attrs {
                        link: Some(l.url.clone()),
                        ..attrs.clone()
                    },
                    out,
                )?
            }
            _ => return None,
        }
    }
    Some(())
}
