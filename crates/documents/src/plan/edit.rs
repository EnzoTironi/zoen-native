//! Pure typed editing of a verified observed Plan branch. No session or writer is enabled.

use std::collections::{HashMap, HashSet};

use loro::{
    ContainerID, ContainerType, JsonMapOp, JsonMovableListOp, JsonOpContent, LoroMap, LoroText,
    LoroValue, TextDelta, ID,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    block_map, block_position, budget, codec, editor_fork, error, initialize_block, integer, line,
    read_nodes, set_budget, string, text_container, BlockKind, CommandResult, Frontiers, Node,
    Page, PlanDocument, PlanError, PlanLine, PlanUpdate, PlanVersion, Result, ValueOrContainer,
    MAX_PLAN_ATOMS, MAX_PLAN_PACKET_BYTES,
};

use crate::{Attrs, BlockEdit, Kind, Run, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanBlock {
    pub id: String,
    pub content: PlanBlockContent,
    pub runs: Vec<Run>,
}

impl PlanBlock {
    #[must_use]
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

    /// # Errors
    /// Refuses revision witnesses exceeding the token bounds or JSON serialization failures.
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
    /// # Errors
    /// Refuses malformed, oversized, noncanonical or unobserved revision witnesses.
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

    /// # Errors
    /// Refuses invalid block identities, roles, financial registers or unsupported rich-text marks.
    pub fn blocks(&self) -> Result<Vec<PlanBlock>> {
        read_nodes(&self.page, self.seed.root())?
            .iter()
            .map(project_block)
            .collect()
    }

    /// # Errors
    /// Refuses invalid blocks or an operation history that cannot be fingerprinted.
    pub fn revision(&self) -> Result<PlanRevision> {
        self.blocks()?;
        Ok(PlanRevision {
            root: self.seed.root().into(),
            frontiers: self.page.frontiers(),
            version: self.version(),
            operations: codec::fingerprint(&self.full_update())?,
        })
    }

    /// # Errors
    /// Refuses a revision from another root, unavailable history or a mismatching operation witness.
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
    /// # Errors
    /// Refuses an invalid observed revision, invalid block edits or a staged delta violating plan invariants.
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
    /// its separate text/cents-only hash through `ExpectedLine`.
    /// # Errors
    /// Refuses invalid typed blocks or a failed canonical content encoding.
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
        ("b" | "i" | "s" | "c" | "hb", LoroValue::Bool(_) | LoroValue::Null)
        | ("a", LoroValue::Null) => true,
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
        text.update(&edit.text, loro::UpdateOptions::default())
            .map_err(error)?;
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
#[cfg(test)]
mod tests;

mod provenance;
pub(super) use provenance::{unused_id, validate_operations, validate_touched_maps};

mod validation;
use validation::{validate_edit, validate_spans};
