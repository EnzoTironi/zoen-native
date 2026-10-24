//! Durable native editor input. The encrypted document journal is the only store.
use super::{drafts::tagged, PageBlockDto, PageDto, PageEditResult, PlanPageBlockDto, TextSpanDto};
use crate::{
    engine::{Engine, R},
    FileEditDraftScope,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(super) const MAX_CAPTURE: usize = 1024 * 1024;
const MAX_RANGES: usize = 8192;
const RECEIPTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum PageInputMode {
    RawRich,
    Markdown,
    LosslessText,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum PageInputTarget {
    Block { block_id: String },
    AppendNew,
    WholeDocument,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct PageInputBindingDto {
    pub writer_id: String,
    pub target: PageInputTarget,
    pub mode: PageInputMode,
    pub base_edit_context: String,
    pub base_block_json: String,
    /// Canonical whole-document seed on Open only; never a recovered capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_document_markdown: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct PageInputCaptureDto {
    pub sequence: u64,
    pub payload: Vec<u8>,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct PageInputDto {
    pub revision: u64,
    pub binding: Option<PageInputBindingDto>,
    pub capture: Option<PageInputCaptureDto>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct PageInputAck {
    pub accepted_revision: u64,
    pub current: PageInputDto,
    pub replayed: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct PageInputApplyResult {
    pub ack: PageInputAck,
    pub edit: PageEditResult,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum TaggedBlock {
    Page(PageBlockDto),
    Plan(PlanPageBlockDto),
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub writer_id: String,
    pub target: PageInputTarget,
    pub base_edit_context: String,
    pub base_block: Option<TaggedBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_document: Option<Vec<PageBlockDto>>,
    pub mode: PageInputMode,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Capture {
    pub sequence: u64,
    #[serde(with = "payload_bytes")]
    pub payload: Vec<u8>,
    pub sha256: String,
}
mod payload_bytes {
    use super::*;
    pub fn serialize<S: serde::Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&tagged::encode_bytes(bytes))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        tagged::decode_bytes(&s, MAX_CAPTURE).map_err(serde::de::Error::custom)
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Applied {
    pub mutation_id: String,
    pub mutation_hash: String,
    pub content_hash: String,
    pub context: String,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub fingerprint: String,
    pub accepted_revision: u64,
    pub applied: Option<Applied>,
}
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputStateV1 {
    pub revision: u64,
    pub binding: Option<Binding>,
    pub capture: Option<Capture>,
    pub receipts: Vec<Receipt>,
}
impl InputStateV1 {
    pub fn is_unused(&self) -> bool {
        self == &Self::default()
    }
    pub fn dto(&self) -> R<PageInputDto> {
        Ok(PageInputDto {
            revision: self.revision,
            binding: self
                .binding
                .as_ref()
                .map(|b| {
                    Ok::<_, crate::CoreError>(PageInputBindingDto {
                        writer_id: b.writer_id.clone(),
                        target: b.target.clone(),
                        mode: b.mode.clone(),
                        base_edit_context: b.base_edit_context.clone(),
                        base_block_json: serde_json::to_string(&b.base_block)
                            .map_err(|_| invalid("invalid base block"))?,
                        base_document_markdown: None,
                    })
                })
                .transpose()?,
            capture: self.capture.as_ref().map(|c| PageInputCaptureDto {
                sequence: c.sequence,
                payload: c.payload.clone(),
                sha256: c.sha256.clone(),
            }),
        })
    }
    pub fn validate(&self) -> R<()> {
        if self.receipts.len() > RECEIPTS || self.capture.is_some() && self.binding.is_none() {
            return Err(invalid("invalid input journal"));
        }
        if let Some(b) = &self.binding {
            validate_writer(&b.writer_id)?;
            validate_target(&b.target)?;
            if b.base_edit_context.len() > tagged::MAX_CONTEXT_BYTES {
                return Err(invalid("input context exceeds limit"));
            }
            match (&b.target, &b.base_block, &b.base_document) {
                (PageInputTarget::AppendNew, None, None) => {}
                (PageInputTarget::WholeDocument, None, Some(blocks))
                    if b.mode == PageInputMode::Markdown =>
                {
                    whole_document::validate(blocks)?;
                }
                (PageInputTarget::Block { block_id }, Some(TaggedBlock::Page(p)), None)
                    if *block_id == p.id => {}
                (PageInputTarget::Block { block_id }, Some(TaggedBlock::Plan(p)), None)
                    if *block_id == p.id => {}
                _ => return Err(invalid("invalid input base block")),
            }
            if let Some(c) = &self.capture {
                validate_payload(&b.mode, c.sequence, &c.payload)?;
                if digest(&c.payload) != c.sha256 {
                    return Err(invalid("invalid capture digest"));
                }
            }
        }
        let mut previous = 0;
        let mut fingerprints = std::collections::HashSet::new();
        for r in &self.receipts {
            if r.accepted_revision == 0
                || r.accepted_revision <= previous
                || r.accepted_revision > self.revision
                || !fingerprints.insert(&r.fingerprint)
            {
                return Err(invalid("invalid input receipt"));
            }
            previous = r.accepted_revision;
        }
        Ok(())
    }
    pub fn require_empty(&self) -> R<()> {
        if self.capture.is_some() {
            Err(invalid(
                "native editor input must be applied or discarded first",
            ))
        } else {
            Ok(())
        }
    }
    pub fn next(&mut self) -> R<()> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("input revision overflow"))?;
        Ok(())
    }
    pub fn accept(&mut self, fingerprint: String, applied: Option<Applied>) -> R<()> {
        self.next()?;
        self.receipts.push(Receipt {
            fingerprint,
            accepted_revision: self.revision,
            applied,
        });
        if self.receipts.len() > RECEIPTS {
            self.receipts.remove(0);
        }
        Ok(())
    }
    pub fn replay(&self, fingerprint: &str) -> Option<&Receipt> {
        self.receipts.iter().find(|r| r.fingerprint == fingerprint)
    }
    pub fn check(&self, writer: &str, revision: u64) -> R<&Binding> {
        let b = self
            .binding
            .as_ref()
            .ok_or_else(|| invalid("input writer is no longer active"))?;
        if b.writer_id != writer || self.revision != revision {
            return Err(invalid("stale input writer or revision"));
        }
        Ok(b)
    }
    pub fn consume(&self, writer: &str, revision: u64, sequence: u64, sha: &str) -> R<&Binding> {
        let b = self.check(writer, revision)?;
        let c = self
            .capture
            .as_ref()
            .ok_or_else(|| invalid("no captured input"))?;
        if c.sequence != sequence || c.sha256 != sha {
            return Err(invalid("stale input capture"));
        }
        Ok(b)
    }
}
pub(super) fn invalid(reason: &str) -> crate::CoreError {
    tagged::invalid(reason)
}
pub(super) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub(super) fn fingerprint(value: &impl Serialize) -> R<String> {
    Ok(digest(&tagged::bounded_json(
        value,
        MAX_CAPTURE + tagged::MAX_CONTEXT_BYTES + 2048,
    )?))
}
pub(super) fn validate_writer(s: &str) -> R<()> {
    if s.is_empty()
        || s.len() > 128
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Err(invalid("invalid input writer"))
    } else {
        Ok(())
    }
}
pub(super) fn validate_target(t: &PageInputTarget) -> R<()> {
    if let PageInputTarget::Block { block_id } = t {
        if block_id.is_empty() || block_id.len() > 256 || block_id.chars().any(char::is_control) {
            return Err(invalid("invalid input target"));
        }
    }
    Ok(())
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct Range {
    start: u32,
    end: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inline {
    start: u32,
    end: u32,
    #[serde(rename = "type")]
    kind: String,
    url: Option<String>,
    origin: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RichBlock {
    start: u32,
    end: u32,
    #[serde(rename = "type")]
    kind: u32,
    level: u32,
    ordinal: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Typing {
    add: Vec<String>,
    remove: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rich {
    schema: String,
    sequence: String,
    mode: String,
    text: String,
    selection: Range,
    composition: Option<Range>,
    inline: Vec<Inline>,
    blocks: Vec<RichBlock>,
    typing: Typing,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Text {
    schema: String,
    sequence: String,
    mode: String,
    text: String,
    selection: Range,
    composition: Option<Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    spans: Option<Vec<TextSpanDto>>,
}
fn decimal(s: &str) -> Option<u64> {
    if s.is_empty()
        || s.len() > 20
        || s.len() > 1 && s.starts_with('0')
        || !s.bytes().all(|b| b.is_ascii_digit())
    {
        None
    } else {
        s.parse().ok()
    }
}
struct Utf16Boundaries(Vec<bool>);
impl Utf16Boundaries {
    fn new(text: &str) -> Self {
        // UTF-8 bytes bound the UTF-16 length. Build once per capture so a
        // richly formatted document cannot rescan its whole text per range.
        let mut boundaries = vec![false; text.len() + 1];
        boundaries[0] = true;
        let mut units = 0;
        for c in text.chars() {
            units += c.len_utf16();
            boundaries[units] = true;
        }
        Self(boundaries)
    }
    fn range(&self, start: u32, end: u32) -> R<()> {
        if start > end
            || self.0.get(start as usize) != Some(&true)
            || self.0.get(end as usize) != Some(&true)
        {
            Err(invalid("invalid UTF-16 scalar range"))
        } else {
            Ok(())
        }
    }
}
fn rich(payload: &[u8]) -> R<Rich> {
    let r: Rich = tagged::strict(payload)?;
    if r.schema != "enrm-ios/1" || r.mode != "rich" || r.inline.len() + r.blocks.len() > MAX_RANGES
    {
        return Err(invalid("unsupported rich input schema or range count"));
    }
    let boundaries = Utf16Boundaries::new(&r.text);
    boundaries.range(r.selection.start, r.selection.end)?;
    if let Some(c) = &r.composition {
        boundaries.range(c.start, c.end)?;
    }
    let styles = [
        "strong",
        "emphasis",
        "underline",
        "strikethrough",
        "link",
        "spoiler",
    ];
    for i in &r.inline {
        boundaries.range(i.start, i.end)?;
        if !styles.contains(&i.kind.as_str())
            || !["stored", "detected"].contains(&i.origin.as_str())
            || (i.kind == "link") != i.url.is_some()
            || i.url
                .as_ref()
                .is_some_and(|s| s.encode_utf16().count() > 8192)
            || i.origin == "detected" && i.kind != "link"
        {
            return Err(invalid("invalid inline input semantics"));
        }
    }
    for b in &r.blocks {
        boundaries.range(b.start, b.end)?;
        if b.kind > 8 || b.level > 6 || b.ordinal == 0 || b.ordinal > 1_000_000 {
            return Err(invalid("invalid rich block"));
        }
    }
    for list in [&r.typing.add, &r.typing.remove] {
        let unique: std::collections::HashSet<_> = list.iter().collect();
        if list.len() > 6
            || unique.len() != list.len()
            || list.iter().any(|s| !styles.contains(&s.as_str()))
        {
            return Err(invalid("invalid input typing marks"));
        }
    }
    if r.typing.add.iter().any(|s| r.typing.remove.contains(s)) {
        return Err(invalid("conflicting input typing marks"));
    }
    Ok(r)
}
fn text(mode: &PageInputMode, payload: &[u8]) -> R<Text> {
    let t: Text = tagged::strict(payload)?;
    let expected = if *mode == PageInputMode::Markdown {
        "markdown"
    } else {
        "lossless-text"
    };
    if t.schema != "page-text/1"
        || t.mode != expected
        || (*mode == PageInputMode::LosslessText) != t.spans.is_some()
    {
        return Err(invalid("unsupported text input schema"));
    }
    let boundaries = Utf16Boundaries::new(&t.text);
    boundaries.range(t.selection.start, t.selection.end)?;
    if let Some(c) = &t.composition {
        boundaries.range(c.start, c.end)?;
    }
    if let Some(spans) = &t.spans {
        if spans.len() > MAX_RANGES {
            return Err(invalid("too many text input spans"));
        }
        for s in spans {
            boundaries.range(s.start, s.end)?;
            if !["b", "i", "s", "c", "a", "hb"].contains(&s.key.as_str()) {
                return Err(invalid("unsupported text input span"));
            }
        }
    }
    Ok(t)
}
pub(super) fn validate_payload(mode: &PageInputMode, sequence: u64, payload: &[u8]) -> R<()> {
    if payload.is_empty() || payload.len() > MAX_CAPTURE {
        return Err(invalid("input capture exceeds 1 MiB"));
    }
    let seq = match mode {
        PageInputMode::RawRich => rich(payload)?.sequence,
        _ => text(mode, payload)?.sequence,
    };
    if decimal(&seq) != Some(sequence) {
        return Err(invalid("input sequence does not match payload"));
    }
    Ok(())
}
fn empty_block(id: String) -> PageBlockDto {
    PageBlockDto {
        id,
        kind: "paragraph".into(),
        level: 0,
        indent: 0,
        number: 0,
        checked: false,
        lang: String::new(),
        url: String::new(),
        alt: String::new(),
        setext: None,
        title: None,
        text: String::new(),
        spans: Vec::new(),
    }
}

impl Engine {
    pub(super) fn with_input<T>(
        &self,
        scope: &FileEditDraftScope,
        f: impl FnOnce(&mut super::session::PageSession) -> R<T>,
    ) -> R<T> {
        self.validate_private_draft_scope(&crate::message_drafts::DraftScope {
            expected_identity_id: scope.expected_identity_id.clone(),
            expected_device_id: scope.expected_device_id.clone(),
            space_id: scope.space_id.clone(),
            thread_root_id: None,
        })?;
        let it = self.item_state(&scope.item_id)?;
        if it.space != scope.space_id
            || !matches!(it.kind, types::ItemKind::Page | types::ItemKind::Plan)
        {
            return Err(invalid("foreign input item scope"));
        }
        self.with_document(&scope.item_id, it.kind, |_, s| {
            s.input_ready(self)?;
            f(s)
        })
    }
    pub(crate) fn page_input_load(&self, s: &FileEditDraftScope) -> R<PageInputDto> {
        self.with_input(s, |p| p.input_state().dto())
    }
    pub(crate) fn page_input_open(
        &self,
        s: &FileEditDraftScope,
        r: u64,
        w: &str,
        t: PageInputTarget,
        m: PageInputMode,
        c: &str,
    ) -> R<PageInputDto> {
        self.with_input(s, |p| p.input_open(self, r, w, t, m, c))
    }
    pub(crate) fn page_input_checkpoint(
        &self,
        s: &FileEditDraftScope,
        w: &str,
        r: u64,
        q: u64,
        b: &[u8],
    ) -> R<PageInputAck> {
        self.with_input(s, |p| p.input_checkpoint(self, w, r, q, b))
    }
    pub(crate) fn page_input_discard(
        &self,
        s: &FileEditDraftScope,
        w: &str,
        r: u64,
        q: u64,
        h: &str,
        id: &str,
    ) -> R<PageInputAck> {
        self.with_input(s, |p| p.input_discard(self, w, r, q, h, id))
    }
    pub(crate) fn page_input_release_empty(
        &self,
        s: &FileEditDraftScope,
        w: &str,
        r: u64,
        id: &str,
    ) -> R<PageInputAck> {
        self.with_input(s, |p| p.input_release_empty(self, w, r, id))
    }
    pub(crate) fn page_input_apply(
        &self,
        s: &FileEditDraftScope,
        w: &str,
        r: u64,
        q: u64,
        h: &str,
        id: &str,
    ) -> R<PageInputApplyResult> {
        self.with_input(s, |p| {
            let (ack, edit) = p.input_apply(self, w, r, q, h, id)?;
            let page: PageDto = self.page_dto(self.item_state(&s.item_id)?, edit.current)?;
            Ok(PageInputApplyResult {
                ack,
                edit: PageEditResult {
                    mutation_hash: edit.request_hash,
                    mutation_id: edit.mutation_id,
                    applied_content_hash: edit.applied_content_hash,
                    replayed: edit.replayed,
                    applied_edit_context: edit.applied_edit_context,
                    page,
                },
            })
        })
    }
}

#[cfg(test)]
mod tests;

mod conversion;
pub(super) use conversion::page_edits;
pub(super) mod whole_document;
