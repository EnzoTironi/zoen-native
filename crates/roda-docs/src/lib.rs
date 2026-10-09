//! Zoen pages (ADR 0025): a page is a Loro document holding a list of blocks; each block
//! is a kind, a few attributes and rich text. Markdown is only an import and export format.
//!
//! Layout inside the Loro document:
//! - map `page`: `lead` (blank lines before the first block of an imported file);
//! - movable list `blocks`: one map per block with `id`, `kind` (JSON of [`Kind`]), `text`
//!   (rich text with marks `b`, `i`, `s`, `c`, `a`=url, `hb`), and, for imported blocks,
//!   `src`, `tail`, `lead` and `fp` (what the block was when imported, to write it back
//!   byte-identical while nobody touched it).

mod markdown;

use std::collections::HashMap;

use loro::{
    Container, ExpandType, ExportMode, LoroDoc, LoroMap, LoroMovableList, LoroText, LoroValue,
    StyleConfig, StyleConfigMap, TextDelta, ValueOrContainer,
};
pub use loro::{Frontiers, VersionVector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use markdown::block_text;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Kind {
    Paragraph,
    Heading {
        level: u32,
        #[serde(default)]
        setext: bool,
    },
    Bullet {
        indent: u32,
    },
    Numbered {
        indent: u32,
        number: u32,
    },
    Task {
        indent: u32,
        checked: bool,
    },
    Quote,
    Code {
        lang: String,
    },
    Divider,
    Image {
        url: String,
        alt: String,
        title: String,
    },
    /// Markdown we keep verbatim and edit as text.
    Raw,
}

impl Kind {
    pub fn is_list_item(&self) -> bool {
        matches!(
            self,
            Kind::Bullet { .. } | Kind::Numbered { .. } | Kind::Task { .. }
        )
    }
    pub fn id(&self) -> &'static str {
        match self {
            Kind::Paragraph => "paragraph",
            Kind::Heading { .. } => "heading",
            Kind::Bullet { .. } => "bullet",
            Kind::Numbered { .. } => "numbered",
            Kind::Task { .. } => "task",
            Kind::Quote => "quote",
            Kind::Code { .. } => "code",
            Kind::Divider => "divider",
            Kind::Image { .. } => "image",
            Kind::Raw => "raw",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attrs {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
    pub hard_break: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub attrs: Attrs,
}

impl Run {
    pub fn plain(t: &str) -> Run {
        Run {
            text: t.to_string(),
            attrs: Attrs::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub id: String,
    pub kind: Kind,
    pub runs: Vec<Run>,
    /// What the importer saw (empty for blocks made in Zoen).
    pub src: String,
    pub tail: String,
    pub lead: String,
    pub fp: String,
}

#[derive(Serialize, Deserialize)]
struct Fp {
    k: Kind,
    h: String,
}

impl Block {
    pub fn new(kind: Kind, runs: Vec<Run>) -> Block {
        Block {
            id: new_id(),
            kind,
            runs,
            src: String::new(),
            tail: String::new(),
            lead: String::new(),
            fp: String::new(),
        }
    }

    /// The kind plus a hash of the canonical inline Markdown: equal fingerprints mean the
    /// block reads the same.
    pub fn fingerprint(&self) -> String {
        let md = markdown::inline_md(&self.runs);
        serde_json::to_string(&Fp {
            k: self.kind.clone(),
            h: hex::encode(&Sha256::digest(md.as_bytes())[..12]),
        })
        .unwrap_or_default()
    }

    pub(crate) fn kind_from_fp(fp: &str) -> Option<Kind> {
        serde_json::from_str::<Fp>(fp).ok().map(|f| f.k)
    }

    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

fn new_id() -> String {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).expect("randomness");
    hex::encode(b)
}

// ───────────────────────────── the Loro page ─────────────────────────────

pub struct Page {
    doc: LoroDoc,
}

const MARKS: [(&str, ExpandType); 6] = [
    ("b", ExpandType::After),
    ("i", ExpandType::After),
    ("s", ExpandType::After),
    ("c", ExpandType::None),
    ("a", ExpandType::None),
    ("hb", ExpandType::None),
];

fn configured() -> LoroDoc {
    let doc = LoroDoc::new();
    let mut styles = StyleConfigMap::new();
    for (k, e) in MARKS {
        styles.insert(k.into(), StyleConfig { expand: e });
    }
    doc.config_text_style(styles);
    doc
}

#[derive(Debug)]
pub struct DocError(pub String);

impl std::fmt::Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err(e: impl std::fmt::Display) -> DocError {
    DocError(e.to_string())
}

/// A block as an editor hands it back after a change. Spans use UTF-16 offsets into
/// `text`, like `NSString`.
#[derive(Clone, Debug)]
pub struct BlockEdit {
    pub id: String,
    pub kind: Kind,
    pub text: String,
    pub spans: Vec<Span>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    /// `b`, `i`, `s`, `c`, `a` (value = url) or `hb`.
    pub key: String,
    pub value: String,
}

impl Default for Page {
    fn default() -> Self {
        Page::new()
    }
}

impl Page {
    pub fn new() -> Page {
        Page { doc: configured() }
    }

    /// A page from Markdown: one block per Markdown block.
    pub fn from_markdown(md: &str) -> Result<Page, DocError> {
        let p = Page::new();
        let imported = markdown::import(md);
        p.doc
            .get_map("page")
            .insert("lead", imported.lead.as_str())
            .map_err(err)?;
        let list = p.blocks_list();
        for (i, b) in imported.blocks.iter().enumerate() {
            let m = list.insert_container(i, LoroMap::new()).map_err(err)?;
            write_block(&m, b, true)?;
        }
        p.doc.commit();
        Ok(p)
    }

    /// Applies updates (or a snapshot) made anywhere.
    pub fn import(&self, bytes: &[u8]) -> Result<(), DocError> {
        self.doc.import(bytes).map_err(err)?;
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.doc.export(ExportMode::snapshot()).unwrap_or_default()
    }

    pub fn version(&self) -> VersionVector {
        self.doc.oplog_vv()
    }

    pub fn frontiers(&self) -> Frontiers {
        self.doc.oplog_frontiers()
    }

    /// Everything since `from` (empty if nothing changed).
    pub fn updates_since(&self, from: &VersionVector) -> Vec<u8> {
        self.doc.commit();
        if &self.doc.oplog_vv() == from {
            return Vec::new();
        }
        self.doc
            .export(ExportMode::updates(from))
            .unwrap_or_default()
    }

    fn blocks_list(&self) -> LoroMovableList {
        self.doc.get_movable_list("blocks")
    }

    pub fn blocks(&self) -> Vec<Block> {
        let list = self.blocks_list();
        (0..list.len())
            .filter_map(|i| match list.get(i) {
                Some(ValueOrContainer::Container(Container::Map(m))) => Some(read_block(&m)),
                _ => None,
            })
            .collect()
    }

    pub fn title(&self) -> String {
        let blocks = self.blocks();
        blocks
            .iter()
            .find(|b| matches!(b.kind, Kind::Heading { .. }))
            .or_else(|| {
                blocks
                    .iter()
                    .find(|b| !b.plain_text().trim().is_empty() && b.kind != Kind::Raw)
            })
            .map(|b| b.plain_text().replace('\u{2028}', " ").trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_default()
    }

    pub fn to_markdown(&self) -> String {
        let lead = match self.doc.get_map("page").get("lead") {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => s.to_string(),
            _ => String::new(),
        };
        markdown::export(&lead, &self.blocks(), false)
    }

    /// Every block written in house style, ignoring how the source spelled it.
    pub fn canonical_markdown(&self) -> String {
        markdown::export("", &self.blocks(), true)
    }

    /// Makes the page match an editor's state: `order` is every block id top to bottom;
    /// `changed` carries the blocks whose kind or text changed (new ids are created).
    pub fn apply(&self, order: &[String], changed: &[BlockEdit]) -> Result<(), DocError> {
        let list = self.blocks_list();
        let ids = |l: &LoroMovableList| -> Vec<String> {
            (0..l.len())
                .map(|i| match l.get(i) {
                    Some(ValueOrContainer::Container(Container::Map(m))) => get_str(&m, "id"),
                    _ => String::new(),
                })
                .collect()
        };
        // Delete blocks no longer present.
        let keep: std::collections::HashSet<&String> = order.iter().collect();
        let current = ids(&list);
        for i in (0..current.len()).rev() {
            if !keep.contains(&current[i]) {
                list.delete(i, 1).map_err(err)?;
            }
        }
        // Insert new ones at the end, then move everything into place.
        let mut known: std::collections::HashSet<String> = ids(&list).into_iter().collect();
        let by_id: HashMap<&str, &BlockEdit> = changed.iter().map(|b| (b.id.as_str(), b)).collect();
        for id in order {
            if !known.contains(id) {
                let m = list
                    .insert_container(list.len(), LoroMap::new())
                    .map_err(err)?;
                m.insert("id", id.as_str()).map_err(err)?;
                m.insert(
                    "kind",
                    serde_json::to_string(&Kind::Paragraph).unwrap_or_default(),
                )
                .map_err(err)?;
                m.insert_container("text", LoroText::new()).map_err(err)?;
                known.insert(id.clone());
            }
        }
        for (want, id) in order.iter().enumerate() {
            let now = ids(&list);
            if now.get(want) == Some(id) {
                continue;
            }
            if let Some(from) = now.iter().position(|x| x == id) {
                list.mov(from, want).map_err(err)?;
            }
        }
        // Content.
        for i in 0..list.len() {
            let Some(ValueOrContainer::Container(Container::Map(m))) = list.get(i) else {
                continue;
            };
            let id = get_str(&m, "id");
            if let Some(edit) = by_id.get(id.as_str()) {
                apply_edit(&m, edit)?;
            }
        }
        self.doc.commit();
        Ok(())
    }

    /// A read-only copy as it was at `version`.
    pub fn at(&self, version: &Frontiers) -> Result<Page, DocError> {
        let doc = self.doc.fork_at(version).map_err(err)?;
        Ok(Page { doc })
    }

    /// Makes the content equal to what it was at `version` (as a new change).
    pub fn revert_to(&self, version: &Frontiers) -> Result<(), DocError> {
        self.doc.revert_to(version).map_err(err)?;
        self.doc.commit();
        Ok(())
    }
}

fn get_str(m: &LoroMap, key: &str) -> String {
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

fn write_block(m: &LoroMap, b: &Block, imported: bool) -> Result<(), DocError> {
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

fn read_block(m: &LoroMap) -> Block {
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
fn u16_to_char(s: &str, u16: usize) -> usize {
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
fn char_marks(runs: &[Run]) -> HashMap<&'static str, Vec<Option<String>>> {
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

fn apply_edit(m: &LoroMap, e: &BlockEdit) -> Result<(), DocError> {
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
