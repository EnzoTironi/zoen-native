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

fn string(map: &LoroMap, key: &str) -> Result<String> {
    match map.get(key) {
        Some(ValueOrContainer::Value(LoroValue::String(s))) => Ok(s.to_string()),
        _ => Err(error(format!("missing or invalid plan field {key}"))),
    }
}

fn integer(map: &LoroMap, key: &str) -> Result<i64> {
    match map.get(key) {
        Some(ValueOrContainer::Value(LoroValue::I64(n))) => Ok(n),
        _ => Err(error(format!("missing or invalid integer {key}"))),
    }
}

fn budget(map: &LoroMap) -> Result<Option<i64>> {
    match map.get("budget_cents") {
        Some(ValueOrContainer::Value(LoroValue::Null)) => Ok(None),
        Some(ValueOrContainer::Value(LoroValue::I64(n))) => Ok(Some(n)),
        _ => Err(error("missing or invalid budget")),
    }
}

fn line(node: &Node) -> Result<PlanLine> {
    if node.kind != BlockKind::Line {
        return Err(error("not a plan line"));
    }
    let done = match node.map.get("done") {
        Some(ValueOrContainer::Value(LoroValue::Bool(v))) => v,
        _ => return Err(error("missing or invalid done")),
    };
    Ok(PlanLine {
        id: string(&node.map, "line_id")?,
        text: node.text.to_string(),
        cost_cents: integer(&node.map, "cost_cents")?,
        done,
    })
}

fn read_nodes(page: &Page, root: &str) -> Result<Vec<Node>> {
    let meta = page.doc.get_map("page");
    if string(&meta, "schema")? != SCHEMA || string(&meta, "root")? != root {
        return Err(error("plan seed identity mismatch"));
    }
    if meta
        .keys()
        .any(|key| !matches!(key.as_str(), "schema" | "root"))
    {
        return Err(error("unknown plan root attribute"));
    }
    let list = page.blocks_list();
    let mut result = Vec::with_capacity(list.len());
    let mut ids = HashSet::new();
    let mut texts = HashSet::new();
    for i in 0..list.len() {
        let Some(ValueOrContainer::Container(Container::Map(map))) = list.get(i) else {
            return Err(error("plan blocks must be maps"));
        };
        let node = read_node(map)?;
        if !ids.insert(node.id.clone()) || !texts.insert(node.text.id()) {
            return Err(error("invalid or duplicate internal block ID"));
        }
        result.push(node);
    }
    if result.len() < 2
        || result[0].kind != BlockKind::Title
        || result[1].kind != BlockKind::Summary
        || result[0].id != stable_id(root, "title", "", 0)
        || result[1].id != stable_id(root, "summary", "", 0)
        || result[2..]
            .iter()
            .any(|n| matches!(n.kind, BlockKind::Title | BlockKind::Summary))
    {
        return Err(error(
            "plan title and summary anchors must remain unique and first",
        ));
    }
    Ok(result)
}

fn parse_role(value: &str) -> Result<BlockKind> {
    match value {
        "plan_title" => Ok(BlockKind::Title),
        "plan_summary" => Ok(BlockKind::Summary),
        "plan_section" => Ok(BlockKind::Section),
        "plan_line" => Ok(BlockKind::Line),
        "plan_prose" => Ok(BlockKind::Prose),
        _ => Err(error("unsupported typed plan block")),
    }
}

fn role_key(kind: BlockKind, key: &str) -> bool {
    matches!(key, "id" | "kind" | "text")
        || match kind {
            BlockKind::Title => key == "budget_cents",
            BlockKind::Line => matches!(key, "line_id" | "cost_cents" | "done"),
            BlockKind::Prose => key == "prose_kind",
            BlockKind::Summary | BlockKind::Section => false,
        }
}

fn read_node(map: LoroMap) -> Result<Node> {
    let id = string(&map, "id")?;
    if id.trim().is_empty() || id.len() > crate::MAX_BLOCK_ID_BYTES {
        return Err(error("invalid internal block ID"));
    }
    let kind = parse_role(&string(&map, "kind")?)?;
    if map.keys().any(|key| !role_key(kind, key.as_str())) {
        return Err(error("attribute does not belong to its plan block role"));
    }
    let text = text_container(&map)?;
    let node = Node {
        id,
        kind,
        map,
        text,
    };
    match kind {
        BlockKind::Title => {
            budget(&node.map)?;
        }
        BlockKind::Line => {
            line(&node)?;
        }
        BlockKind::Prose => {
            edit::parse_prose_kind(&string(&node.map, "prose_kind")?)?;
        }
        BlockKind::Summary | BlockKind::Section => {}
    }
    edit::read_runs(&node.text)?;
    Ok(node)
}

fn line_hash(line: &PlanLine) -> Result<String> {
    let bytes = serde_json::to_vec(&(&line.text, line.cost_cents)).map_err(error)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn project(page: &Page, root: &str) -> Result<PlanView> {
    let nodes = read_nodes(page, root)?;
    let mut sections: Vec<SectionView> = Vec::new();
    let mut sum = Some(0_i128);
    for node in &nodes[2..] {
        match node.kind {
            BlockKind::Section => sections.push(SectionView {
                block_id: Some(node.id.clone()),
                title: node.text.to_string(),
                recovered: false,
                lines: Vec::new(),
            }),
            BlockKind::Line => {
                if sections.is_empty() {
                    sections.push(SectionView {
                        block_id: None,
                        title: "Recovered lines".into(),
                        recovered: true,
                        lines: Vec::new(),
                    });
                }
                let line = line(node)?;
                sum = sum.and_then(|n| n.checked_add(i128::from(line.cost_cents)));
                let content_hash = line_hash(&line)?;
                sections
                    .last_mut()
                    .expect("section just created")
                    .lines
                    .push(LineView {
                        block_id: node.id.clone(),
                        line,
                        content_hash,
                    });
            }
            BlockKind::Prose => {}
            _ => return Err(error("unexpected anchor inside plan body")),
        }
    }
    let total = sum
        .and_then(|n| i64::try_from(n).ok())
        .map_or(TotalCents::Overflow, TotalCents::Cents);
    Ok(PlanView {
        title: nodes[0].text.to_string(),
        summary: nodes[1].text.to_string(),
        budget_cents: budget(&nodes[0].map)?,
        sections,
        total,
    })
}

fn stable_id(root: &str, kind: &str, legacy: &str, occurrence: usize) -> String {
    let bytes = serde_json::to_vec(&(root, kind, legacy, occurrence)).expect("strings serialize");
    hex::encode(Sha256::digest(bytes))
}

struct Desired {
    id: String,
    kind: BlockKind,
    text: String,
    line: Option<PlanLine>,
    budget: Option<i64>,
}

fn replay_version(page: &Page, root: &str, plan: &PlanDoc) -> Result<()> {
    let mut desired = vec![
        Desired {
            id: stable_id(root, "title", "", 0),
            kind: BlockKind::Title,
            text: plan.title.clone(),
            line: None,
            budget: plan.budget_cents,
        },
        Desired {
            id: stable_id(root, "summary", "", 0),
            kind: BlockKind::Summary,
            text: plan.summary.clone(),
            line: None,
            budget: None,
        },
    ];
    let mut occurrences: HashMap<&str, usize> = HashMap::new();
    for (i, section) in plan.sections.iter().enumerate() {
        desired.push(Desired {
            id: stable_id(root, "section", "", i),
            kind: BlockKind::Section,
            text: section.title.clone(),
            line: None,
            budget: None,
        });
        for line in &section.lines {
            let occurrence = occurrences.entry(line.id.as_str()).or_default();
            let id = stable_id(root, "line", &line.id, *occurrence);
            *occurrence += 1;
            desired.push(Desired {
                id,
                kind: BlockKind::Line,
                text: line.text.clone(),
                line: Some(line.clone()),
                budget: None,
            });
        }
    }
    let list = page.blocks_list();
    let keep: HashSet<_> = desired.iter().map(|b| b.id.as_str()).collect();
    for i in (0..list.len()).rev() {
        let map = block_map(page, i)?;
        if !keep.contains(string(&map, "id")?.as_str()) {
            list.delete(i, 1).map_err(error)?;
        }
    }
    for (want, target) in desired.iter().enumerate() {
        let current = block_position(page, &target.id)?;
        let map = if let Some(now) = current {
            if now != want {
                list.mov(now, want).map_err(error)?;
            }
            block_map(page, want)?
        } else {
            let map = list.insert_container(want, LoroMap::new()).map_err(error)?;
            initialize_block(
                &map,
                &target.id,
                target.kind,
                &target.text,
                target.line.as_ref(),
                target.budget,
            )?;
            continue;
        };
        let text = text_container(&map)?;
        set_text(&text, &target.text)?;
        match target.kind {
            BlockKind::Title => set_budget(&map, target.budget)?,
            BlockKind::Line => {
                let value = target.line.as_ref().expect("line target");
                if integer(&map, "cost_cents")? != value.cost_cents {
                    map.insert("cost_cents", value.cost_cents).map_err(error)?;
                }
                if !matches!(map.get("done"), Some(ValueOrContainer::Value(LoroValue::Bool(done))) if done == value.done)
                {
                    map.insert("done", value.done).map_err(error)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn initialize_block(
    map: &LoroMap,
    id: &str,
    kind: BlockKind,
    text: &str,
    line: Option<&PlanLine>,
    budget: Option<i64>,
) -> Result<()> {
    map.insert("id", id).map_err(error)?;
    map.insert("kind", kind.id()).map_err(error)?;
    let rich = map
        .insert_container("text", LoroText::new())
        .map_err(error)?;
    if !text.is_empty() {
        rich.insert(0, text).map_err(error)?;
    }
    if kind == BlockKind::Title {
        map.insert(
            "budget_cents",
            budget.map_or(LoroValue::Null, LoroValue::I64),
        )
        .map_err(error)?;
    }
    if let Some(line) = line {
        map.insert("line_id", line.id.as_str()).map_err(error)?;
        map.insert("cost_cents", line.cost_cents).map_err(error)?;
        map.insert("done", line.done).map_err(error)?;
    }
    Ok(())
}

fn set_budget(map: &LoroMap, value: Option<i64>) -> Result<()> {
    if budget(map)? != value {
        map.insert(
            "budget_cents",
            value.map_or(LoroValue::Null, LoroValue::I64),
        )
        .map_err(error)?;
    }
    Ok(())
}

fn block_map(page: &Page, index: usize) -> Result<LoroMap> {
    match page.blocks_list().get(index) {
        Some(ValueOrContainer::Container(Container::Map(m))) => Ok(m),
        _ => Err(error("invalid block map")),
    }
}

fn block_position(page: &Page, id: &str) -> Result<Option<usize>> {
    for i in 0..page.blocks_list().len() {
        if string(&block_map(page, i)?, "id")? == id {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

fn text_container(map: &LoroMap) -> Result<LoroText> {
    match map.get("text") {
        Some(ValueOrContainer::Container(Container::Text(t))) => Ok(t),
        _ => Err(error("invalid rich text container")),
    }
}

// Deterministic scalar-index edit, preserving source bytes including CRLF/U+2028.
fn set_text(text: &LoroText, target: &str) -> Result<()> {
    let before: Vec<_> = text.to_string().chars().collect();
    let after: Vec<_> = target.chars().collect();
    let prefix = before
        .iter()
        .zip(&after)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let remove = before.len() - prefix - suffix;
    if remove > 0 {
        text.delete(prefix, remove).map_err(error)?;
    }
    let add: String = after[prefix..after.len() - suffix].iter().collect();
    if !add.is_empty() {
        text.insert(prefix, &add).map_err(error)?;
    }
    Ok(())
}

fn find<'a>(nodes: &'a [Node], block: &str) -> Result<&'a Node> {
    nodes
        .iter()
        .find(|n| n.id == block)
        .ok_or_else(|| error("plan block not found"))
}

fn check_line(node: &Node, expected: &ExpectedLine) -> Result<PlanLine> {
    let value = line(node)?;
    if let ExpectedLine::Hash(hash) = expected {
        if line_hash(&value)? != *hash {
            return Err(PlanError::StaleLine);
        }
    }
    Ok(value)
}

fn new_block_id() -> Result<String> {
    let mut bytes = [0; 16];
    getrandom::getrandom(&mut bytes).map_err(error)?;
    Ok(hex::encode(bytes))
}

fn apply_command(page: &Page, nodes: &[Node], command: PlanCommand) -> Result<Option<String>> {
    let list = page.blocks_list();
    match command {
        PlanCommand::SetTitle(value) => set_text(&nodes[0].text, &value)?,
        PlanCommand::SetSummary(value) => set_text(&nodes[1].text, &value)?,
        PlanCommand::SetBudget(value) => {
            if value.is_some_and(|n| n < 0) {
                return Err(error("new budget cannot be negative"));
            }
            set_budget(&nodes[0].map, value)?;
        }
        PlanCommand::ChangeLine {
            block,
            expected,
            patch,
        } => {
            let node = find(nodes, &block)?;
            let previous = check_line(node, &expected)?;
            if let Some(cost) = patch.cost_cents {
                if cost < 0 {
                    return Err(error("new cost cannot be negative"));
                }
                if cost != previous.cost_cents {
                    node.map.insert("cost_cents", cost).map_err(error)?;
                }
            }
            if let Some(done) = patch.done {
                if done != previous.done {
                    node.map.insert("done", done).map_err(error)?;
                }
            }
            if let Some(text) = patch.text {
                set_text(&node.text, &text)?;
            }
        }
        PlanCommand::RemoveLine { block, expected } => {
            check_line(find(nodes, &block)?, &expected)?;
            let at = block_position(page, &block)?.ok_or_else(|| error("line not found"))?;
            list.delete(at, 1).map_err(error)?;
        }
        PlanCommand::InsertLine {
            section,
            text,
            cost_cents,
        } => {
            if cost_cents < 0 {
                return Err(error("new cost cannot be negative"));
            }
            if find(nodes, &section)?.kind != BlockKind::Section {
                return Err(error("not a section"));
            }
            let start = nodes
                .iter()
                .position(|n| n.id == section)
                .expect("section found");
            let at = nodes[start + 1..]
                .iter()
                .position(|n| n.kind == BlockKind::Section)
                .map_or(nodes.len(), |i| start + 1 + i);
            let id = edit::unused_id(page)?;
            let line = PlanLine {
                id: format!("ln_{id}"),
                text: text.clone(),
                cost_cents,
                done: false,
            };
            let map = list.insert_container(at, LoroMap::new()).map_err(error)?;
            initialize_block(&map, &id, BlockKind::Line, &text, Some(&line), None)?;
            return Ok(Some(id));
        }
        PlanCommand::AddSection { title } => {
            let id = edit::unused_id(page)?;
            let map = list
                .insert_container(list.len(), LoroMap::new())
                .map_err(error)?;
            initialize_block(&map, &id, BlockKind::Section, &title, None, None)?;
            return Ok(Some(id));
        }
        PlanCommand::RenameSection { block, title } => {
            let node = find(nodes, &block)?;
            if node.kind != BlockKind::Section {
                return Err(error("not a section"));
            }
            set_text(&node.text, &title)?;
        }
        PlanCommand::RemoveSection { block } => {
            if find(nodes, &block)?.kind != BlockKind::Section {
                return Err(error("not a section"));
            }
            let start = nodes
                .iter()
                .position(|n| n.id == block)
                .expect("section found");
            let end = nodes[start + 1..]
                .iter()
                .position(|n| n.kind == BlockKind::Section)
                .map_or(nodes.len(), |i| start + 1 + i);
            list.delete(start, end - start).map_err(error)?;
        }
        PlanCommand::MoveAfter { block, after } => {
            let from = nodes
                .iter()
                .position(|n| n.id == block)
                .ok_or_else(|| error("block not found"))?;
            let anchor = nodes
                .iter()
                .position(|n| n.id == after)
                .ok_or_else(|| error("anchor not found"))?;
            if from < 2 || anchor < 1 {
                return Err(error("title/summary anchors cannot move"));
            }
            if from != anchor {
                list.mov(from, if from < anchor { anchor } else { anchor + 1 })
                    .map_err(error)?;
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original() -> PlanDoc {
        PlanDoc {
            title: "Trip".into(),
            summary: "One\nline".into(),
            budget_cents: Some(500),
            sections: vec![PlanSection {
                title: "Hotel".into(),
                lines: vec![PlanLine {
                    id: "existing-line".into(),
                    text: "Stay".into(),
                    cost_cents: 100,
                    done: false,
                }],
            }],
        }
    }

    fn history(plan: PlanDoc) -> Vec<LegacyVersion> {
        vec![LegacyVersion {
            event_id: "authenticated-creation".into(),
            document: plan,
        }]
    }

    fn seed() -> CanonicalPlanSeed {
        CanonicalPlanSeed::replay(&history(original())).unwrap()
    }

    fn line_block(doc: &PlanDocument) -> String {
        doc.view().unwrap().sections[0].lines[0].block_id.clone()
    }

    fn section_block(doc: &PlanDocument) -> String {
        doc.view().unwrap().sections[0].block_id.clone().unwrap()
    }

    fn change(doc: &mut PlanDocument, patch: LinePatch) {
        let block = line_block(doc);
        doc.apply(PlanCommand::ChangeLine {
            block,
            expected: ExpectedLine::AnyCurrent,
            patch,
        })
        .unwrap();
    }

    fn merge_in_both_orders(
        seed: &CanonicalPlanSeed,
        updates: &[PlanUpdate],
    ) -> (PlanView, PlanView) {
        let mut forward = seed.open_editor().unwrap();
        for update in updates {
            forward.import(update).unwrap();
        }
        let mut reverse = seed.open_editor().unwrap();
        for update in updates.iter().rev() {
            reverse.import(update).unwrap();
        }
        assert_eq!(forward.view().unwrap(), reverse.view().unwrap());
        assert_eq!(forward.version(), reverse.version());
        (forward.view().unwrap(), reverse.view().unwrap())
    }

    fn assert_refused_without_change(
        doc: &mut PlanDocument,
        update: &PlanUpdate,
        expected: PlanError,
    ) {
        let view = doc.view().unwrap();
        let version = doc.version();
        let json = doc.full_update().to_json().unwrap();
        assert_eq!(doc.import(update), Err(expected));
        assert_eq!(doc.view().unwrap(), view);
        assert_eq!(doc.version(), version);
        assert_eq!(doc.full_update().to_json().unwrap(), json);
    }

    #[test]
    fn legacy_ids_extreme_cents_and_exact_utf8_are_not_normalized() {
        let mut plan = original();
        plan.title = "  🚃\r\n\u{2028} ".into();
        plan.summary = "\0two\n\r\u{2028}😀".into();
        plan.budget_cents = Some(i64::MIN);
        plan.sections[0].title = String::new();
        let duplicate_id = "  legacy-id ".repeat(600);
        plan.sections[0].lines = vec![
            PlanLine {
                id: duplicate_id.clone(),
                text: " \r\n😀\u{2028}\0 ".into(),
                cost_cents: i64::MIN,
                done: true,
            },
            PlanLine {
                id: duplicate_id,
                text: String::new(),
                cost_cents: i64::MAX,
                done: false,
            },
            PlanLine {
                id: String::new(),
                text: "empty original ID".into(),
                cost_cents: 0,
                done: false,
            },
        ];
        plan.sections.push(PlanSection {
            title: "Empty".into(),
            lines: Vec::new(),
        });
        let seed = CanonicalPlanSeed::replay(&history(plan.clone())).unwrap();
        let editor = seed.open_editor().unwrap();
        let view = editor.view().unwrap();
        assert_eq!(view.to_plan_doc(), plan);
        assert_eq!(view.total, TotalCents::Cents(-1));
        let ids: Vec<_> = view.sections[0].lines.iter().map(|l| &l.block_id).collect();
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 3);
        assert!(ids.iter().all(|id| id.len() <= 128));
        assert_eq!(seed.legacy_at(1).unwrap().document, plan);
        assert_eq!(
            CanonicalPlanSeed::replay(&history(plan))
                .unwrap()
                .open_editor()
                .unwrap()
                .full_update()
                .to_json()
                .unwrap(),
            editor.full_update().to_json().unwrap()
        );
    }

    #[test]
    fn legacy_prefixes_share_the_exact_tape_and_two_first_edits_survive() {
        let v1 = history(original());
        let mut v2 = original();
        v2.title = "Trip updated".into();
        v2.budget_cents = None;
        v2.sections[0].lines[0].cost_cents = 200;
        let versions = [
            v1.clone(),
            vec![LegacyVersion {
                event_id: "authenticated-second".into(),
                document: v2.clone(),
            }],
        ]
        .concat();
        let old = CanonicalPlanSeed::replay(&v1).unwrap();
        let current = CanonicalPlanSeed::replay(&versions).unwrap();
        assert_eq!(old.legacy_frontiers(1), current.legacy_frontiers(1));
        assert_eq!(old.legacy_at(1), current.legacy_at(1));
        let mut a = old.open_editor().unwrap();
        let mut b = current.open_editor().unwrap();
        assert_eq!(b.view().unwrap().to_plan_doc(), v2);
        change(
            &mut a,
            LinePatch {
                text: Some("Reserved stay".into()),
                ..Default::default()
            },
        );
        change(
            &mut b,
            LinePatch {
                done: Some(true),
                ..Default::default()
            },
        );
        let a_first = a.full_update();
        let b_first = b.full_update();
        a.admit_legacy_prefix(&current).unwrap();
        a.import(&b_first).unwrap();
        b.import(&a_first).unwrap();
        assert_eq!(a.view().unwrap(), b.view().unwrap());
        let (merged, _) = merge_in_both_orders(&current, &[a_first, b_first]);
        let line = &merged.sections[0].lines[0].line;
        assert_eq!(line.id, "existing-line");
        assert_eq!(line.text, "Reserved stay");
        assert_eq!(line.cost_cents, 200);
        assert!(line.done);
        assert_eq!(merged.title, "Trip updated");
        assert_eq!(merged.budget_cents, None);
        assert_eq!(merged.total, TotalCents::Cents(200));
        assert_eq!(current.legacy_at(1).unwrap().document, original());
        assert_eq!(current.legacy_at(2).unwrap().document, v2);
    }

    #[test]
    fn text_cost_and_done_merge_independently_even_when_callers_repeat_old_fields() {
        let seed = seed();
        let mut text = seed.open_editor().unwrap();
        let mut cost = seed.open_editor().unwrap();
        let mut done = seed.open_editor().unwrap();
        change(
            &mut text,
            LinePatch {
                text: Some("Longer stay".into()),
                cost_cents: Some(100),
                done: Some(false),
            },
        );
        change(
            &mut cost,
            LinePatch {
                text: Some("Stay".into()),
                cost_cents: Some(777),
                done: Some(false),
            },
        );
        change(
            &mut done,
            LinePatch {
                text: Some("Stay".into()),
                cost_cents: Some(100),
                done: Some(true),
            },
        );
        let updates = [text.full_update(), cost.full_update(), done.full_update()];
        let peers: Vec<_> = updates
            .iter()
            .flat_map(|u| {
                u.0.changes
                    .iter()
                    .filter(|c| c.id.peer != MIGRATION_PEER)
                    .map(|c| c.id.peer)
            })
            .collect();
        assert_eq!(peers.iter().collect::<HashSet<_>>().len(), 3);
        let (view, _) = merge_in_both_orders(&seed, &updates);
        assert_eq!(view.sections[0].lines.len(), 1);
        assert_eq!(
            view.sections[0].lines[0].line,
            PlanLine {
                id: "existing-line".into(),
                text: "Longer stay".into(),
                cost_cents: 777,
                done: true
            }
        );
        assert_eq!(view.total, TotalCents::Cents(777));
        // A real subsequent exact import is an idempotent operation, not another line.
        text.import(&cost.full_update()).unwrap();
        text.import(&done.full_update()).unwrap();
        let before = text.version();
        text.import(&done.full_update()).unwrap();
        assert_eq!(text.version(), before);
        assert_eq!(text.view().unwrap(), view);
    }

    #[test]
    fn deleting_the_last_section_retains_an_unseen_insert_in_a_visible_recovery_group() {
        let seed = seed();
        let mut delete = seed.open_editor().unwrap();
        let mut insert = seed.open_editor().unwrap();
        delete
            .apply(PlanCommand::RemoveSection {
                block: section_block(&delete),
            })
            .unwrap();
        assert!(delete.view().unwrap().sections.is_empty());
        let CommandResult::Created(inserted) = insert
            .apply(PlanCommand::InsertLine {
                section: section_block(&insert),
                text: "A concurrent train".into(),
                cost_cents: 333,
            })
            .unwrap()
        else {
            panic!("line not inserted");
        };
        let (view, _) = merge_in_both_orders(&seed, &[delete.full_update(), insert.full_update()]);
        assert_eq!(view.sections.len(), 1);
        let recovered = &view.sections[0];
        assert!(recovered.recovered);
        assert_eq!(recovered.block_id, None);
        assert_eq!(recovered.lines.len(), 1);
        assert_eq!(recovered.lines[0].block_id, inserted);
        assert_eq!(recovered.lines[0].line.text, "A concurrent train");
        assert_eq!(recovered.lines[0].line.cost_cents, 333);
        assert_eq!(view.total, TotalCents::Cents(333));
        delete.import(&insert.full_update()).unwrap();
        insert.import(&delete.full_update()).unwrap();
        assert_eq!(delete.view().unwrap(), view);
        assert_eq!(insert.view().unwrap(), view);
    }

    #[test]
    fn a_legitimate_merged_total_can_overflow_without_losing_either_branch() {
        let mut plan = original();
        plan.sections[0].lines[0].cost_cents = 0;
        plan.sections[0].lines.push(PlanLine {
            id: "second".into(),
            text: "Second".into(),
            cost_cents: 0,
            done: false,
        });
        let seed = CanonicalPlanSeed::replay(&history(plan)).unwrap();
        let mut a = seed.open_editor().unwrap();
        let mut b = seed.open_editor().unwrap();
        let lines = a.view().unwrap().sections[0].lines.clone();
        a.apply(PlanCommand::ChangeLine {
            block: lines[0].block_id.clone(),
            expected: ExpectedLine::AnyCurrent,
            patch: LinePatch {
                cost_cents: Some(i64::MAX),
                ..Default::default()
            },
        })
        .unwrap();
        b.apply(PlanCommand::ChangeLine {
            block: lines[1].block_id.clone(),
            expected: ExpectedLine::AnyCurrent,
            patch: LinePatch {
                cost_cents: Some(1),
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(a.view().unwrap().total, TotalCents::Cents(i64::MAX));
        assert_eq!(b.view().unwrap().total, TotalCents::Cents(1));
        let (view, _) = merge_in_both_orders(&seed, &[a.full_update(), b.full_update()]);
        assert_eq!(view.total, TotalCents::Overflow);
        assert_eq!(view.sections[0].lines.len(), 2);
        assert_eq!(view.sections[0].lines[0].line.cost_cents, i64::MAX);
        assert_eq!(view.sections[0].lines[1].line.cost_cents, 1);
        // Exact cancellation is representable even when an intermediate i64 sum is not.
        let mut cancellation = view.to_plan_doc();
        cancellation.sections[0].lines.push(PlanLine {
            id: "legacy-negative".into(),
            text: "Credit".into(),
            cost_cents: -1,
            done: false,
        });
        assert_eq!(
            CanonicalPlanSeed::replay(&history(cancellation))
                .unwrap()
                .open_editor()
                .unwrap()
                .view()
                .unwrap()
                .total,
            TotalCents::Cents(i64::MAX)
        );
    }

    #[test]
    fn forged_known_seed_operation_is_refused_before_a_collision_can_hide_it() {
        let seed = seed();
        let mut doc = seed.open_editor().unwrap();
        let mut packet = doc.full_update();
        let mut changed = false;
        for c in &mut packet.0.changes {
            for op in &mut c.ops {
                if let JsonOpContent::Map(JsonMapOp::Insert { key, value }) = &mut op.content {
                    if key == "cost_cents" {
                        *value = LoroValue::I64(900);
                        changed = true;
                    }
                }
            }
        }
        assert!(changed);
        let packet = PlanUpdate::from_json(&packet.to_json().unwrap()).unwrap();
        assert_refused_without_change(&mut doc, &packet, PlanError::ReservedSeed);
        doc.import(&seed.open_editor().unwrap().full_update())
            .unwrap();
        assert_eq!(doc.view().unwrap().total, TotalCents::Cents(100));
    }

    #[test]
    fn new_operations_from_the_reserved_peer_cannot_extend_the_frozen_seed() {
        let seed = seed();
        let mut doc = seed.open_editor().unwrap();
        // An actual Loro writer, impersonating the reserved peer after the real seed.
        let attacker = editor_fork(&doc.page).unwrap();
        attacker.doc.set_peer_id(MIGRATION_PEER).unwrap();
        block_map(&attacker, 3)
            .unwrap()
            .insert("cost_cents", 900_i64)
            .unwrap();
        attacker.doc.commit();
        let packet = PlanUpdate(attacker.doc.export_json_updates_without_peer_compression(
            &VersionVector::default(),
            &attacker.version(),
        ));
        assert_refused_without_change(&mut doc, &packet, PlanError::ReservedSeed);
    }

    #[test]
    fn mixed_complete_and_missing_dependency_updates_refuse_atomically_then_retry_exactly() {
        let seed = seed();
        let mut author = seed.open_editor().unwrap();
        change(
            &mut author,
            LinePatch {
                text: Some("Booked".into()),
                ..Default::default()
            },
        );
        let prerequisite = author.full_update();
        let first = author.version();
        change(
            &mut author,
            LinePatch {
                cost_cents: Some(700),
                ..Default::default()
            },
        );
        let mut incomplete = author.updates_since(&first).unwrap();
        let mut independent = seed.open_editor().unwrap();
        change(
            &mut independent,
            LinePatch {
                done: Some(true),
                ..Default::default()
            },
        );
        incomplete
            .0
            .changes
            .extend(independent.full_update().0.changes);
        let incomplete = PlanUpdate::from_json(&incomplete.to_json().unwrap()).unwrap();
        let mut receiver = seed.open_editor().unwrap();
        assert_refused_without_change(&mut receiver, &incomplete, PlanError::MissingDependencies);
        receiver.import(&prerequisite).unwrap();
        receiver.import(&incomplete).unwrap();
        let line = &receiver.view().unwrap().sections[0].lines[0].line;
        assert_eq!(line.text, "Booked");
        assert_eq!(line.cost_cents, 700);
        assert!(line.done);
        let version = receiver.version();
        receiver.import(&incomplete).unwrap();
        assert_eq!(receiver.version(), version);
    }

    #[test]
    fn a_claimed_extension_cannot_change_the_authenticated_ancestry() {
        let seed = seed();
        let mut doc = seed.open_editor().unwrap();
        let before = doc.full_update().to_json().unwrap();
        let mut rewritten = original();
        rewritten.sections[0].lines[0].cost_cents = 900;
        let wrong = CanonicalPlanSeed::replay(&history(rewritten)).unwrap();
        assert_eq!(
            doc.admit_legacy_prefix(&wrong),
            Err(PlanError::IncompatiblePrefix)
        );
        assert_eq!(doc.full_update().to_json().unwrap(), before);
    }

    #[test]
    fn unsupported_kinds_do_not_become_paragraphs_and_drop_financial_metadata() {
        let seed = seed();
        let mut doc = seed.open_editor().unwrap();
        let attacker = editor_fork(&doc.page).unwrap();
        block_map(&attacker, 3)
            .unwrap()
            .insert("kind", "paragraph")
            .unwrap();
        attacker.doc.commit();
        let packet = PlanUpdate(attacker.doc.export_json_updates_without_peer_compression(
            &doc.page.version(),
            &attacker.version(),
        ));
        assert_refused_without_change(
            &mut doc,
            &packet,
            error("unsupported or invalid plan attribute"),
        );
    }

    #[test]
    fn approval_hash_survives_promotion_and_done_but_not_a_new_cost() {
        let seed = seed();
        let mut doc = seed.open_editor().unwrap();
        let original_hash = line_hash(&original().sections[0].lines[0]).unwrap();
        assert_eq!(
            doc.view().unwrap().sections[0].lines[0].content_hash,
            original_hash
        );
        let block = line_block(&doc);
        doc.apply(PlanCommand::ChangeLine {
            block: block.clone(),
            expected: ExpectedLine::Hash(original_hash.clone()),
            patch: LinePatch {
                done: Some(true),
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(
            doc.view().unwrap().sections[0].lines[0].content_hash,
            original_hash
        );
        change(
            &mut doc,
            LinePatch {
                cost_cents: Some(200),
                ..Default::default()
            },
        );
        let before = doc.version();
        assert_eq!(
            doc.apply(PlanCommand::ChangeLine {
                block,
                expected: ExpectedLine::Hash(original_hash),
                patch: LinePatch {
                    done: Some(false),
                    ..Default::default()
                }
            }),
            Err(PlanError::StaleLine)
        );
        assert_eq!(doc.version(), before);
        assert!(doc.view().unwrap().sections[0].lines[0].line.done);
    }

    #[test]
    fn moving_a_line_preserves_its_container_and_a_concurrent_text_edit() {
        let seed = seed();
        let mut moved = seed.open_editor().unwrap();
        let mut edited = seed.open_editor().unwrap();
        let original_block = line_block(&moved);
        let CommandResult::Created(new_section) = moved
            .apply(PlanCommand::AddSection {
                title: "Transport".into(),
            })
            .unwrap()
        else {
            panic!("section not created");
        };
        moved
            .apply(PlanCommand::MoveAfter {
                block: original_block.clone(),
                after: new_section.clone(),
            })
            .unwrap();
        change(
            &mut edited,
            LinePatch {
                text: Some("Modified while moving".into()),
                ..Default::default()
            },
        );
        let (view, _) = merge_in_both_orders(&seed, &[moved.full_update(), edited.full_update()]);
        assert!(view.sections[0].lines.is_empty());
        assert_eq!(view.sections[1].block_id.as_ref(), Some(&new_section));
        assert_eq!(view.sections[1].lines[0].block_id, original_block);
        assert_eq!(view.sections[1].lines[0].line.text, "Modified while moving");
        assert_eq!(view.total, TotalCents::Cents(100));
    }
}
