//! Zoen pages (ADR 0040): a page is a Loro document holding a list of blocks; each block
//! is a kind, a few attributes and rich text. Markdown is only an import and export format.
//!
//! Layout inside the Loro document:
//! - map `page`: `lead` (blank lines before the first block of an imported file);
//! - movable list `blocks`: one map per block with `id`, `kind` (JSON of [`Kind`]), `text`
//!   (rich text with marks `b`, `i`, `s`, `c`, `a`=url, `hb`), and, for imported blocks,
//!   `src`, `tail`, `lead` and `fp` (what the block was when imported, to write it back
//!   byte-identical while nobody touched it).

mod block_storage;
mod markdown;

use block_storage::{apply_edit, char_marks, get_str, read_block, u16_to_char, write_block};
pub use block_storage::{block_markdown, spans_of};
pub mod plan;

#[cfg(test)]
mod apply_tests;

use std::collections::{HashMap, HashSet};

use loro::{
    Container, ExpandType, ExportMode, LoroDoc, LoroMap, LoroMovableList, LoroText, LoroValue,
    StyleConfig, StyleConfigMap, ValueOrContainer,
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

    /// SHA-256 of the visible kind, exact text and normalized marks. Source spelling,
    /// run segmentation and the block's identity are not content.
    pub fn content_hash(&self) -> String {
        #[derive(Serialize)]
        struct Content {
            kind: Kind,
            text: String,
            spans: Vec<Span>,
        }
        let kind = match &self.kind {
            Kind::Heading { level, .. } => Kind::Heading {
                level: *level,
                setext: false,
            },
            kind => kind.clone(),
        };
        let bytes = serde_json::to_vec(&Content {
            kind,
            text: self.plain_text(),
            spans: canonical_spans(&self.runs),
        })
        .expect("page content serializes");
        hex::encode(
            Sha256::new()
                .chain_update(b"zoen-page-block-content-v1\0")
                .chain_update(bytes)
                .finalize(),
        )
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    /// `b`, `i`, `s`, `c`, `a` (value = url) or `hb`.
    pub key: String,
    pub value: String,
}

/// Maximum formatting ranges accepted for one block edit.
pub const MAX_SPANS_PER_BLOCK: usize = 16_384;
const MAX_BLOCK_ID_BYTES: usize = 128;
const MAX_SPAN_VALUE_BYTES: usize = 8_192;

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

    /// Applies complete updates (or a snapshot) made anywhere. Missing dependencies
    /// refuse the entire import, including any changes that could apply on their own.
    pub fn import(&self, bytes: &[u8]) -> Result<(), DocError> {
        let staged = self.doc.fork();
        let from = staged.oplog_vv();
        let status = staged.import(bytes).map_err(err)?;
        if status.pending.is_some_and(|pending| !pending.is_empty()) {
            return Err(DocError("page update has missing dependencies".into()));
        }
        // Distinct peers can concurrently insert the same external block id. Refuse
        // that ambiguous merged state before any changes reach the live document.
        validate_ids(&block_ids(&staged.get_movable_list("blocks")))?;
        if staged.oplog_vv() == from {
            return Ok(());
        }
        // Re-export only complete changes. The original already has every dependency
        // in `from`, and never receives Loro's pending queue from the staging copy.
        let delta = staged.export(ExportMode::updates(&from)).map_err(err)?;
        let status = self.doc.import(&delta).map_err(err)?;
        if status.pending.is_some_and(|pending| !pending.is_empty()) {
            return Err(DocError("page update has missing dependencies".into()));
        }
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

    /// SHA-256 of ordered block identities and semantic content, independent of CRDT
    /// history and original Markdown spelling.
    pub fn content_hash(&self) -> String {
        let blocks: Vec<_> = self
            .blocks()
            .into_iter()
            .map(|b| (b.id.clone(), b.content_hash()))
            .collect();
        let bytes = serde_json::to_vec(&blocks).expect("page content serializes");
        hex::encode(
            Sha256::new()
                .chain_update(b"zoen-page-content-v1\0")
                .chain_update(bytes)
                .finalize(),
        )
    }

    /// Editor equality also preserves stored syntax metadata that the public semantic
    /// hash intentionally normalizes (currently Heading.setext). No hash/version change.
    pub fn editor_content_eq(&self, other: &Self) -> bool {
        self.content_hash() == other.content_hash()
            && self
                .blocks()
                .iter()
                .map(|b| &b.kind)
                .eq(other.blocks().iter().map(|b| &b.kind))
    }

    pub fn block_hash(&self, id: &str) -> Option<String> {
        self.blocks()
            .into_iter()
            .find(|b| b.id == id)
            .map(|b| b.content_hash())
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
    /// Validate and stage first, so a refused edit cannot change the original page.
    /// For a stale editor, call this on `at(observed_frontiers)` and merge its updates.
    pub fn apply(&self, order: &[String], changed: &[BlockEdit]) -> Result<(), DocError> {
        validate_apply(&block_ids(&self.blocks_list()), order, changed)?;
        let staged = Page {
            doc: self.doc.fork(),
        };
        let from = staged.version();
        staged.apply_validated(order, changed)?;
        if staged.editor_content_eq(self) {
            return Ok(());
        }
        let delta = staged.doc.export(ExportMode::updates(&from)).map_err(err)?;
        self.import(&delta)
    }

    fn apply_validated(&self, order: &[String], changed: &[BlockEdit]) -> Result<(), DocError> {
        let list = self.blocks_list();
        // Delete blocks no longer present.
        let keep: std::collections::HashSet<&String> = order.iter().collect();
        let current = block_ids(&list);
        for i in (0..current.len()).rev() {
            if !keep.contains(&current[i]) {
                list.delete(i, 1).map_err(err)?;
            }
        }
        // Insert new ones at the end, then move everything into place.
        let mut known: std::collections::HashSet<String> = block_ids(&list).into_iter().collect();
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
            let now = block_ids(&list);
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

    /// An independent copy as it was at `version`. Editing it only changes the copy;
    /// its generated updates can then be merged without replacing unseen blocks.
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

fn block_ids(list: &LoroMovableList) -> Vec<String> {
    (0..list.len())
        .map(|i| match list.get(i) {
            Some(ValueOrContainer::Container(Container::Map(m))) => get_str(&m, "id"),
            _ => String::new(),
        })
        .collect()
}

fn validate_ids(ids: &[String]) -> Result<HashSet<&str>, DocError> {
    let mut seen = HashSet::new();
    for id in ids {
        if id.trim().is_empty() || id.len() > MAX_BLOCK_ID_BYTES {
            return Err(DocError("invalid block id".into()));
        }
        if !seen.insert(id.as_str()) {
            return Err(DocError("duplicate block id".into()));
        }
    }
    Ok(seen)
}

fn validate_apply(
    current: &[String],
    order: &[String],
    changed: &[BlockEdit],
) -> Result<(), DocError> {
    let current = validate_ids(current)?;
    let order_ids = validate_ids(order)?;
    let mut edited = HashSet::new();
    for edit in changed {
        if !order_ids.contains(edit.id.as_str()) {
            return Err(DocError("changed block is not in the order".into()));
        }
        if !edited.insert(edit.id.as_str()) {
            return Err(DocError("duplicate changed block".into()));
        }
        validate_spans(edit)?;
    }
    if order_ids
        .iter()
        .any(|id| !current.contains(id) && !edited.contains(id))
    {
        return Err(DocError("new block requires its content".into()));
    }
    Ok(())
}

fn validate_spans(edit: &BlockEdit) -> Result<(), DocError> {
    if edit.spans.len() > MAX_SPANS_PER_BLOCK {
        return Err(DocError("too many formatting spans".into()));
    }
    let mut offsets = HashSet::new();
    for span in &edit.spans {
        if span.start > span.end {
            return Err(DocError("reversed formatting span".into()));
        }
        if !MARKS.iter().any(|(key, _)| *key == span.key) {
            return Err(DocError("unknown formatting mark".into()));
        }
        if span.value.len() > MAX_SPAN_VALUE_BYTES {
            return Err(DocError("formatting value is too large".into()));
        }
        offsets.insert(span.start as usize);
        offsets.insert(span.end as usize);
    }
    offsets.remove(&0);
    let mut pos = 0;
    for c in edit.text.chars() {
        pos += c.len_utf16();
        offsets.remove(&pos);
    }
    if !offsets.is_empty() {
        return Err(DocError(
            "formatting span is not on a UTF-16 scalar boundary".into(),
        ));
    }
    Ok(())
}

fn canonical_spans(runs: &[Run]) -> Vec<Span> {
    let mut spans = spans_of(runs);
    spans.retain(|span| span.start < span.end);
    spans.sort_by(|a, b| {
        (&a.key, a.start, a.end, &a.value).cmp(&(&b.key, b.start, b.end, &b.value))
    });
    let mut merged: Vec<Span> = Vec::new();
    for span in spans {
        if let Some(last) = merged.last_mut() {
            if last.key == span.key && last.value == span.value && last.end == span.start {
                last.end = span.end;
                continue;
            }
        }
        merged.push(span);
    }
    merged
}
