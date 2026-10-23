//! O motor: guarda os logs, mantém a projeção (estado derivado) e aplica as regras.
//!
//! Fluxo de toda escrita: **avaliar Concessões → assinar evento → gravar no SQLite →
//! aplicar na projeção**. Ler é só consultar a projeção. Reabrir o app = reverificar
//! cada log e reprojetar do zero.

use std::collections::{HashMap, HashSet};

use event_log::{content_hash, Author, LogError, Signer, SpaceLog};
use grants::{evaluate, standing_allow_permitted, standing_key, Budget, Decision, Policy, Reason};
use serde_json::{json, Value};
use store::Store;
use types::*;

use crate::apps;
use crate::pages::{BeforeEvent, Currentness, PlanOrigin, PlanReplay, Projection};

use crate::dto::*;
use crate::i18n::{money, t, ts};
use crate::tr;
use crate::CoreError;

mod access_policy;
mod agent_views;
mod app_execution;
mod app_installation;
mod app_permissions;
mod call_transport;
mod calls;
mod cards;
mod inspection;
mod items;
mod labels;
mod persistence;
mod plans;
mod requests;
mod state_items;
mod state_projection;
mod timeline;

use labels::{action_label, LowerFirst};
pub(crate) use labels::{
    event_label, kind_id, kind_label, plan_from_dto, plan_to_dto, reason_label, standing_label,
    trust_dto, trust_from_dto,
};

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// "R$ 1.348" ou "R$ 12,40".
pub fn brl(cents: i64) -> String {
    crate::i18n::money_in(crate::i18n::Lang::Pt, cents)
}

/// Mês civil (UTC) de um instante, como `ano*12 + mês`. Orçamentos são mensais.
fn month_key(ms: i64) -> i64 {
    let days = ms.div_euclid(86_400_000);
    // Algoritmo de Howard Hinnant (days_from_civil inverso).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    y * 12 + m
}

fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' => 'a',
            'é' | 'ê' | 'è' | 'É' | 'Ê' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'ô' | 'õ' | 'Ó' | 'Ô' | 'Õ' => 'o',
            'ú' | 'ü' | 'Ú' => 'u',
            'ç' | 'Ç' => 'c',
            other => other.to_ascii_lowercase(),
        })
        .collect()
}

// ───────────────────────────── projeção ─────────────────────────────

#[derive(Clone, Debug)]
#[allow(dead_code)] // `System` é o seam para avisos do núcleo (entrou, saiu, chave mudou).
pub(crate) enum EntryBody {
    Message {
        text: String,
        attaches: Option<ItemId>,
        /// Inline reply / thread reply link (see `types::reply`).
        reply: Option<types::ReplyRef>,
        /// Edits, delete for everyone and reactions folded over the original.
        marks: MessageMarks,
    },
    ItemEdited {
        item: ItemId,
        version: u32,
        note: String,
        is_undo: bool,
    },
    Request {
        request: RequestId,
    },
    RequestResolved {
        request: RequestId,
        approved: bool,
    },
    System {
        text: String,
    },
    Background {
        spec: BackgroundSpec,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub hash: String,
    /// Stable id from the author's device (survives the relay confirming the event).
    pub client_id: String,
    pub space: SpaceId,
    pub seq: u64,
    pub author: IdentityId,
    pub at_ms: i64,
    pub body: EntryBody,
}

/// What later `MessageAnnotated` events did to a message.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MessageMarks {
    pub edited: bool,
    pub deleted: bool,
    pub forwarded: bool,
    pub media: Option<types::MessageMedia>,
    /// One reaction per person, in the order people first reacted.
    pub reactions: Vec<(IdentityId, String)>,
}

impl MessageMarks {
    /// Folds one annotation from `by` over a message written by `author`. Returns the new
    /// text for an edit. Only the author edits or deletes; a deleted message stays deleted.
    pub(crate) fn apply(
        &mut self,
        author: &str,
        by: &str,
        note: &types::Annotation,
    ) -> Option<String> {
        use types::Annotation as A;
        if self.deleted || (note.author_only() && author != by) {
            return None;
        }
        // A reaction is one emoji (with modifiers); anything longer is a forged event.
        if matches!(note, A::React { emoji } if emoji.is_empty() || emoji.len() > 32) {
            return None;
        }
        match note {
            A::Edit { text } => {
                self.edited = true;
                return Some(text.trim().to_string());
            }
            A::Delete => {
                self.deleted = true;
                self.reactions.clear();
                return Some(String::new());
            }
            A::React { emoji } => match self.reactions.iter_mut().find(|(who, _)| who == by) {
                Some(mine) => mine.1 = emoji.clone(),
                None => self.reactions.push((by.to_string(), emoji.clone())),
            },
            A::Unreact => self.reactions.retain(|(who, _)| who != by),
        }
        None
    }
}

impl Entry {
    /// The id the UI, search and jump-to-message use for this entry. It is the
    /// client id when there is one, so it does not change when the relay
    /// confirms an optimistic write (which re-hashes it into the chain).
    pub(crate) fn id(&self) -> &str {
        if self.client_id.is_empty() {
            &self.hash
        } else {
            &self.client_id
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SpaceState {
    pub calls: Vec<types::calls::CallRecord>,
    pub call_transport: Vec<types::call_transport::CallTransportRecord>,
    pub id: SpaceId,
    pub title: String,
    pub kind: SpaceKind,
    pub privacy: Privacy,
    pub members: Vec<(IdentityId, Role)>,
    pub entries: Vec<Entry>,
    pub last_at_ms: i64,
    pub integrity_error: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct Version {
    pub number: u32,
    pub content: ItemContent,
    pub author: IdentityId,
    pub at_ms: i64,
    pub note: String,
    pub is_undo: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ItemState {
    pub id: ItemId,
    pub space: SpaceId,
    pub kind: ItemKind,
    pub origin: String,
    pub versions: Vec<Version>,
    /// Derived only while applying accepted content, including replay. A MIME or
    /// path change must not turn an earlier voice recording into recent music.
    pub has_voice_history: bool,
}

impl ItemState {
    pub(crate) fn current(&self) -> &Version {
        self.versions.last().expect("item sempre tem v1")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReqStatus {
    Pending,
    Approved,
    Denied,
}

#[derive(Clone, Debug)]
pub(crate) struct RequestState {
    pub req: AgentRequest,
    pub space: SpaceId,
    pub opened_ms: i64,
    pub opened_content_hash: String,
    pub resolved_content_hash: Option<String>,
    pub status: ReqStatus,
    pub resolved_ms: Option<i64>,
    pub resolution: Option<RequestResolution>,
    /// Inert owner intent derived from the same signed conversation log.
    pub review: Option<crate::agent_binary_documents::tool_approvals::renewal::ReviewIntent>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct GrantState {
    pub grant: Grant,
    pub at_ms: i64,
    pub revoked: bool,
}

#[derive(Default)]
pub(crate) struct State {
    pub spaces: HashMap<SpaceId, SpaceState>,
    // Derived from the signed log, not another durable authority. O(1) membership checks.
    media_groups: types::media::MediaGroupTracker,
    // Local failed bytes are readable recovery data, never a new attachment target.
    recovery_items: HashSet<ItemId>,
    pub items: HashMap<ItemId, ItemState>,
    pub requests: HashMap<RequestId, RequestState>,
    pub request_order: Vec<RequestId>,
    /// Ordem em que os pedidos foram resolvidos (para a sequência de aprovações).
    pub resolved_order: Vec<RequestId>,
    pub grants: Vec<GrantState>,
    pub usage: Vec<(IdentityId, i64, i64)>, // (agente, centavos, quando)
    pub(crate) plan_origins: HashMap<ItemId, PlanOrigin>,
    pub(crate) waiting_spaces: HashSet<SpaceId>,
    /// My per-chat prefs (pin, archive, mute), from my Personal Space.
    pub(crate) chat_prefs: HashMap<SpaceId, types::ChatPrefs>,
}

// ───────────────────────────── motor ─────────────────────────────

pub struct Engine {
    pub(crate) store: Store,
    pub(crate) db_path: String,
    pub(crate) logs: HashMap<SpaceId, SpaceLog>,
    pub(crate) broken_spaces: HashMap<SpaceId, SpaceState>,
    pub(crate) approval_replay_needed: bool,
    pub(crate) space_order: Vec<SpaceId>,
    pub(crate) identities: HashMap<IdentityId, Identity>,
    pub(crate) identity_order: Vec<IdentityId>,
    pub(crate) signers: HashMap<IdentityId, Signer>,
    /// The search index is a projection; rebuilt lazily after the log changes.
    pub(crate) index_dirty: bool,
    pub(crate) me: Option<IdentityId>,
    pub(crate) state: State,
    pub(crate) space_projections: HashMap<SpaceId, State>,
    pub(crate) policy: Policy,
    /// Account, outbox and relay bookkeeping (see `sync.rs`).
    pub(crate) net: crate::sync::NetState,
    /// Open pages: Loro documents rebuilt from the log (see `pages.rs`).
    pub(crate) pages: crate::pages::PageCache,
}

pub(crate) type R<T> = Result<T, CoreError>;

impl From<store::StoreError> for CoreError {
    fn from(e: store::StoreError) -> Self {
        CoreError::Storage {
            message: e.to_string(),
        }
    }
}

fn not_found(what: &str) -> CoreError {
    CoreError::NotFound {
        what: what.to_string(),
    }
}

/// A mini-app tool an agent may call, from the manifest of an Item in its Space.
#[derive(Clone, Debug)]
pub struct AgentTool {
    pub item_id: String,
    pub item_title: String,
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_new_restore_notes_follow_session_language_and_preserve_history() {
        for (locale, restore_note) in [("en", "Restored v1"), ("pt-BR", "Restaurou a v1")] {
            let engine = crate::Client::open(":memory:".into(), locale.into()).unwrap();
            engine.seed_demo_if_empty().unwrap();
            let plan = engine
                .items()
                .into_iter()
                .find(|item| item.kind_id == "plan" && item.title.contains("Paraty"))
                .unwrap();
            let before = engine.item(plan.id.clone()).unwrap();
            let restored = engine
                .restore_plan_version(
                    plan.id.clone(),
                    1,
                    engine.plan_page(plan.id).unwrap().content_hash,
                )
                .unwrap();
            assert_eq!(restored.versions[0].note, restore_note);
            assert_eq!(
                restored
                    .versions
                    .iter()
                    .skip(1)
                    .map(|version| &version.note)
                    .collect::<Vec<_>>(),
                before
                    .versions
                    .iter()
                    .map(|version| &version.note)
                    .collect::<Vec<_>>()
            );
            let note = engine
                .items()
                .into_iter()
                .find(|item| item.kind_id == "note")
                .unwrap();
            let before = engine.item(note.id.clone()).unwrap();
            let restored = engine.restore_version(note.id, 1).unwrap();
            assert_eq!(restored.versions[0].note, restore_note);
            assert_eq!(
                restored
                    .versions
                    .iter()
                    .skip(1)
                    .map(|version| &version.note)
                    .collect::<Vec<_>>(),
                before
                    .versions
                    .iter()
                    .map(|version| &version.note)
                    .collect::<Vec<_>>()
            );
            assert_eq!(restored.text, before.text);
            assert!(engine.verify_all().iter().all(|report| report.valid));
        }
    }

    #[test]
    fn reopening_v3_history_and_pending_ciphertext_preserves_the_database() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../event-log/testdata/legacy-sealed-v3.json"
        ))
        .unwrap();
        let read = |name: &str| {
            protocol::Sequenced::decode(&hex::decode(fixture[name].as_str().unwrap()).unwrap())
                .unwrap()
        };
        let path = std::env::temp_dir().join(format!("zoen-v3-upgrade-{}.db", new_id("test")));
        let pending;
        {
            let e = Engine::open(path.to_str().unwrap()).unwrap();
            for name in ["genesis", "legacy_full"] {
                let ev = read(name);
                e.store
                    .append_event(
                        &event_log::event_from_content(
                            ev.env.content().to_vec(),
                            ev.env.sig,
                            ev.env.cert,
                            ev.seq,
                            ev.prev,
                            ev.hash,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            let old = read("legacy_full");
            let mut content = event_log::content::SignedContent::parse(old.env.content()).unwrap();
            content.client_id = "queued-before-upgrade".into();
            let bytes = content.encode();
            let key = Signer::from_secret(&[34; 32]);
            pending = event_log::event_from_content(
                bytes.clone(),
                key.sign(event_log::content::content_hash(&bytes).as_bytes()),
                old.env.cert,
                0,
                String::new(),
                String::new(),
            )
            .unwrap();
            e.store.outbox_put(&pending).unwrap();
        }
        {
            let e = Engine::open(path.to_str().unwrap()).unwrap();
            assert_eq!(e.store.meta("event_format").unwrap().as_deref(), Some("3"));
            assert_eq!(e.store.event_count().unwrap(), 2);
            assert!(e.logs["sp_upgrade"].verify().is_ok());
            assert_eq!(
                e.store.outbox_get(&pending.client_id).unwrap(),
                Some(pending)
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn brl_formats_like_brazil() {
        assert_eq!(brl(150_000), "R$ 1.500");
        assert_eq!(brl(1_240), "R$ 12,40");
        assert_eq!(brl(5), "R$ 0,05");
        assert_eq!(brl(123_456_789), "R$ 1.234.567,89");
        assert_eq!(brl(-4_000), "−R$ 40");
    }

    #[test]
    fn month_key_matches_calendar() {
        // 2026-10-07T12:00Z e 2026-10-31T23:59Z → mesmo mês; 2026-11-01 → próximo.
        let oct7 = 1_791_374_400_000;
        let oct31 = 1_793_491_140_000;
        let nov1 = 1_793_491_260_000;
        assert_eq!(month_key(oct7), 2026 * 12 + 10);
        assert_eq!(month_key(oct31), month_key(oct7));
        assert_eq!(month_key(nov1), month_key(oct7) + 1);
    }

    #[test]
    fn fold_ignores_accents_and_case() {
        assert_eq!(fold("Pousada Ç Ãção"), "pousada c acao");
    }
}
