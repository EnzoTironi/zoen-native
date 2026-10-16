//! Authenticated Plan timeline for the shared document session.
//! Public Plan Page/edit admission remains closed until its consumers and clients migrate.
//!
//! Logs own the ordered facts; the outbox owns unconfirmed intent. This adapter builds
//! a disposable typed projection and never writes either one or a second Plan store.

use base64::Engine as _;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
use zoen_docs::plan::{
    CanonicalPlanSeed, LegacyVersion, PlanDocument, PlanVersion, TotalCents, MAX_PLAN_PACKET_BYTES,
};
use zoen_types::*;

use crate::engine::{Engine, R};
use crate::CoreError;

mod api;
mod confirmed;
pub(crate) mod projection;
pub use api::{
    PlanPageBlockDto, PlanPageContentDto, PlanPageDto, PlanPageReadSourceDto, PlanProseKindDto,
};

const MANIFEST_SCHEMA: &str = "plan_v1";

pub(crate) struct PlanSnapshot {
    pub document: PlanDoc,
    pub total_cents: Option<i64>,
    pub version: u32,
    pub author: String,
    pub at_ms: i64,
    pub text: String,
}

pub(crate) fn checked_total(document: &PlanDoc) -> Option<i64> {
    document
        .sections
        .iter()
        .flat_map(|section| &section.lines)
        .try_fold(0_i128, |total, line| {
            total.checked_add(i128::from(line.cost_cents))
        })
        .and_then(|total| i64::try_from(total).ok())
}

impl PlanSnapshot {
    pub(super) fn legacy(document: &PlanDoc, version: &crate::engine::Version) -> Self {
        let mut text = vec![document.title.clone(), document.summary.clone()];
        for section in &document.sections {
            text.push(section.title.clone());
            text.extend(section.lines.iter().map(|line| line.text.clone()));
        }
        Self {
            document: document.clone(),
            total_cents: checked_total(document),
            version: version.number,
            author: version.author.clone(),
            at_ms: version.at_ms,
            text: text.join("\n"),
        }
    }

    pub(super) fn canonical(document: &PlanDocument, version: u32, source: &Source) -> R<Self> {
        let view = document
            .view()
            .map_err(|error| invalid(&error.to_string()))?;
        let total_cents = match view.total {
            TotalCents::Cents(cents) => Some(cents),
            TotalCents::Overflow => None,
        };
        let text = document
            .blocks()
            .map_err(|error| invalid(&error.to_string()))?
            .iter()
            .map(|block| block.plain_text())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Self {
            document: view.to_plan_doc(),
            total_cents,
            version,
            author: source.author.clone(),
            at_ms: source.at_ms,
            text,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Source {
    /// Scoped signed-content digest. Sequence/hash can change when an intent is confirmed.
    pub event_id: String,
    pub client_id: String,
    pub author: String,
    pub at_ms: i64,
    pub ordered: Option<(u64, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Acceptance {
    Legacy,
    Canonical(PlanVersion),
    Refused(String),
    Waiting(String),
    Pending,
    TransportRefused(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Slot {
    /// An invalid undo has no historical version number, but retains its source.
    pub number: Option<u32>,
    pub source: Source,
    /// Exact content, including late incompatible writes, for read-only history.
    pub content: Option<ItemContent>,
    pub reverted_to: Option<u32>,
    pub acceptance: Acceptance,
}

pub(super) struct PlanFold {
    pub slots: Vec<Slot>,
    pub document: Option<PlanDocument>,
    pub frozen_at: Option<Source>,
    pub accepted_current: Option<u32>,
    pub waiting: bool,
    pub pending_legacy: bool,
    manifest: Option<PlanManifestV1>,
    prefixes: Vec<PlanManifestV1>,
}

impl PlanFold {
    /// The session's shadow owns the verified document; this copy retains only history.
    pub(super) fn metadata(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            document: None,
            frozen_at: self.frozen_at.clone(),
            accepted_current: self.accepted_current,
            waiting: self.waiting,
            pending_legacy: self.pending_legacy,
            manifest: self.manifest.clone(),
            prefixes: self.prefixes.clone(),
        }
    }

    /// These facts come only from the authenticated ordered creation and legacy tape.
    pub fn origin_tag(&self) -> Option<(&str, &str, &str)> {
        self.prefixes.first().map(|origin| {
            (
                origin.space.as_str(),
                origin.item.as_str(),
                origin.creation_event.as_str(),
            )
        })
    }

    pub fn accepts_prefix(&self, prefix: &PlanManifestV1) -> bool {
        self.prefixes.iter().any(|verified| verified == prefix)
    }

    /// A pending legacy snapshot/undo never enters the tape, even if it was signed here.
    pub fn writable(&self) -> bool {
        self.document.is_some() && !self.waiting && !self.pending_legacy
    }

    pub fn manifest(&self) -> R<PlanManifestV1> {
        if !self.writable() {
            return Err(invalid("the authenticated Plan prefix is not ready"));
        }
        self.manifest
            .clone()
            .ok_or_else(|| invalid("the Plan has no authenticated legacy creation"))
    }

    /// The journal must bind its expected event source, not merely a reused client ID.
    /// Transport confirmation alone does not establish semantic acceptance.
    pub fn acknowledges(&self, client_id: &str, event_id: &str) -> bool {
        self.slots.iter().any(|slot| {
            slot.source.client_id == client_id
                && slot.source.event_id == event_id
                && slot.source.ordered.is_some()
                && matches!(slot.acceptance, Acceptance::Canonical(_))
        })
    }
}

fn invalid(reason: &str) -> CoreError {
    CoreError::Invalid {
        reason: reason.into(),
    }
}

pub(super) fn source(space: &str, item: &str, event: &Event, ordered: bool) -> Source {
    let mut digest = Sha256::new();
    digest.update(b"zoen.plan.timeline.source.v1\0");
    digest_field(&mut digest, space.as_bytes());
    digest_field(&mut digest, item.as_bytes());
    digest_field(&mut digest, &event.content);
    Source {
        event_id: hex::encode(digest.finalize()),
        client_id: event.client_id.clone(),
        author: event.author.clone(),
        at_ms: event.at_ms,
        ordered: ordered.then(|| (event.seq, event.hash.clone())),
    }
}

fn digest_field(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

pub(super) fn item_event(event: &Event, item: &str) -> bool {
    match &event.body {
        EventBody::ItemCreated { item: id, .. }
        | EventBody::ItemVersioned { item: id, .. }
        | EventBody::ItemReverted { item: id, .. } => id == item,
        _ => false,
    }
}

fn legacy_intent(event: &Event) -> bool {
    matches!(
        event.body,
        EventBody::ItemCreated {
            content: ItemContent::Plan(_),
            ..
        } | EventBody::ItemVersioned {
            content: ItemContent::Plan(_),
            ..
        } | EventBody::ItemReverted { .. }
    )
}

fn event_content(event: &Event, slots: &[Slot]) -> (Option<ItemContent>, Option<u32>) {
    match &event.body {
        EventBody::ItemCreated { content, .. } | EventBody::ItemVersioned { content, .. } => {
            (Some(content.clone()), None)
        }
        EventBody::ItemReverted { to_version, .. } => (
            slots
                .iter()
                .find(|slot| slot.number == Some(*to_version))
                .and_then(|slot| slot.content.clone()),
            Some(*to_version),
        ),
        _ => (None, None),
    }
}

#[derive(Clone)]
enum PayloadRead {
    Ready(Vec<u8>),
    Waiting(String),
    Malformed(String),
}

fn hash_address(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

struct Tape {
    space: String,
    item: String,
    origin: Option<Source>,
    history: Vec<LegacyVersion>,
    prefixes: Vec<PlanManifestV1>,
    digest: Sha256,
}

impl Tape {
    fn new(space: &str, item: &str) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"zoen.plan.legacy.prefix.v1\0");
        digest_field(&mut digest, space.as_bytes());
        digest_field(&mut digest, item.as_bytes());
        Self {
            space: space.into(),
            item: item.into(),
            origin: None,
            history: Vec::new(),
            prefixes: Vec::new(),
            digest,
        }
    }

    fn push(&mut self, event: &Event, source: &Source, document: &PlanDoc) -> R<()> {
        if self.origin.is_none() {
            digest_field(&mut self.digest, source.event_id.as_bytes());
            self.origin = Some(source.clone());
        }
        // Raw authenticated source bytes bind the resolved undo as well as ordinary
        // whole-document writes. Neither their spelling nor IDs are reconstructed.
        digest_field(&mut self.digest, &event.content);
        self.history.push(LegacyVersion {
            event_id: source.event_id.clone(),
            document: document.clone(),
        });
        let count = u32::try_from(self.history.len())
            .map_err(|_| invalid("Plan history is too large; nothing was discarded"))?;
        self.prefixes.push(PlanManifestV1 {
            schema: MANIFEST_SCHEMA.into(),
            space: self.space.clone(),
            item: self.item.clone(),
            creation_event: self.origin.as_ref().expect("set above").event_id.clone(),
            legacy_versions: count,
            legacy_head: source.event_id.clone(),
            prefix_sha256: hex::encode(self.digest.clone().finalize()),
        });
        Ok(())
    }

    fn validate_manifest(&self, content: &PageContent) -> R<()> {
        let Some(PageManifest::PlanV1(manifest)) = &content.manifest else {
            return Err(invalid("a Plan requires the supported PlanV1 manifest"));
        };
        let expected = manifest
            .legacy_versions
            .checked_sub(1)
            .and_then(|i| self.prefixes.get(i as usize));
        if expected != Some(manifest) {
            return Err(invalid(
                "Plan ancestry is not an exact authenticated local prefix",
            ));
        }
        Ok(())
    }

    fn document(&self) -> R<PlanDocument> {
        CanonicalPlanSeed::replay(&self.history)
            .and_then(|seed| seed.open_editor())
            .map_err(|error| invalid(&error.to_string()))
    }
}

fn read_plan_payload(store: &zoen_store::Store, page: &PageContent) -> R<PayloadRead> {
    if let Some(blob) = &page.blob {
        if !page.loro.is_empty()
            || blob.bytes > MAX_PLAN_PACKET_BYTES as u64
            || !hash_address(&blob.sha256)
            || !hash_address(&blob.blob)
            || !hash_address(&blob.key)
        {
            return Ok(PayloadRead::Malformed(
                "invalid Plan blob descriptor".into(),
            ));
        }
        let bytes: Option<Vec<u8>> = store
            .conn()
            .query_row(
                "SELECT bytes FROM media WHERE sha256 = ?1 AND length(bytes) = ?2",
                rusqlite::params![blob.sha256, blob.bytes],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| CoreError::Storage {
                message: error.to_string(),
            })?;
        return Ok(match bytes {
            Some(bytes) if hex::encode(Sha256::digest(&bytes)) == blob.sha256 => {
                PayloadRead::Ready(bytes)
            }
            _ => {
                PayloadRead::Waiting("the Plan blob is missing or has damaged cached bytes".into())
            }
        });
    }
    if page.loro.len() > MAX_PLAN_PACKET_BYTES.div_ceil(3) * 4 {
        return Ok(PayloadRead::Malformed(
            "Plan payload exceeds the packet limit; nothing was discarded".into(),
        ));
    }
    Ok(
        match base64::engine::general_purpose::STANDARD.decode(&page.loro) {
            Ok(bytes) if bytes.len() <= MAX_PLAN_PACKET_BYTES => PayloadRead::Ready(bytes),
            _ => PayloadRead::Malformed("invalid or oversized inline Plan payload".into()),
        },
    )
}

impl Engine {
    pub(crate) fn plan_snapshot_in_space(&self, space: &str, item: &str) -> R<PlanSnapshot> {
        let scoped = self
            .space_projections
            .get(space)
            .ok_or_else(|| invalid("the Space projection is unavailable"))?;
        let it = scoped
            .items
            .get(item)
            .filter(|it| it.space == space && it.kind == ItemKind::Plan)
            .ok_or_else(|| invalid("the scoped Item is not a Plan"))?;
        let Some(projection::PlanOrigin::Explicit(origin)) = scoped.plan_origins.get(item) else {
            return match (&it.current().content, scoped.plan_origins.get(item)) {
                (ItemContent::Plan(document), Some(projection::PlanOrigin::Proposal)) => {
                    Ok(PlanSnapshot::legacy(document, it.current()))
                }
                _ => Err(invalid("the scoped Plan has no confirmed origin")),
            };
        };
        let log = self.logs.get(space);
        let (fold, _) = confirmed::Prefix::from_log(space, log, log.map_or(0, |log| log.len()))?
            .fold(item, |page| self.plan_payload(page))?;
        if fold.waiting
            || fold
                .origin_tag()
                .is_none_or(|(_, _, creation)| creation != origin)
        {
            return Err(invalid("the scoped Plan history is unavailable"));
        }
        if fold.frozen_at.is_none() {
            if let ItemContent::Plan(document) = &it.current().content {
                return Ok(PlanSnapshot::legacy(document, it.current()));
            }
        }
        let version = fold
            .accepted_current
            .ok_or_else(|| invalid("the scoped Plan has no accepted version"))?;
        let source = &fold
            .slots
            .iter()
            .find(|slot| slot.number == Some(version))
            .ok_or_else(|| invalid("the scoped Plan version has no authenticated source"))?
            .source;
        PlanSnapshot::canonical(
            fold.document
                .as_ref()
                .ok_or_else(|| invalid("the scoped Plan document is unavailable"))?,
            version,
            source,
        )
    }

    /// A malformed signed payload is a refusal. Missing or damaged local media is
    /// a waiting condition, and an infrastructure error leaves the fold unchanged.
    /// Check cached length before allocating a payload already subject to codec caps.
    fn plan_payload(&self, page: &PageContent) -> R<PayloadRead> {
        read_plan_payload(&self.store, page)
    }

    pub(super) fn plan_timeline(&self, item: &str) -> R<PlanFold> {
        let it = self
            .state
            .items
            .get(item)
            .ok_or_else(|| CoreError::NotFound {
                what: "Item".into(),
            })?;
        if it.kind != ItemKind::Plan {
            return Err(invalid("this Item is not a Plan"));
        }
        // A mutable projection must not select a newly recreated origin in another
        // Space. There is no global-time election for colliding item identities.
        if self.logs.values().any(|log| {
            log.events().iter().any(|event| {
                event.space != it.space
                    && matches!(&event.body, EventBody::ItemCreated { item: id, .. } if id == item)
            })
        }) {
            return Err(invalid("the authenticated Item origin is ambiguous"));
        }
        let log = self.logs.get(&it.space);
        let end = log.map_or(0, |log| log.len());
        let pending = self.store.outbox()?;
        let prefix = confirmed::Prefix::from_log(&it.space, log, end)?;
        let (mut fold, mut version_number) = prefix.fold(item, |page| self.plan_payload(page))?;

        // No pending source enters the verified tape or freezes its ancestry. Failed
        // intent also remains readable, but no longer blocks a future promotion.
        for pending in pending {
            let event = &pending.event;
            if event.space != it.space || !item_event(event, item) {
                continue;
            }
            let source = source(&it.space, item, event, false);
            if fold.slots.iter().any(|slot| {
                slot.source.ordered.is_some()
                    && slot.source.client_id == source.client_id
                    && slot.source.event_id == source.event_id
            }) {
                continue;
            }
            let (content, reverted_to) = event_content(event, &fold.slots);
            let number = if reverted_to.is_none() || content.is_some() {
                version_number = version_number
                    .checked_add(1)
                    .ok_or_else(|| invalid("Plan history is too large; nothing was discarded"))?;
                Some(version_number)
            } else {
                None
            };
            fold.pending_legacy |= !pending.failed && legacy_intent(event);
            let acceptance = if pending.failed {
                Acceptance::TransportRefused(
                    pending
                        .last_error
                        .unwrap_or_else(|| "the relay refused this intent".into()),
                )
            } else {
                Acceptance::Pending
            };
            fold.slots.push(Slot {
                number,
                source,
                content,
                reverted_to,
                acceptance,
            });
        }
        Ok(fold)
    }
}

#[cfg(test)]
mod tests;
