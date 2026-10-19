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

// ───────────────────────────── export ─────────────────────────────

pub(crate) fn export(lead: &str, blocks: &[Block], canonical: bool) -> String {
    let mut s = String::from(if canonical { "" } else { lead });
    for (i, b) in blocks.iter().enumerate() {
        // A block that wasn't in the source next to its neighbour (a new one, or one after
        // a new one) needs a blank line, or it would run into the block above (a paragraph
        // after a quote becomes part of the quote). List items of one list sit together.
        if i > 0 && !s.is_empty() && !s.ends_with("\n\n") {
            let prev = &blocks[i - 1];
            let adjacent_in_source = !b.src.is_empty() && !prev.src.is_empty();
            if !(adjacent_in_source || prev.kind.is_list_item() && b.kind.is_list_item()) {
                s.push('\n');
            }
        }
        if !canonical && !b.src.is_empty() && b.fingerprint() == b.fp {
            s.push_str(&b.src);
            s.push_str(&b.tail);
            continue;
        }
        let body = render(b);
        s.push_str(&body);
        if !body.ends_with('\n') {
            s.push('\n');
        }
        if !canonical && !b.src.is_empty() {
            s.push_str(&b.tail);
        } else if let Some(next) = blocks.get(i + 1) {
            // New blocks: list items of the same list sit together, everything else gets
            // a blank line.
            if !(b.kind.is_list_item() && next.kind.is_list_item()) {
                s.push('\n');
            }
        }
    }
    s
}

fn bullet_prefix(kind: &Kind) -> String {
    match kind {
        Kind::Bullet { indent } => format!("{}- ", "  ".repeat(*indent as usize)),
        Kind::Numbered { indent, number } => {
            format!("{}{}. ", "   ".repeat(*indent as usize), number)
        }
        Kind::Task { indent, checked } => format!(
            "{}- [{}] ",
            "  ".repeat(*indent as usize),
            if *checked { "x" } else { " " }
        ),
        _ => String::new(),
    }
}

/// The lead to write for an edited list item: the original one if it still describes the
/// same kind of item (with the checkbox updated), else a fresh one.
fn item_lead(b: &Block) -> String {
    let fresh = bullet_prefix(&b.kind);
    if b.lead.is_empty() {
        return fresh;
    }
    let orig = crate::Block::kind_from_fp(&b.fp);
    let same_shape = match (&orig, &b.kind) {
        (Some(Kind::Task { indent: a, .. }), Kind::Task { indent: c, .. }) => a == c,
        (Some(Kind::Bullet { indent: a }), Kind::Bullet { indent: c }) => a == c,
        (
            Some(Kind::Numbered {
                indent: a,
                number: n,
            }),
            Kind::Numbered {
                indent: c,
                number: m,
            },
        ) => a == c && n == m,
        _ => false,
    };
    if !same_shape {
        return fresh;
    }
    let mut lead = b.lead.trim_end_matches(['\n', '\r']).to_string();
    if !lead.ends_with(' ') && !lead.ends_with('\t') {
        lead.push(' ');
    }
    if let Kind::Task { checked, .. } = b.kind {
        if let (Some(open), Some(close)) = (lead.find('['), lead.find(']')) {
            if close == open + 2 {
                lead.replace_range(open + 1..close, if checked { "x" } else { " " });
            }
        }
    }
    lead
}

fn render(b: &Block) -> String {
    let inline = inline_md(&b.runs);
    match &b.kind {
        Kind::Paragraph => continuation(&inline, ""),
        Kind::Heading { level, .. } => {
            let lead = if !b.lead.is_empty()
                && matches!(crate::Block::kind_from_fp(&b.fp), Some(Kind::Heading { level: l, .. }) if l == *level)
            {
                b.lead.clone()
            } else {
                format!("{} ", "#".repeat((*level).clamp(1, 6) as usize))
            };
            let mut text = inline.clone();
            if text.ends_with('#') {
                text.insert(text.len() - 1, '\\');
            }
            if text.contains('\n') && *level <= 2 {
                // A heading over several lines can only be written setext-style.
                let under = if *level == 1 { "===" } else { "---" };
                return format!("{text}\n{under}\n");
            }
            format!("{lead}{}\n", text.replace('\n', " "))
        }
        Kind::Bullet { .. } | Kind::Numbered { .. } | Kind::Task { .. } => {
            let lead = item_lead(b);
            let pad = " ".repeat(lead.chars().count());
            let body = continuation(&inline, &pad);
            format!("{lead}{}", body.trim_start_matches(&pad as &str))
        }
        Kind::Quote => {
            let lead = if b.lead.trim_start().starts_with('>') {
                b.lead.clone()
            } else {
                "> ".into()
            };
            let mut s = String::new();
            for (i, line) in inline.split('\n').enumerate() {
                if i == 0 {
                    s.push_str(&lead);
                } else {
                    s.push_str(lead.trim_end());
                    s.push(' ');
                }
                s.push_str(line);
                s.push('\n');
            }
            s
        }
        Kind::Code { lang } => {
            let code: String = b
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>()
                .replace('\u{2028}', "\n");
            let longest = longest_run(&code, '`');
            let (open, close) = if b.lead.trim_start().starts_with("```")
                || b.lead.trim_start().starts_with("~~~")
            {
                let o = b.lead.trim_end_matches(['\n', '\r']).to_string();
                let t = o.trim_start();
                let ch = t.chars().next().unwrap_or('`');
                let n = t.chars().take_while(|c| *c == ch).count();
                if ch == '`' && longest >= n {
                    let f = "`".repeat(longest + 1);
                    (format!("{f}{lang}"), f)
                } else {
                    (t.to_string(), ch.to_string().repeat(n))
                }
            } else {
                let f = "`".repeat((longest + 1).max(3));
                (format!("{f}{lang}"), f)
            };
            let mut s = format!("{open}\n{code}");
            if !code.is_empty() {
                s.push('\n');
            }
            s.push_str(&close);
            s.push('\n');
            s
        }
        Kind::Divider => "***\n".into(),
        Kind::Image { url, alt, title } => {
            if title.is_empty() {
                format!("![{}]({})\n", escape(alt, false), url_md(url))
            } else {
                format!(
                    "![{}]({} \"{}\")\n",
                    escape(alt, false),
                    url_md(url),
                    title.replace('"', "\\\"")
                )
            }
        }
        Kind::Raw => {
            let t: String = b.runs.iter().map(|r| r.text.as_str()).collect();
            let mut t = t.replace('\u{2028}', "\n");
            if !t.ends_with('\n') {
                t.push('\n');
            }
            t
        }
    }
}

fn url_md(url: &str) -> String {
    if url.contains([' ', '(', ')', '<', '>']) {
        format!("<{}>", url.replace('<', "%3C").replace('>', "%3E"))
    } else {
        url.to_string()
    }
}

/// Line breaks in a block's text become new lines, each prefixed with `pad`.
fn continuation(inline: &str, pad: &str) -> String {
    let mut s = String::new();
    for (i, line) in inline.split('\n').enumerate() {
        if i > 0 {
            s.push_str(pad);
        }
        s.push_str(line);
        s.push('\n');
    }
    s
}

fn longest_run(s: &str, ch: char) -> usize {
    let mut best = 0;
    let mut cur = 0;
    for c in s.chars() {
        if c == ch {
            cur += 1;
            best = best.max(cur);
        } else {
            cur = 0;
        }
    }
    best
}

/// Escapes characters that would otherwise start Markdown syntax.
fn escape(s: &str, line_start: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let at_start = line_start && i == 0;
        match c {
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '~' | '|' => {
                out.push('\\');
                out.push(c);
            }
            '!' if chars.get(i + 1) == Some(&'[') => {
                out.push('\\');
                out.push(c);
            }
            '&' => {
                // Only when it could read as an entity.
                let rest: String = chars[i + 1..].iter().take(12).collect();
                let ent = rest
                    .find(';')
                    .map(|p| {
                        p > 0
                            && rest[..p]
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '#')
                    })
                    .unwrap_or(false);
                if ent {
                    out.push('\\');
                }
                out.push(c);
            }
            '\t' if at_start => out.push_str("&#9;"),
            ' ' if at_start => out.push_str("&#32;"),
            '#' | '>' | '+' | '-' | '=' if at_start => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
        if at_start && c.is_ascii_digit() {
            // `1.` or `1)` at the start of a line would start a list.
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '.' || chars[j] == ')') {
                // Escape the delimiter by rewriting the rest: simplest is to escape now.
                let digits: String = chars[i + 1..j].iter().collect();
                out.push_str(&digits);
                out.push('\\');
                out.push(chars[j]);
                let rest: String = chars[j + 1..].iter().collect();
                out.push_str(&escape(&rest, false));
                return out;
            }
        }
    }
    out
}

/// One character's formatting, used to rebuild clean delimiters.
#[derive(Clone, PartialEq)]
struct CharFmt {
    b: bool,
    i: bool,
    s: bool,
    code: bool,
    link: Option<String>,
    hard: bool,
}

/// The canonical Markdown for a block's runs (the house style).
pub(crate) fn inline_md(runs: &[Run]) -> String {
    let mut chars: Vec<(char, CharFmt)> = Vec::new();
    for r in runs {
        for c in r.text.chars() {
            chars.push((
                c,
                CharFmt {
                    b: r.attrs.bold,
                    i: r.attrs.italic,
                    s: r.attrs.strike,
                    code: r.attrs.code,
                    link: r.attrs.link.clone(),
                    hard: r.attrs.hard_break,
                },
            ));
        }
    }
    // Emphasis can't open or close on whitespace: move it outside.
    for key in 0..3 {
        let get = |f: &CharFmt| match key {
            0 => f.b,
            1 => f.i,
            _ => f.s,
        };
        let mut i = 0;
        while i < chars.len() {
            if !get(&chars[i].1) {
                i += 1;
                continue;
            }
            let start = i;
            while i < chars.len() && get(&chars[i].1) {
                i += 1;
            }
            let end = i;
            let mut a = start;
            while a < end && chars[a].0.is_whitespace() {
                a += 1;
            }
            let mut z = end;
            while z > a && chars[z - 1].0.is_whitespace() {
                z -= 1;
            }
            for (k, ch) in chars.iter_mut().enumerate().take(end).skip(start) {
                if k < a || k >= z || ch.0 == '\u{2028}' {
                    match key {
                        0 => ch.1.b = false,
                        1 => ch.1.i = false,
                        _ => ch.1.s = false,
                    }
                }
            }
        }
    }
    // Group into segments of equal formatting.
    let mut segs: Vec<(String, CharFmt)> = Vec::new();
    for (c, f) in chars {
        match segs.last_mut() {
            Some((t, g)) if *g == f && c != '\u{2028}' && !t.ends_with('\u{2028}') => t.push(c),
            _ => segs.push((c.to_string(), f)),
        }
    }
    let mut out = String::new();
    let mut at_line_start = true;
    let mut idx = 0;
    while idx < segs.len() {
        // A link groups consecutive segments with the same URL.
        let link = segs[idx].1.link.clone();
        let mut j = idx;
        while j < segs.len() && segs[j].1.link == link && segs[j].0 != "\u{2028}" {
            j += 1;
        }
        if j == idx {
            // A line break.
            let f = &segs[idx].1;
            out.push_str(if f.hard { "\\\n" } else { "\n" });
            at_line_start = true;
            idx += 1;
            continue;
        }
        let inner = fmt_segments(&segs[idx..j], at_line_start && link.is_none());
        match &link {
            Some(url) => {
                let text: String = segs[idx..j].iter().map(|s| s.0.as_str()).collect();
                let plain = segs[idx..j]
                    .iter()
                    .all(|s| !s.1.b && !s.1.i && !s.1.s && !s.1.code);
                if plain
                    && text == *url
                    && (url.starts_with("http://") || url.starts_with("https://"))
                {
                    out.push('<');
                    out.push_str(url);
                    out.push('>');
                } else {
                    out.push('[');
                    out.push_str(&inner);
                    out.push_str("](");
                    out.push_str(&url_md(url));
                    out.push(')');
                }
            }
            None => out.push_str(&inner),
        }
        at_line_start = false;
        idx = j;
    }
    out
}

fn fmt_segments(segs: &[(String, CharFmt)], line_start: bool) -> String {
    let mut out = String::new();
    let mut open: Vec<&'static str> = Vec::new();
    let mut first = line_start;
    for (text, f) in segs {
        let want: Vec<&'static str> = [(f.b, "**"), (f.i, "*"), (f.s, "~~")]
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, d)| *d)
            .collect();
        // Keep the open delimiters that are still wanted (outermost first), close the rest.
        let keep = open.iter().take_while(|d| want.contains(d)).count();
        while open.len() > keep {
            let d = open.pop().unwrap();
            close_delim(&mut out, d);
        }
        let new: Vec<&'static str> = want.iter().filter(|d| !open.contains(d)).copied().collect();
        // Opening delimiters can't be followed by whitespace: emit it first.
        let lead_ws: String = if new.is_empty() || f.code {
            String::new()
        } else {
            text.chars()
                .take_while(|c| c.is_whitespace() && *c != '\u{2028}')
                .collect()
        };
        let text: &str = &text[lead_ws.len()..];
        out.push_str(&lead_ws);
        for d in new {
            out.push_str(d);
            open.push(d);
        }
        if f.code {
            let n = longest_run(text, '`') + 1;
            let fence = "`".repeat(n);
            let pad = text.starts_with('`')
                || text.ends_with('`')
                || (text.starts_with(' ') && text.ends_with(' ') && !text.trim().is_empty());
            out.push_str(&fence);
            if pad {
                out.push(' ');
            }
            out.push_str(text);
            if pad {
                out.push(' ');
            }
            out.push_str(&fence);
        } else {
            out.push_str(&escape(text, first));
        }
        first = false;
    }
    while let Some(d) = open.pop() {
        close_delim(&mut out, d);
    }
    out
}

/// Closes `d`, moving trailing whitespace outside it (a closer can't follow whitespace).
fn close_delim(out: &mut String, d: &str) {
    let trimmed = out.trim_end_matches([' ', '\t']).len();
    // Don't move whitespace that an escape protects.
    let ws = out[trimmed..].to_string();
    out.truncate(trimmed);
    out.push_str(d);
    out.push_str(&ws);
}
