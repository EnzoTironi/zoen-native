//! Canonical and source-preserving Markdown rendering.
use crate::{Block, DocError, Kind, Run};

// ───────────────────────────── export ─────────────────────────────

pub(crate) fn export(lead: &str, blocks: &[Block], canonical: bool) -> Result<String, DocError> {
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
        if !canonical && !b.src.is_empty() && b.fingerprint()? == b.fp {
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
    Ok(s)
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
fn item_lead(block: &Block) -> String {
    let fresh = bullet_prefix(&block.kind);
    if block.lead.is_empty() {
        return fresh;
    }
    let orig = crate::Block::kind_from_fp(&block.fp);
    let same_shape = match (&orig, &block.kind) {
        (
            Some(Kind::Task {
                indent: prior_indent,
                ..
            }),
            Kind::Task {
                indent: current_indent,
                ..
            },
        )
        | (
            Some(Kind::Bullet {
                indent: prior_indent,
            }),
            Kind::Bullet {
                indent: current_indent,
            },
        ) => prior_indent == current_indent,
        (
            Some(Kind::Numbered {
                indent: prior_indent,
                number: prior_number,
            }),
            Kind::Numbered {
                indent: current_indent,
                number: current_number,
            },
        ) => prior_indent == current_indent && prior_number == current_number,
        _ => false,
    };
    if !same_shape {
        return fresh;
    }
    let mut lead = block.lead.trim_end_matches(['\n', '\r']).to_string();
    if !lead.ends_with(' ') && !lead.ends_with('\t') {
        lead.push(' ');
    }
    if let Kind::Task { checked, .. } = block.kind {
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
        Kind::Heading { level, setext } => {
            let lead = if !b.lead.is_empty()
                && matches!(crate::Block::kind_from_fp(&b.fp), Some(Kind::Heading { level: l, .. }) if l == *level)
            {
                b.lead.clone()
            } else {
                format!("{} ", "#".repeat((*level).clamp(1, 6) as usize))
            };
            let mut text = inline;
            if text.ends_with('#') {
                text.insert(text.len() - 1, '\\');
            }
            if (*setext || text.contains('\n')) && *level <= 2 {
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
        Kind::Code { lang } => render_code(b, lang),
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

fn render_code(block: &Block, lang: &str) -> String {
    let code: String = block
        .runs
        .iter()
        .map(|r| r.text.as_str())
        .collect::<String>()
        .replace('\u{2028}', "\n");
    let longest = longest_run(&code, '`');
    let (open, close) = if block.lead.trim_start().starts_with("```")
        || block.lead.trim_start().starts_with("~~~")
    {
        let original = block.lead.trim_end_matches(['\n', '\r']).to_string();
        let trimmed = original.trim_start();
        let ch = trimmed.chars().next().unwrap_or('`');
        let fence_length = trimmed.chars().take_while(|c| *c == ch).count();
        if ch == '`' && longest >= fence_length {
            let fence = "`".repeat(longest + 1);
            (format!("{fence}{lang}"), fence)
        } else {
            (trimmed.to_string(), ch.to_string().repeat(fence_length))
        }
    } else {
        let fence = "`".repeat((longest + 1).max(3));
        (format!("{fence}{lang}"), fence)
    };
    let mut s = format!("{open}\n{code}");
    if !code.is_empty() {
        s.push('\n');
    }
    s.push_str(&close);
    s.push('\n');
    s
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
                let ent = rest.find(';').is_some_and(|p| {
                    p > 0
                        && rest[..p]
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '#')
                });
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

/// The canonical Markdown for a block's runs (the house style).
pub(crate) fn inline_md(runs: &[Run]) -> String {
    let chars = formatted_characters(runs);
    // Group into segments of equal formatting.
    let mut segs: Vec<(String, crate::Attrs)> = Vec::new();
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
            out.push_str(if f.hard_break { "\\\n" } else { "\n" });
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
                    .all(|s| !s.1.bold && !s.1.italic && !s.1.strike && !s.1.code);
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

fn formatted_characters(runs: &[Run]) -> Vec<(char, crate::Attrs)> {
    let mut chars: Vec<(char, crate::Attrs)> = Vec::new();
    for r in runs {
        for c in r.text.chars() {
            chars.push((
                c,
                crate::Attrs {
                    bold: r.attrs.bold,
                    italic: r.attrs.italic,
                    strike: r.attrs.strike,
                    code: r.attrs.code,
                    link: r.attrs.link.clone(),
                    hard_break: r.attrs.hard_break,
                },
            ));
        }
    }
    // Emphasis can't open or close on whitespace: move it outside.
    for key in 0..3 {
        let get = |f: &crate::Attrs| match key {
            0 => f.bold,
            1 => f.italic,
            _ => f.strike,
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
                        0 => ch.1.bold = false,
                        1 => ch.1.italic = false,
                        _ => ch.1.strike = false,
                    }
                }
            }
        }
    }
    chars
}

fn fmt_segments(segs: &[(String, crate::Attrs)], line_start: bool) -> String {
    let mut out = String::new();
    let mut open: Vec<&'static str> = Vec::new();
    let mut first = line_start;
    for (text, f) in segs {
        let want: Vec<&'static str> = [(f.bold, "**"), (f.italic, "*"), (f.strike, "~~")]
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, d)| *d)
            .collect();
        // Keep the open delimiters that are still wanted (outermost first), close the rest.
        let keep = open.iter().take_while(|d| want.contains(d)).count();
        for delimiter in open.drain(keep..).rev() {
            close_delim(&mut out, delimiter);
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
