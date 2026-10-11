//! Pure typed editing of a verified observed Plan branch. No session or writer is enabled.

use std::collections::{HashMap, HashSet};

use loro::{
    ContainerID, ContainerType, JsonMapOp, JsonMovableListOp, JsonOpContent, LoroMap, LoroText,
    LoroValue, TextDelta, ID,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::*;
use crate::{Attrs, BlockEdit, Kind, Run, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanBlock {
    pub id: String,
    pub content: PlanBlockContent,
    pub runs: Vec<Run>,
}

impl PlanBlock {
    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum PlanBlockContent {
    Title {
        budget_cents: Option<i64>,
    },
    Summary,
    Section,
    Line {
        legacy_id: String,
        cost_cents: i64,
        done: bool,
    },
    Prose {
        kind: Kind,
    },
}

impl PlanBlockContent {
    fn role(&self) -> BlockKind {
        match self {
            Self::Title { .. } => BlockKind::Title,
            Self::Summary => BlockKind::Summary,
            Self::Section => BlockKind::Section,
            Self::Line { .. } => BlockKind::Line,
            Self::Prose { .. } => BlockKind::Prose,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlanBlockEdit {
    pub id: String,
    pub content: PlanBlockContent,
    pub text: String,
    /// UTF-16 scalar boundaries into the exact raw text, using Page's supported marks.
    pub spans: Vec<Span>,
}

/// A locally verified causal revision. It includes operation witnesses rather than just a
/// visible-content hash, so deleted or overwritten historical operations remain bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanRevision {
    root: String,
    frontiers: Frontiers,
    version: PlanVersion,
    operations: [u8; 32],
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionToken {
    schema: String,
    root: String,
    frontiers_hex: String,
    operations_hex: String,
}

impl PlanRevision {
    /// A bounded witness, not an authority or a serialized caller-supplied document.
    pub const MAX_ENCODED_BYTES: usize = 64 * 1024;

    pub fn encode(&self) -> Result<Vec<u8>> {
        let token = RevisionToken {
            schema: "zoen.plan.revision.v1".into(),
            root: self.root.clone(),
            frontiers_hex: hex::encode(self.frontiers.encode()),
            operations_hex: hex::encode(self.operations),
        };
        // Check the large variable field before the serializer allocates its output.
        if token.frontiers_hex.len() > Self::MAX_ENCODED_BYTES {
            return Err(PlanError::Limit("plan revision token"));
        }
        let bytes = codec::bounded_json(&token)?;
        if bytes.len() > Self::MAX_ENCODED_BYTES {
            return Err(PlanError::Limit("plan revision token"));
        }
        Ok(bytes)
    }
}

impl PlanDocument {
    pub fn verify_revision(&self, encoded: &[u8]) -> Result<PlanRevision> {
        if encoded.len() > PlanRevision::MAX_ENCODED_BYTES {
            return Err(PlanError::Limit("plan revision token"));
        }
        let token: RevisionToken =
            serde_json::from_value(codec::strict_value(encoded)?).map_err(error)?;
        if token.schema != "zoen.plan.revision.v1" || token.root != self.seed.root() {
            return Err(PlanError::InvalidRevision);
        }
        let frontiers_bytes = hex::decode(&token.frontiers_hex).map_err(error)?;
        let frontiers = Frontiers::decode(&frontiers_bytes).map_err(error)?;
        let operations_bytes = hex::decode(&token.operations_hex).map_err(error)?;
        let operations: [u8; 32] = operations_bytes
            .try_into()
            .map_err(|_| PlanError::InvalidRevision)?;
        if hex::encode(&frontiers_bytes) != token.frontiers_hex
            || frontiers.encode() != frontiers_bytes
            || hex::encode(operations) != token.operations_hex
        {
            return Err(PlanError::InvalidRevision);
        }
        let version = self
            .page
            .doc
            .frontiers_to_vv(&frontiers)
            .ok_or(PlanError::InvalidRevision)?;
        let revision = PlanRevision {
            root: token.root,
            frontiers,
            version: PlanVersion(version),
            operations,
        };
        self.at(&revision)?;
        Ok(revision)
    }

    pub fn blocks(&self) -> Result<Vec<PlanBlock>> {
        read_nodes(&self.page, self.seed.root())?
            .iter()
            .map(project_block)
            .collect()
    }

    pub fn revision(&self) -> Result<PlanRevision> {
        self.blocks()?;
        Ok(PlanRevision {
            root: self.seed.root().into(),
            frontiers: self.page.frontiers(),
            version: self.version(),
            operations: codec::fingerprint(&self.full_update())?,
        })
    }

    pub fn at(&self, revision: &PlanRevision) -> Result<Self> {
        if revision.root != self.seed.root()
            || !self.page.version().includes_vv(&revision.version.0)
            || self.page.doc.frontiers_to_vv(&revision.frontiers).as_ref()
                != Some(&revision.version.0)
        {
            return Err(PlanError::InvalidRevision);
        }
        let historical = self.page.at(&revision.frontiers).map_err(error)?;
        let result = Self {
            page: editor_fork(&historical)?,
            seed: self.seed.clone(),
        };
        if codec::fingerprint(&result.full_update())? != revision.operations {
            return Err(PlanError::InvalidRevision);
        }
        result.blocks()?;
        Ok(result)
    }

    /// Edits an independent observed branch, then atomically admits its delta into the
    /// current document. Unseen blocks and untouched independent registers are retained.
    pub fn apply_from(
        &mut self,
        revision: &PlanRevision,
        order: &[String],
        changed: &[PlanBlockEdit],
    ) -> Result<CommandResult> {
        let observed = self.at(revision)?;
        validate_edit(&self.page, &observed, order, changed)?;
        let before = observed.version();
        apply_edit(&observed.page, order, changed)?;
        observed.page.doc.commit();
        read_nodes(&observed.page, self.seed.root())?;
        if observed.version() == before {
            return Ok(CommandResult::Unchanged);
        }
        let update = observed.updates_since(&before)?;
        let staged = self.import_staged(&update, &self.seed)?;
        self.page = staged;
        Ok(CommandResult::Changed)
    }

    /// Exact typed block content and canonical supported marks. Financial approval keeps
    /// its separate text/cents-only hash through ExpectedLine.
    pub fn semantic_hash(&self) -> Result<String> {
        let mut hash = Sha256::new().chain_update(b"zoen-plan-semantic-v1\0");
        for block in self.blocks()? {
            let bytes = serde_json::to_vec(&(
                &block.id,
                &block.content,
                block.plain_text(),
                crate::canonical_spans(&block.runs),
            ))
            .map_err(error)?;
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        Ok(hex::encode(hash.finalize()))
    }
}

fn project_block(node: &Node) -> Result<PlanBlock> {
    let content = match node.kind {
        BlockKind::Title => PlanBlockContent::Title {
            budget_cents: budget(&node.map)?,
        },
        BlockKind::Summary => PlanBlockContent::Summary,
        BlockKind::Section => PlanBlockContent::Section,
        BlockKind::Line => {
            let value = line(node)?;
            PlanBlockContent::Line {
                legacy_id: value.id,
                cost_cents: value.cost_cents,
                done: value.done,
            }
        }
        BlockKind::Prose => PlanBlockContent::Prose {
            kind: parse_prose_kind(&string(&node.map, "prose_kind")?)?,
        },
    };
    Ok(PlanBlock {
        id: node.id.clone(),
        content,
        runs: read_runs(&node.text)?,
    })
}

pub(super) fn read_runs(text: &LoroText) -> Result<Vec<Run>> {
    let mut runs: Vec<Run> = Vec::new();
    for delta in text.to_delta() {
        let TextDelta::Insert { insert, attributes } = delta else {
            return Err(error("unsupported plan text delta"));
        };
        let mut attrs = Attrs::default();
        for (key, value) in attributes.unwrap_or_default() {
            validate_mark_value(&key, &value)?;
            let on = matches!(value, LoroValue::Bool(true));
            match key.as_str() {
                "b" => attrs.bold = on,
                "i" => attrs.italic = on,
                "s" => attrs.strike = on,
                "c" => attrs.code = on,
                "hb" => attrs.hard_break = on,
                "a" => {
                    if let LoroValue::String(url) = value {
                        attrs.link = Some(url.to_string());
                    }
                }
                _ => return Err(error("unsupported plan mark")),
            }
        }
        if insert.is_empty() {
            continue;
        }
        if let Some(previous) = runs.last_mut().filter(|run| run.attrs == attrs) {
            previous.text.push_str(&insert);
        } else {
            runs.push(Run {
                text: insert,
                attrs,
            });
        }
    }
    Ok(runs)
}

pub(super) fn validate_mark_value(key: &str, value: &LoroValue) -> Result<()> {
    let valid = match (key, value) {
        ("b" | "i" | "s" | "c" | "hb", LoroValue::Bool(_) | LoroValue::Null) => true,
        ("a", LoroValue::Null) => true,
        ("a", LoroValue::String(url)) => url.len() <= crate::MAX_SPAN_VALUE_BYTES,
        _ => false,
    };
    if !valid {
        return Err(error("unsupported plan mark or value"));
    }
    Ok(())
}

// Kind's ordinary Page decoder permits ignored fields. Plan grammar requires an exact
// known variant with its actual field set, before Loro can hide overwritten assignments.
pub(super) fn parse_prose_kind(encoded: &str) -> Result<Kind> {
    if encoded.len() > crate::MAX_SPAN_VALUE_BYTES {
        return Err(PlanError::Limit("plan prose kind"));
    }
    let raw = codec::strict_value(encoded.as_bytes())?;
    let kind: Kind = serde_json::from_value(raw.clone()).map_err(error)?;
    let canonical = serde_json::to_value(&kind).map_err(error)?;
    let mut normalized = raw;
    // Existing Heading permits omitted setext=false. No other ignored/default fields.
    if let Kind::Heading { level, .. } = kind {
        if !(1..=6).contains(&level) {
            return Err(error("heading level must be between one and six"));
        }
        normalized
            .as_object_mut()
            .ok_or_else(|| error("prose kind must be an object"))?
            .entry("setext")
            .or_insert(serde_json::Value::Bool(false));
    }
    if matches!(kind, Kind::Numbered { number: 0, .. }) || normalized != canonical {
        return Err(error("invalid or unknown prose kind field"));
    }
    Ok(kind)
}

fn validate_edit(
    current: &Page,
    observed: &PlanDocument,
    order: &[String],
    changed: &[PlanBlockEdit],
) -> Result<()> {
    let blocks = observed.blocks()?;
    let existing: HashMap<_, _> = blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    let ordered = crate::validate_ids(order).map_err(error)?;
    if order.len() < 2 || order[0] != blocks[0].id || order[1] != blocks[1].id {
        return Err(error("plan anchors must remain unique and first"));
    }
    let historical = historical_ids(current)?;
    let mut edited = HashSet::new();
    let mut bytes = order
        .iter()
        .try_fold(0usize, |sum, id| sum.checked_add(id.len()))
        .ok_or(PlanError::Limit("plan edit bytes"))?;
    for edit in changed {
        if !ordered.contains(edit.id.as_str()) || !edited.insert(edit.id.as_str()) {
            return Err(error("changed block must occur once in the order"));
        }
        if edit.text.chars().count() > MAX_PLAN_ATOMS
            || edit.spans.len() > crate::MAX_SPANS_PER_BLOCK
        {
            return Err(PlanError::Limit("plan edit characters or spans"));
        }
        let attributes = match &edit.content {
            PlanBlockContent::Line { legacy_id, .. } => legacy_id.len(),
            PlanBlockContent::Prose { kind } => {
                let strings = match kind {
                    Kind::Code { lang } => lang.len(),
                    Kind::Image { url, alt, title } => url
                        .len()
                        .checked_add(alt.len())
                        .and_then(|n| n.checked_add(title.len()))
                        .ok_or(PlanError::Limit("plan prose kind"))?,
                    _ => 0,
                };
                if strings > crate::MAX_SPAN_VALUE_BYTES {
                    return Err(PlanError::Limit("plan prose kind"));
                }
                strings
            }
            _ => 0,
        };
        if attributes > MAX_PLAN_PACKET_BYTES {
            return Err(PlanError::Limit("plan edit bytes"));
        }
        for span in &edit.spans {
            bytes = bytes
                .checked_add(span.key.len())
                .and_then(|n| n.checked_add(span.value.len()))
                .and_then(|n| n.checked_add(32))
                .filter(|n| *n <= MAX_PLAN_PACKET_BYTES)
                .ok_or(PlanError::Limit("plan edit bytes"))?;
        }
        bytes = bytes
            .checked_add(edit.text.len())
            .and_then(|n| {
                codec::bounded_json(&edit.content)
                    .ok()
                    .and_then(|content| n.checked_add(content.len()))
            })
            .filter(|n| *n <= MAX_PLAN_PACKET_BYTES)
            .ok_or(PlanError::Limit("plan edit bytes"))?;
        validate_spans(edit)?;
        let old = existing.get(edit.id.as_str()).copied();
        validate_content(&edit.id, &edit.content, old.map(|block| &block.content))?;
        if old.is_none() && historical.contains_key(&edit.id) {
            return Err(error("a historical plan block ID cannot be reused"));
        }
    }
    if bytes > MAX_PLAN_PACKET_BYTES || order.len() > MAX_PLAN_ATOMS {
        return Err(PlanError::Limit("plan edit size"));
    }
    if order
        .iter()
        .any(|id| !existing.contains_key(id.as_str()) && !edited.contains(id.as_str()))
    {
        return Err(error("new plan block requires typed content"));
    }
    Ok(())
}

fn validate_content(
    id: &str,
    target: &PlanBlockContent,
    old: Option<&PlanBlockContent>,
) -> Result<()> {
    if old.is_some_and(|old| old.role() != target.role())
        || (old.is_none()
            && matches!(
                target,
                PlanBlockContent::Title { .. } | PlanBlockContent::Summary
            ))
    {
        return Err(error("plan block roles are immutable"));
    }
    match target {
        PlanBlockContent::Title { budget_cents } => {
            if budget_cents.is_some_and(|cents| cents < 0) && old != Some(target) {
                return Err(error("new budget cannot be negative"));
            }
        }
        PlanBlockContent::Line {
            legacy_id,
            cost_cents,
            ..
        } => {
            let previous = match old {
                Some(PlanBlockContent::Line {
                    legacy_id: prior_id,
                    cost_cents,
                    ..
                }) => {
                    if prior_id != legacy_id {
                        return Err(error("legacy line identity is immutable"));
                    }
                    Some(*cost_cents)
                }
                None => {
                    if *legacy_id != format!("ln_{id}") {
                        return Err(error("new line identity must derive from its block ID"));
                    }
                    None
                }
                _ => return Err(error("plan block roles are immutable")),
            };
            if *cost_cents < 0 && previous != Some(*cost_cents) {
                return Err(error("new cost cannot be negative"));
            }
        }
        PlanBlockContent::Prose { kind } => {
            parse_prose_kind(&serde_json::to_string(kind).map_err(error)?)?;
        }
        PlanBlockContent::Summary | PlanBlockContent::Section => {}
    }
    Ok(())
}

fn validate_spans(edit: &PlanBlockEdit) -> Result<Vec<Span>> {
    crate::validate_spans(&BlockEdit {
        id: edit.id.clone(),
        kind: Kind::Paragraph,
        text: edit.text.clone(),
        spans: edit.spans.clone(),
    })
    .map_err(error)?;
    let mut ranges: HashMap<&str, Vec<Span>> = HashMap::new();
    for span in &edit.spans {
        if span.key != "a" && !matches!(span.value.as_str(), "" | "1") {
            return Err(error("boolean plan marks require an empty value or one"));
        }
        if span.start < span.end {
            let mut normalized = span.clone();
            if normalized.key != "a" {
                normalized.value.clear();
            }
            ranges.entry(&span.key).or_default().push(normalized);
        }
    }
    let mut merged: Vec<Span> = Vec::new();
    for group in ranges.values_mut() {
        group.sort_by_key(|span| (span.start, span.end));
        for span in group.iter() {
            if let Some(prior) = merged.last_mut().filter(|prior| prior.key == span.key) {
                if span.start < prior.end && span.value != prior.value {
                    return Err(error("conflicting overlapping plan marks"));
                }
                if span.start <= prior.end && span.value == prior.value {
                    prior.end = prior.end.max(span.end);
                    continue;
                }
            }
            merged.push(span.clone());
        }
    }
    Ok(merged)
}

fn apply_edit(page: &Page, order: &[String], changed: &[PlanBlockEdit]) -> Result<()> {
    let list = page.blocks_list();
    let desired: HashSet<_> = order.iter().map(String::as_str).collect();
    for index in (0..list.len()).rev() {
        if !desired.contains(string(&block_map(page, index)?, "id")?.as_str()) {
            list.delete(index, 1).map_err(error)?;
        }
    }
    let edits: HashMap<_, _> = changed
        .iter()
        .map(|edit| (edit.id.as_str(), edit))
        .collect();
    for (index, id) in order.iter().enumerate() {
        let map = if let Some(position) = block_position(page, id)? {
            if position != index {
                list.mov(position, index).map_err(error)?;
            }
            block_map(page, index)?
        } else {
            let edit = edits
                .get(id.as_str())
                .ok_or_else(|| error("missing new block edit"))?;
            let map = list
                .insert_container(index, LoroMap::new())
                .map_err(error)?;
            let legacy = match &edit.content {
                PlanBlockContent::Line {
                    legacy_id,
                    cost_cents,
                    done,
                } => Some(PlanLine {
                    id: legacy_id.clone(),
                    text: edit.text.clone(),
                    cost_cents: *cost_cents,
                    done: *done,
                }),
                _ => None,
            };
            initialize_block(
                &map,
                id,
                edit.content.role(),
                &edit.text,
                legacy.as_ref(),
                None,
            )?;
            if let PlanBlockContent::Prose { kind } = &edit.content {
                map.insert("prose_kind", serde_json::to_string(kind).map_err(error)?)
                    .map_err(error)?;
            }
            map
        };
        if let Some(edit) = edits.get(id.as_str()) {
            apply_fields(&map, &edit.content)?;
            apply_rich_text(&text_container(&map)?, edit)?;
        }
    }
    Ok(())
}

fn apply_fields(map: &LoroMap, content: &PlanBlockContent) -> Result<()> {
    match content {
        PlanBlockContent::Title { budget_cents } => set_budget(map, *budget_cents)?,
        PlanBlockContent::Line {
            cost_cents, done, ..
        } => {
            if integer(map, "cost_cents")? != *cost_cents {
                map.insert("cost_cents", *cost_cents).map_err(error)?;
            }
            if !matches!(map.get("done"), Some(ValueOrContainer::Value(LoroValue::Bool(before))) if before == *done)
            {
                map.insert("done", *done).map_err(error)?;
            }
        }
        PlanBlockContent::Prose { kind } => {
            let encoded = serde_json::to_string(kind).map_err(error)?;
            if string(map, "prose_kind")? != encoded {
                map.insert("prose_kind", encoded).map_err(error)?;
            }
        }
        PlanBlockContent::Summary | PlanBlockContent::Section => {}
    }
    Ok(())
}

fn apply_rich_text(text: &LoroText, edit: &PlanBlockEdit) -> Result<()> {
    if text.to_string() != edit.text {
        text.update(&edit.text, Default::default()).map_err(error)?;
    }
    let have = crate::char_marks(&read_runs(text)?);
    let length = edit.text.chars().count();
    let mut want: HashMap<_, _> = crate::MARKS
        .into_iter()
        .map(|(key, _)| (key, vec![None; length]))
        .collect();
    for span in validate_spans(edit)? {
        let start = crate::u16_to_char(&edit.text, span.start as usize);
        let end = crate::u16_to_char(&edit.text, span.end as usize);
        let value = if span.key == "a" {
            span.value.clone()
        } else {
            "1".into()
        };
        let slots = want
            .get_mut(span.key.as_str())
            .ok_or_else(|| error("unknown plan mark"))?;
        for slot in &mut slots[start..end] {
            *slot = Some(value.clone());
        }
    }
    for (key, _) in crate::MARKS {
        let mut index = 0;
        while index < length {
            if have[key][index] == want[key][index] {
                index += 1;
                continue;
            }
            let start = index;
            let target = &want[key][index];
            while index < length
                && have[key][index] != want[key][index]
                && &want[key][index] == target
            {
                index += 1;
            }
            match target {
                None => text.unmark(start..index, key).map_err(error)?,
                Some(value) => {
                    let value = if key == "a" {
                        LoroValue::from(value.as_str())
                    } else {
                        LoroValue::Bool(true)
                    };
                    text.mark(start..index, key, value).map_err(error)?;
                }
            }
        }
    }
    Ok(())
}

// Lifetime history is bounded explicitly here. Deleted IDs are never removed from this
// registry or reused by a new ordinary block. No second persistent store is introduced.
fn historical_ids(page: &Page) -> Result<HashMap<String, HashSet<ContainerID>>> {
    let update =
        PlanUpdate(page.doc.export_json_updates_without_peer_compression(
            &VersionVector::default(),
            &page.version(),
        ));
    codec::fingerprint(&update)?;
    let mut result: HashMap<String, HashSet<ContainerID>> = HashMap::new();
    for change in update.0.changes {
        for op in change.ops {
            if let JsonOpContent::Map(JsonMapOp::Insert {
                key,
                value: LoroValue::String(id),
            }) = op.content
            {
                if key == "id" {
                    result
                        .entry(id.to_string())
                        .or_default()
                        .insert(op.container);
                }
            }
        }
    }
    Ok(result)
}

pub(super) fn unused_id(page: &Page) -> Result<String> {
    let history = historical_ids(page)?;
    loop {
        let id = new_block_id()?;
        if !history.contains_key(&id) {
            return Ok(id);
        }
    }
}

pub(super) fn validate_operations(page: &Page, update: &PlanUpdate) -> Result<()> {
    let vv = page.version();
    let mut ids = historical_ids(page)?;
    let mut roles = HashMap::new();
    for change in &update.0.changes {
        for op in &change.ops {
            if let JsonOpContent::Map(JsonMapOp::Insert {
                key,
                value: LoroValue::String(role),
            }) = &op.content
            {
                if key == "kind" {
                    roles.insert(op.container.clone(), parse_role(role)?);
                }
                if key == "id" && change.id.peer == MIGRATION_PEER {
                    ids.entry(role.to_string())
                        .or_default()
                        .insert(op.container.clone());
                }
            }
        }
    }
    for change in &update.0.changes {
        for op in &change.ops {
            if op.counter < *vv.get(&change.id.peer).unwrap_or(&0) {
                continue;
            }
            match &op.content {
                JsonOpContent::Map(JsonMapOp::Insert { key, value }) => {
                    if matches!(&op.container, ContainerID::Root { name, .. } if name.as_str() == "page")
                    {
                        if !matches!(key.as_str(), "schema" | "root") {
                            return Err(error("unknown plan root attribute"));
                        }
                        continue;
                    }
                    let role = page
                        .doc
                        .try_get_map(op.container.clone())
                        .and_then(|map| string(&map, "kind").ok())
                        .map(|role| parse_role(&role))
                        .transpose()?
                        .or_else(|| roles.get(&op.container).copied())
                        .ok_or_else(|| error("plan attributes need a typed block role"))?;
                    if !role_key(role, key) {
                        return Err(error("attribute does not belong to its plan block role"));
                    }
                    if change.id.peer != MIGRATION_PEER {
                        if key == "id" {
                            let LoroValue::String(id) = value else {
                                return Err(error("invalid block ID"));
                            };
                            let owners = ids.entry(id.to_string()).or_default();
                            if owners.iter().any(|owner| *owner != op.container) {
                                return Err(error("a historical plan block ID cannot be reused"));
                            }
                            owners.insert(op.container.clone());
                        }
                        if key == "text" {
                            let expected = ContainerID::new_normal(
                                ID::new(change.id.peer, op.counter),
                                ContainerType::Text,
                            );
                            if value != &LoroValue::Container(expected) {
                                return Err(error("a new block must own its rich text container"));
                            }
                        }
                    }
                }
                JsonOpContent::MovableList(JsonMovableListOp::Insert { value, .. }) => {
                    if change.id.peer != MIGRATION_PEER {
                        for (offset, value) in value.iter().enumerate() {
                            let expected = ContainerID::new_normal(
                                ID::new(change.id.peer, op.counter + offset as i32),
                                ContainerType::Map,
                            );
                            if value != &LoroValue::Container(expected) {
                                return Err(error("a new list entry must own a new block map"));
                            }
                        }
                    }
                }
                JsonOpContent::MovableList(JsonMovableListOp::Set { value, .. })
                    if change.id.peer != MIGRATION_PEER =>
                {
                    let expected = ContainerID::new_normal(
                        ID::new(change.id.peer, op.counter),
                        ContainerType::Map,
                    );
                    if value != &LoroValue::Container(expected) {
                        return Err(error("a replacement list entry must own a new block map"));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub(super) fn validate_touched_maps(page: &Page, update: &PlanUpdate) -> Result<()> {
    let mut maps = HashSet::new();
    for change in &update.0.changes {
        for op in &change.ops {
            if op.container.container_type() == ContainerType::Map
                && matches!(op.container, ContainerID::Normal { .. })
            {
                maps.insert(op.container.clone());
            }
        }
    }
    for address in maps {
        let map = page
            .doc
            .try_get_map(address)
            .ok_or_else(|| error("missing typed block map"))?;
        let node = read_node(map)?;
        if node.kind == BlockKind::Line
            && node.map.get_last_editor("line_id") != Some(MIGRATION_PEER)
            && string(&node.map, "line_id")? != format!("ln_{}", node.id)
        {
            return Err(error("new line identity must derive from its block ID"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
