//! Isolated typed Plan experiment. No production Page codec or client uses this module.
//!
//! Plans use Page's Loro document and one movable block list, without Page::blocks' fallback
//! decoder. Financial fields are independent registers; rich text retains exact legacy bytes.
//! A frozen, deterministic migration peer replays authenticated legacy prefixes. It is never
//! an editor peer. Inspectable Loro operations are checked before import, including operations
//! whose IDs already exist and would otherwise be silently ignored by the CRDT.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use loro::{
    Container, ContainerTrait, JsonMapOp, JsonOpContent, JsonSchema, LoroMap, LoroText, LoroValue,
    ValueOrContainer,
};
use sha2::{Digest, Sha256};
use types::{PlanDoc, PlanLine, PlanSection};

use crate::{Frontiers, Page, VersionVector};

mod codec;
mod edit;
pub use codec::{MAX_PLAN_ATOMS, MAX_PLAN_CHANGES, MAX_PLAN_PACKET_BYTES, MAX_PLAN_RAW_OPS};
pub use edit::{PlanBlock, PlanBlockContent, PlanBlockEdit, PlanRevision};

const MIGRATION_PEER: u64 = 0;
const SCHEMA: &str = "plan-experiment-1";
type Result<T> = std::result::Result<T, PlanError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    Invalid(String),
    ReservedSeed,
    IncompatiblePrefix,
    MissingDependencies,
    StaleLine,
    OperationConflict,
    UnsupportedCodec,
    InvalidRevision,
    Limit(&'static str),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => f.write_str(reason),
            Self::ReservedSeed => f.write_str("unverified reserved migration operations"),
            Self::IncompatiblePrefix => f.write_str("legacy history is not an extension"),
            Self::MissingDependencies => f.write_str("plan update has missing dependencies"),
            Self::StaleLine => f.write_str("the expected line changed"),
            Self::OperationConflict => f.write_str("conflicting plan operation ID"),
            Self::UnsupportedCodec => f.write_str("unsupported plan codec"),
            Self::InvalidRevision => {
                f.write_str("plan revision is not the locally observed history")
            }
            Self::Limit(reason) => write!(f, "plan limit: {reason}"),
        }
    }
}

impl std::error::Error for PlanError {}

fn error(e: impl std::fmt::Display) -> PlanError {
    PlanError::Invalid(e.to_string())
}

/// Input already authenticated and ordered by the caller. This library does not verify
/// signatures or select a relay cutover; it never trusts a prefix claimed by an update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyVersion {
    pub event_id: String,
    pub document: PlanDoc,
}

struct SeedInner {
    page: Page,
    history: Vec<LegacyVersion>,
    frontiers: Vec<Frontiers>,
}

/// Immutable, locally verified operation tape. Editors only receive independent forks.
#[derive(Clone)]
pub struct CanonicalPlanSeed(Arc<SeedInner>);

impl CanonicalPlanSeed {
    pub fn replay(history: &[LegacyVersion]) -> Result<Self> {
        if history.is_empty()
            || history.iter().any(|v| v.event_id.is_empty())
            || history
                .iter()
                .map(|v| &v.event_id)
                .collect::<HashSet<_>>()
                .len()
                != history.len()
        {
            return Err(error("a legacy prefix needs distinct, nonempty event IDs"));
        }
        let page = Page::new();
        page.doc.set_peer_id(MIGRATION_PEER).map_err(error)?;
        page.doc.set_record_timestamp(false);
        let root = &history[0].event_id;
        let mut frontiers = Vec::with_capacity(history.len());
        for (i, version) in history.iter().enumerate() {
            // Distinct messages fix commit boundaries even when all timestamps are zero.
            page.doc
                .set_next_commit_message(&format!("plan-legacy-1:{}", version.event_id));
            page.doc.set_next_commit_timestamp(0);
            if i == 0 {
                let meta = page.doc.get_map("page");
                meta.insert("schema", SCHEMA).map_err(error)?;
                meta.insert("root", root.as_str()).map_err(error)?;
            }
            replay_version(&page, root, &version.document)?;
            page.doc.commit();
            read_nodes(&page, root)?;
            frontiers.push(page.frontiers());
        }
        Ok(Self(Arc::new(SeedInner {
            page,
            history: history.to_vec(),
            frontiers,
        })))
    }

    pub fn open_editor(&self) -> Result<PlanDocument> {
        Ok(PlanDocument {
            page: editor_fork(&self.0.page)?,
            seed: self.clone(),
        })
    }

    pub fn legacy_at(&self, version: usize) -> Option<&LegacyVersion> {
        version.checked_sub(1).and_then(|i| self.0.history.get(i))
    }

    pub fn legacy_frontiers(&self, version: usize) -> Option<&Frontiers> {
        version.checked_sub(1).and_then(|i| self.0.frontiers.get(i))
    }

    fn root(&self) -> &str {
        &self.0.history[0].event_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanVersion(VersionVector);

/// Opaque inspectable operations with a bounded, explicitly versioned JSON codec.
/// No production PageContent caller is activated. Ordinary Page binary payloads are
/// unsupported here; raw original operations are inspected before collision-eliding import.
#[derive(Debug, Clone)]
pub struct PlanUpdate(JsonSchema);

impl PlanUpdate {
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        codec::decode(bytes)
    }

    pub fn to_json(&self) -> Result<Vec<u8>> {
        codec::encode(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotalCents {
    Cents(i64),
    Overflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineView {
    pub block_id: String,
    pub line: PlanLine,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionView {
    /// None is a derived recovery group, never a persisted section or automatic write.
    pub block_id: Option<String>,
    pub title: String,
    pub recovered: bool,
    pub lines: Vec<LineView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanView {
    pub title: String,
    pub summary: String,
    pub budget_cents: Option<i64>,
    pub sections: Vec<SectionView>,
    pub total: TotalCents,
}

impl PlanView {
    /// Derived compatibility projection. Canonical prose and rich-text marks remain in
    /// PlanDocument::blocks; this value is never an independent editable document.
    pub fn to_plan_doc(&self) -> PlanDoc {
        PlanDoc {
            title: self.title.clone(),
            summary: self.summary.clone(),
            budget_cents: self.budget_cents,
            sections: self
                .sections
                .iter()
                .map(|s| PlanSection {
                    title: s.title.clone(),
                    lines: s.lines.iter().map(|l| l.line.clone()).collect(),
                })
                .collect(),
        }
    }
}

/// Comparison is over exact text/cents, matching the existing approval hash. The FFI
/// must still authorize actor/grant/proposal; AnyCurrent grants no agent authority.
#[derive(Debug, Clone)]
pub enum ExpectedLine {
    AnyCurrent,
    Hash(String),
}

#[derive(Debug, Clone, Default)]
pub struct LinePatch {
    pub text: Option<String>,
    pub cost_cents: Option<i64>,
    pub done: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum PlanCommand {
    SetTitle(String),
    SetSummary(String),
    SetBudget(Option<i64>),
    ChangeLine {
        block: String,
        expected: ExpectedLine,
        patch: LinePatch,
    },
    RemoveLine {
        block: String,
        expected: ExpectedLine,
    },
    InsertLine {
        section: String,
        text: String,
        cost_cents: i64,
    },
    AddSection {
        title: String,
    },
    RenameSection {
        block: String,
        title: String,
    },
    RemoveSection {
        block: String,
    },
    MoveAfter {
        block: String,
        after: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    Unchanged,
    Changed,
    Created(String),
}

/// A single canonical typed document. Page's permissive block decoder is inaccessible.
pub struct PlanDocument {
    page: Page,
    seed: CanonicalPlanSeed,
}

impl PlanDocument {
    pub fn version(&self) -> PlanVersion {
        PlanVersion(self.page.version())
    }

    pub fn view(&self) -> Result<PlanView> {
        project(&self.page, self.seed.root())
    }

    pub fn fork(&self) -> Result<Self> {
        Ok(Self {
            page: editor_fork(&self.page)?,
            seed: self.seed.clone(),
        })
    }

    pub fn updates_since(&self, from: &PlanVersion) -> Result<PlanUpdate> {
        let current = self.page.version();
        if !current.includes_vv(&from.0) {
            return Err(PlanError::MissingDependencies);
        }
        Ok(PlanUpdate(
            self.page
                .doc
                .export_json_updates_without_peer_compression(&from.0, &current),
        ))
    }

    pub fn full_update(&self) -> PlanUpdate {
        PlanUpdate(self.page.doc.export_json_updates_without_peer_compression(
            &VersionVector::default(),
            &self.page.version(),
        ))
    }

    /// Only an independently authenticated extension can enlarge the allowed tape. A
    /// production cutover owner must stop calling this after its first accepted Page event.
    pub fn admit_legacy_prefix(&mut self, seed: &CanonicalPlanSeed) -> Result<()> {
        if !seed.0.history.starts_with(&self.seed.0.history) {
            return Err(PlanError::IncompatiblePrefix);
        }
        let update = PlanUpdate(
            seed.0
                .page
                .doc
                .export_json_updates_without_peer_compression(
                    &self.seed.0.page.version(),
                    &seed.0.page.version(),
                ),
        );
        let staged = self.import_staged(&update, seed)?;
        self.page = staged;
        self.seed = seed.clone();
        Ok(())
    }

    pub fn import(&mut self, update: &PlanUpdate) -> Result<()> {
        let staged = self.import_staged(update, &self.seed)?;
        self.page = staged;
        Ok(())
    }

    fn import_staged(&self, update: &PlanUpdate, seed: &CanonicalPlanSeed) -> Result<Page> {
        codec::validate(&self.page, update, seed)?;
        validate_identity_operations(&self.page, update)?;
        edit::validate_operations(&self.page, update)?;
        let staged = editor_fork(&self.page)?;
        let status = staged
            .doc
            .import_json_updates(update.0.clone())
            .map_err(error)?;
        if status.pending.is_some_and(|p| !p.is_empty())
            || update
                .0
                .start_version
                .iter()
                .any(|id| !staged.version().includes_id(id))
        {
            return Err(PlanError::MissingDependencies);
        }
        read_nodes(&staged, seed.root())?;
        edit::validate_touched_maps(&staged, update)?;
        Ok(staged)
    }

    pub fn apply(&mut self, command: PlanCommand) -> Result<CommandResult> {
        let staged = editor_fork(&self.page)?;
        let nodes = read_nodes(&staged, self.seed.root())?;
        let from = staged.version();
        let created = apply_command(&staged, &nodes, command)?;
        staged.doc.commit();
        read_nodes(&staged, self.seed.root())?;
        if staged.version() == from {
            return Ok(CommandResult::Unchanged);
        }
        let update = PlanUpdate(
            staged
                .doc
                .export_json_updates_without_peer_compression(&from, &staged.version()),
        );
        self.page = self.import_staged(&update, &self.seed)?;
        Ok(created.map_or(CommandResult::Changed, CommandResult::Created))
    }
}

fn editor_fork(page: &Page) -> Result<Page> {
    let doc = page.doc.fork();
    // Loro normally assigns a new random peer to a fork. Exclude the reserved peer
    // explicitly rather than relying on its tiny random collision probability.
    if doc.peer_id() == MIGRATION_PEER {
        loop {
            let mut bytes = [0; 8];
            getrandom::getrandom(&mut bytes).map_err(error)?;
            let peer = u64::from_le_bytes(bytes);
            if peer != MIGRATION_PEER && peer != u64::MAX {
                doc.set_peer_id(peer).map_err(error)?;
                break;
            }
        }
    }
    doc.set_record_timestamp(false);
    Ok(Page { doc })
}

fn immutable_key(key: &str) -> bool {
    matches!(key, "id" | "kind" | "line_id" | "text" | "schema" | "root")
}

// Covered operations have already passed exact collision validation. Inspect new identity
// writes against retained map state, including blocks removed from the visible list, so
// validation does not export the document's entire history on every bounded packet.
fn validate_identity_operations(page: &Page, update: &PlanUpdate) -> Result<()> {
    let mut known = HashMap::new();
    let vv = page.version();
    for change in &update.0.changes {
        for op in &change.ops {
            let JsonOpContent::Map(map) = &op.content else {
                continue;
            };
            let key = match map {
                JsonMapOp::Insert { key, .. } | JsonMapOp::Delete { key } => key,
            };
            if !immutable_key(key) {
                continue;
            }
            if op.counter < *vv.get(&change.id.peer).unwrap_or(&0) {
                continue;
            }
            let address = (op.container.clone(), key.clone());
            let witness = (
                change.id.peer,
                op.counter,
                serde_json::to_value(op).map_err(error)?,
            );
            if let Some(prior) = known.get(&address) {
                if prior != &witness {
                    return Err(error("immutable plan metadata changed"));
                }
            } else {
                if matches!(map, JsonMapOp::Delete { .. }) {
                    return Err(error("immutable plan metadata deleted"));
                }
                if page
                    .doc
                    .try_get_map(op.container.clone())
                    .is_some_and(|m| m.get(key).is_some())
                {
                    return Err(error("immutable plan metadata changed"));
                }
                known.insert(address, witness);
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Title,
    Summary,
    Section,
    Line,
    Prose,
}

impl BlockKind {
    fn id(self) -> &'static str {
        match self {
            Self::Title => "plan_title",
            Self::Summary => "plan_summary",
            Self::Section => "plan_section",
            Self::Line => "plan_line",
            Self::Prose => "plan_prose",
        }
    }
}

struct Node {
    id: String,
    kind: BlockKind,
    map: LoroMap,
    text: LoroText,
}

mod projection;
use projection::{
    budget, integer, line, line_hash, parse_role, project, read_node, read_nodes, role_key, string,
};

mod replay;
use replay::{initialize_block, replay_version, stable_id};

mod commands;
use commands::{
    apply_command, block_map, block_position, new_block_id, set_budget, set_text, text_container,
};

#[cfg(test)]
mod tests;
