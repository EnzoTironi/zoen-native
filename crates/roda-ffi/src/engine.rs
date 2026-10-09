//! O motor: guarda os logs, mantém a projeção (estado derivado) e aplica as regras.
//!
//! Fluxo de toda escrita: **avaliar Concessões → assinar evento → gravar no SQLite →
//! aplicar na projeção**. Ler é só consultar a projeção. Reabrir o app = reverificar
//! cada log e reprojetar do zero.

use std::collections::HashMap;

use roda_grants::{
    evaluate, standing_allow_permitted, standing_key, Budget, Decision, Policy, Reason,
};
use roda_log::{content_hash, Author, LogError, Signer, SpaceLog};
use roda_store::Store;
use roda_types::*;
use serde_json::{json, Value};

use crate::apps;

use crate::dto::*;
use crate::i18n::{money, t, ts};
use crate::tr;
use crate::CoreError;

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
        /// Inline reply / thread reply link (see `roda_types::reply`).
        reply: Option<roda_types::ReplyRef>,
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
}

impl ItemState {
    fn current(&self) -> &Version {
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
    pub status: ReqStatus,
    pub resolved_ms: Option<i64>,
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
    pub items: HashMap<ItemId, ItemState>,
    pub requests: HashMap<RequestId, RequestState>,
    pub request_order: Vec<RequestId>,
    /// Ordem em que os pedidos foram resolvidos (para a sequência de aprovações).
    pub resolved_order: Vec<RequestId>,
    pub grants: Vec<GrantState>,
    pub usage: Vec<(IdentityId, i64, i64)>, // (agente, centavos, quando)
}

impl State {
    pub(crate) fn apply(&mut self, e: &Event) {
        let entry = |body| Entry {
            hash: e.hash.clone(),
            client_id: e.client_id.clone(),
            space: e.space.clone(),
            seq: e.seq,
            author: e.author.clone(),
            at_ms: e.at_ms,
            body,
        };
        match &e.body {
            EventBody::SpaceCreated {
                title,
                kind,
                privacy,
            } => {
                self.spaces.insert(
                    e.space.clone(),
                    SpaceState {
                        id: e.space.clone(),
                        title: title.clone(),
                        kind: *kind,
                        privacy: *privacy,
                        members: vec![(e.author.clone(), Role::Owner)],
                        entries: Vec::new(),
                        last_at_ms: e.at_ms,
                        integrity_error: None,
                    },
                );
            }
            EventBody::MemberAdded { identity, role } => {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.members.retain(|(id, _)| id != identity);
                    s.members.push((identity.clone(), *role));
                }
            }
            EventBody::MemberRemoved { identity } => {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.members.retain(|(id, _)| id != identity);
                }
            }
            EventBody::SpaceEncrypted => {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.privacy = Privacy::EndToEnd;
                    s.entries.push(entry(EntryBody::System {
                        text: t(
                            "Criptografia de ponta a ponta ativada. Só os membros leem as mensagens novas.",
                            "End-to-end encryption is on. Only members can read new messages.",
                        ),
                    }));
                }
            }
            EventBody::MessagePosted {
                text,
                attaches,
                reply,
                ..
            } => {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.entries.push(entry(EntryBody::Message {
                        text: text.clone(),
                        attaches: attaches.clone(),
                        reply: reply.clone(),
                    }));
                    s.last_at_ms = s.last_at_ms.max(e.at_ms);
                }
            }
            EventBody::BackgroundSet { background } => {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.entries.push(entry(EntryBody::Background {
                        spec: background.clone(),
                    }));
                    s.last_at_ms = s.last_at_ms.max(e.at_ms);
                }
            }
            EventBody::ItemCreated {
                item,
                kind,
                content,
                origin,
            } => {
                self.items.insert(
                    item.clone(),
                    ItemState {
                        id: item.clone(),
                        space: e.space.clone(),
                        kind: *kind,
                        origin: origin.clone(),
                        versions: vec![Version {
                            number: 1,
                            content: content.clone(),
                            author: e.author.clone(),
                            at_ms: e.at_ms,
                            note: t("Criado", "Created"),
                            is_undo: false,
                        }],
                    },
                );
            }
            EventBody::ItemVersioned {
                item,
                content,
                note,
            } => {
                if let Some(it) = self.items.get_mut(item) {
                    let number = it.versions.len() as u32 + 1;
                    it.versions.push(Version {
                        number,
                        content: content.clone(),
                        author: e.author.clone(),
                        at_ms: e.at_ms,
                        note: note.clone(),
                        is_undo: false,
                    });
                    if let Some(s) = self.spaces.get_mut(&e.space) {
                        s.entries.push(entry(EntryBody::ItemEdited {
                            item: item.clone(),
                            version: number,
                            note: note.clone(),
                            is_undo: false,
                        }));
                        s.last_at_ms = s.last_at_ms.max(e.at_ms);
                    }
                }
            }
            EventBody::ItemReverted {
                item,
                to_version,
                note,
            } => {
                if let Some(it) = self.items.get_mut(item) {
                    if let Some(target) = it
                        .versions
                        .iter()
                        .find(|v| v.number == *to_version)
                        .cloned()
                    {
                        let number = it.versions.len() as u32 + 1;
                        it.versions.push(Version {
                            number,
                            content: target.content,
                            author: e.author.clone(),
                            at_ms: e.at_ms,
                            note: note.clone(),
                            is_undo: true,
                        });
                        if let Some(s) = self.spaces.get_mut(&e.space) {
                            s.entries.push(entry(EntryBody::ItemEdited {
                                item: item.clone(),
                                version: number,
                                note: note.clone(),
                                is_undo: true,
                            }));
                        }
                    }
                }
            }
            EventBody::GrantIssued { grant } => {
                self.grants.push(GrantState {
                    grant: grant.clone(),
                    at_ms: e.at_ms,
                    revoked: false,
                });
            }
            EventBody::GrantRevoked { grant } => {
                for g in self.grants.iter_mut().filter(|g| &g.grant.id == grant) {
                    g.revoked = true;
                }
            }
            EventBody::RequestOpened { request } => {
                self.request_order.push(request.id.clone());
                self.requests.insert(
                    request.id.clone(),
                    RequestState {
                        req: request.clone(),
                        space: e.space.clone(),
                        opened_ms: e.at_ms,
                        status: ReqStatus::Pending,
                        resolved_ms: None,
                    },
                );
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.entries.push(entry(EntryBody::Request {
                        request: request.id.clone(),
                    }));
                }
            }
            EventBody::RequestResolved {
                request, approved, ..
            } => {
                self.resolved_order.push(request.clone());
                if let Some(r) = self.requests.get_mut(request) {
                    r.status = if *approved {
                        ReqStatus::Approved
                    } else {
                        ReqStatus::Denied
                    };
                    r.resolved_ms = Some(e.at_ms);
                }
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.entries.push(entry(EntryBody::RequestResolved {
                        request: request.clone(),
                        approved: *approved,
                    }));
                }
            }
            EventBody::UsageRecorded { agent, cents, .. } => {
                self.usage.push((agent.clone(), *cents, e.at_ms));
            }
            EventBody::ProfileKeyShared { .. }
            | EventBody::Checkpoint { .. }
            | EventBody::Sealed { .. }
            | EventBody::Unsupported { .. } => {}
        }
    }
}

// ───────────────────────────── motor ─────────────────────────────

pub struct Engine {
    pub(crate) store: Store,
    pub(crate) db_path: String,
    pub(crate) logs: HashMap<SpaceId, SpaceLog>,
    pub(crate) space_order: Vec<SpaceId>,
    pub(crate) identities: HashMap<IdentityId, Identity>,
    pub(crate) identity_order: Vec<IdentityId>,
    pub(crate) signers: HashMap<IdentityId, Signer>,
    /// The search index is a projection; rebuilt lazily after the log changes.
    pub(crate) index_dirty: bool,
    pub(crate) me: Option<IdentityId>,
    pub(crate) state: State,
    pub(crate) policy: Policy,
    /// Account, outbox and relay bookkeeping (see `sync.rs`).
    pub(crate) net: crate::sync::NetState,
    /// Open pages: Loro documents rebuilt from the log (see `pages.rs`).
    pub(crate) pages: crate::pages::PageCache,
}

pub(crate) type R<T> = Result<T, CoreError>;

impl From<roda_store::StoreError> for CoreError {
    fn from(e: roda_store::StoreError) -> Self {
        CoreError::Storage {
            reason: e.to_string(),
        }
    }
}

fn not_found(what: &str) -> CoreError {
    CoreError::NotFound {
        what: what.to_string(),
    }
}

impl Engine {
    pub fn open(path: &str) -> R<Self> {
        let store = if path == ":memory:" {
            Store::in_memory()?
        } else {
            Store::open(path)?
        };
        let mut engine = Engine {
            store,
            db_path: path.to_string(),
            logs: HashMap::new(),
            space_order: Vec::new(),
            identities: HashMap::new(),
            identity_order: Vec::new(),
            signers: HashMap::new(),
            me: None,
            state: State::default(),
            policy: Policy::default(),
            index_dirty: true,
            net: Default::default(),
            pages: Default::default(),
        };
        engine.migrate_event_format()?;
        engine.migrate_mls()?;
        engine.reload()?;
        Ok(engine)
    }

    /// Recarrega tudo do disco: reverifica cada log e reprojeta do zero.
    pub fn reload(&mut self) -> R<()> {
        self.pages
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        self.logs.clear();
        self.space_order.clear();
        self.identities.clear();
        self.identity_order.clear();
        self.signers.clear();
        self.state = State::default();
        self.index_dirty = true;
        for (identity, secret) in self.store.identities()? {
            if let Some(secret) = secret {
                self.signers
                    .insert(identity.id.clone(), Signer::from_secret(&secret));
            }
            self.identity_order.push(identity.id.clone());
            self.identities.insert(identity.id.clone(), identity);
        }
        self.me = self.store.meta("me")?;
        for space in self.store.space_ids()? {
            let events = self.store.events(&space)?;
            match SpaceLog::from_events(space.clone(), events.clone()) {
                Ok(log) => {
                    for e in log.events() {
                        self.state.apply(e);
                    }
                    self.logs.insert(space.clone(), log);
                }
                Err(err) => {
                    // Log adulterado: não projetamos nada dele, mas o Espaço aparece com o erro.
                    let title = events
                        .iter()
                        .find_map(|e| match &e.body {
                            EventBody::SpaceCreated { title, .. } => Some(title.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| t("Espaço", "Space"));
                    self.state.spaces.insert(
                        space.clone(),
                        SpaceState {
                            id: space.clone(),
                            title,
                            kind: SpaceKind::Group,
                            privacy: Privacy::EndToEnd,
                            members: vec![],
                            entries: vec![],
                            last_at_ms: 0,
                            integrity_error: Some(err.to_string()),
                        },
                    );
                }
            }
            self.space_order.push(space);
        }
        self.reload_sync()?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.identities.is_empty()
    }

    pub(crate) fn me_id(&self) -> R<IdentityId> {
        self.me.clone().ok_or_else(|| not_found("identidade local"))
    }

    // ── escrita de baixo nível ──

    pub(crate) fn create_identity(&mut self, mut identity: Identity, local: bool) -> R<IdentityId> {
        let signer = Signer::generate();
        identity.id = signer.id();
        let secret = signer.secret();
        self.store
            .put_identity(&identity, if local { Some(&secret) } else { None })?;
        if local {
            self.signers.insert(identity.id.clone(), signer);
        }
        self.identity_order.push(identity.id.clone());
        let id = identity.id.clone();
        self.identities.insert(id.clone(), identity);
        Ok(id)
    }

    pub(crate) fn append_at(
        &mut self,
        space: &str,
        author: &str,
        at_ms: i64,
        body: EventBody,
    ) -> R<Event> {
        let signer = self.author_for(author)?;
        if self.net.synced.contains(space) {
            // Only this device's account publishes to the relay for now; local agents
            // join shared chats as real members with the agent runtime (milestone 3).
            if self
                .net
                .account
                .as_ref()
                .is_some_and(|a| a.identity != signer.identity)
            {
                return Err(CoreError::Forbidden {
                    reason: t(
                        "agentes ainda não escrevem em conversas compartilhadas",
                        "agents can't write in shared chats yet",
                    ),
                });
            }
            // Relay-ordered Space: sign now, queue, show it right away; the relay assigns
            // the sequence number when it gets there (now or after being offline).
            return self.append_synced(space, &signer, at_ms, body);
        }
        let is_new = !self.logs.contains_key(space);
        if is_new && !matches!(body, EventBody::SpaceCreated { .. }) {
            return Err(not_found(&t("Espaço", "Space")));
        }
        let log = self
            .logs
            .entry(space.to_string())
            .or_insert_with(|| SpaceLog::new(space));
        let client_id = new_ulid(at_ms);
        let seen = log.head();
        let event = log
            .sequence(signer.sign_event(space, &client_id, at_ms, seen, body))
            .clone();
        if let Err(e) = self.store.append_event(&event) {
            // Mantém memória e disco iguais: desfaz o append em memória recarregando.
            self.reload()?;
            return Err(e.into());
        }
        if is_new {
            self.space_order.push(space.to_string());
        }
        self.state.apply(&event);
        self.index_dirty = true;
        Ok(event)
    }

    pub(crate) fn append(&mut self, space: &str, author: &str, body: EventBody) -> R<Event> {
        self.append_at(space, author, now_ms(), body)
    }

    // ── consultas internas ──

    pub(crate) fn persona(&self, id: &str) -> Persona {
        let me = self.me.as_deref();
        match self.identities.get(id) {
            Some(i) => {
                let owner = i.owner.as_ref().and_then(|o| self.identities.get(o));
                let initials: String = i
                    .name
                    .split_whitespace()
                    .filter_map(|w| w.chars().next())
                    .take(2)
                    .collect::<String>()
                    .to_uppercase();
                Persona {
                    id: i.id.clone(),
                    kind: match i.kind {
                        IdentityKind::Person => PersonaKind::Person,
                        IdentityKind::Agent => PersonaKind::Agent,
                    },
                    name: i.name.clone(),
                    handle: i.handle.clone(),
                    initials,
                    tint_hex: i.tint_hex.clone(),
                    glyph: i.glyph.clone(),
                    bio: i.bio.clone(),
                    owner_id: i.owner.clone(),
                    owner_name: owner.map(|o| o.name.clone()),
                    owner_tint_hex: owner.map(|o| o.tint_hex.clone()),
                    is_me: Some(id) == me,
                    is_mine: i.owner.as_deref() == me && me.is_some(),
                }
            }
            None => Persona {
                id: id.to_string(),
                kind: PersonaKind::Person,
                name: t("Desconhecido", "Unknown"),
                handle: String::new(),
                initials: "?".into(),
                tint_hex: "#8E8E93".into(),
                glyph: None,
                bio: String::new(),
                owner_id: None,
                owner_name: None,
                owner_tint_hex: None,
                is_me: false,
                is_mine: false,
            },
        }
    }

    pub(crate) fn space_state(&self, id: &str) -> R<&SpaceState> {
        self.state
            .spaces
            .get(id)
            .ok_or_else(|| not_found(&t("Espaço", "Space")))
    }

    fn item_state(&self, id: &str) -> R<&ItemState> {
        self.state.items.get(id).ok_or_else(|| not_found("Item"))
    }

    pub(crate) fn is_member(&self, space: &str, who: &str) -> bool {
        self.state
            .spaces
            .get(space)
            .map(|s| s.members.iter().any(|(m, _)| m == who))
            .unwrap_or(false)
    }

    pub(crate) fn trust(&self, agent: &str, space: &str) -> TrustLevel {
        let owner = self.identities.get(agent).and_then(|a| a.owner.clone());
        self.state
            .grants
            .iter()
            .rev()
            .filter(|g| !g.revoked && g.grant.grantee.as_deref() == Some(agent))
            .filter(|g| g.grant.scope == GrantScope::Space(space.to_string()))
            .find_map(|g| match g.grant.capability {
                Capability::Trust(l) => Some(l),
                _ => None,
            })
            // Padrões do documento: seus agentes nos seus Espaços → Agir; os outros → Sugerir.
            .unwrap_or(if owner.as_deref() == self.me.as_deref() {
                TrustLevel::Act
            } else {
                TrustLevel::Suggest
            })
    }

    /// Orçamento do mês. `None` quando o orçamento não vive neste aparelho
    /// (agente de outra pessoa: quem é dono paga).
    pub(crate) fn budget(&self, agent: &str) -> Option<Budget> {
        let limit = self
            .state
            .grants
            .iter()
            .rev()
            .filter(|g| !g.revoked && g.grant.grantee.as_deref() == Some(agent))
            .find_map(|g| match g.grant.capability {
                Capability::MonthlyBudget { cents } => Some(cents),
                _ => None,
            })?;
        let month = month_key(now_ms());
        let spent = self
            .state
            .usage
            .iter()
            .filter(|(a, _, at)| a == agent && month_key(*at) == month)
            .map(|(_, c, _)| c)
            .sum();
        Some(Budget {
            limit_cents: limit,
            spent_cents: spent,
        })
    }

    fn decide(&self, agent: &str, space: &str, action: &ActionClass, ai_cost: i64) -> Decision {
        // Agente de outra pessoa sem orçamento local: o dono dele decide o orçamento.
        let budget = self.budget(agent).unwrap_or(Budget {
            limit_cents: i64::MAX / 4,
            spent_cents: 0,
        });
        let decision = evaluate(
            self.trust(agent, space),
            action,
            ai_cost,
            &budget,
            &self.policy,
        );
        // A standing decision from the owner settles what would otherwise be a request:
        // "always deny" blocks it, "always approve" lets it run (never past a red line).
        let Decision::Request(reason) = decision else {
            return decision;
        };
        match self.standing(agent, space, action) {
            Some((_, false)) => Decision::Block(Reason::StandingDeny),
            Some((_, true)) if standing_allow_permitted(action, &self.policy) => Decision::Act {
                undoable: matches!(action, ActionClass::Reversible),
            },
            _ => Decision::Request(reason),
        }
    }

    /// The newest unrevoked standing decision for this agent, kind of action and Space.
    fn standing(
        &self,
        agent: &str,
        space: &str,
        action: &ActionClass,
    ) -> Option<(&GrantState, bool)> {
        let key = standing_key(action);
        self.state
            .grants
            .iter()
            .rev()
            .filter(|g| !g.revoked && g.grant.grantee.as_deref() == Some(agent))
            .filter(|g| g.grant.scope == GrantScope::Space(space.to_string()))
            .find_map(|g| match &g.grant.capability {
                Capability::Standing { action, allow } if action == key => Some((g, *allow)),
                _ => None,
            })
    }

    fn personal_space(&self) -> Option<SpaceId> {
        self.space_order
            .iter()
            .find(|s| {
                self.state
                    .spaces
                    .get(*s)
                    .map(|x| x.kind == SpaceKind::Personal)
                    .unwrap_or(false)
            })
            .cloned()
    }

    fn line_hash(line: &PlanLine) -> String {
        content_hash(&(&line.text, line.cost_cents))
    }

    // ── conversões para DTO ──

    pub(crate) fn card(&self, item: &str) -> Option<ItemCard> {
        let it = self.state.items.get(item)?;
        let v = it.current();
        let (summary, total, budget, lines, done) = match &v.content {
            ItemContent::Plan(p) => {
                let all: Vec<&PlanLine> = p.sections.iter().flat_map(|s| &s.lines).collect();
                (
                    p.summary.clone(),
                    Some(p.total_cents()),
                    p.budget_cents,
                    all.len() as u32,
                    all.iter().filter(|l| l.done).count() as u32,
                )
            }
            ItemContent::Text { text } => (text.clone(), None, None, 0, 0),
            ItemContent::App(_) => (String::new(), None, None, 0, 0),
            ItemContent::Page(_) => {
                let text = self.page_text(item);
                let summary: String = text
                    .lines()
                    .skip(1)
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("")
                    .chars()
                    .take(160)
                    .collect();
                (summary, None, None, 0, 0)
            }
            ItemContent::File(f) => (f.mime.clone(), None, None, 0, 0),
        };
        Some(ItemCard {
            app: self.app_state_dto(it),
            item_id: it.id.clone(),
            title: v.content.title(),
            summary,
            kind_label: kind_label(it.kind).into(),
            kind_id: kind_id(it.kind).into(),
            version: v.number,
            total_cents: total,
            budget_cents: budget,
            line_count: lines,
            done_count: done,
        })
    }

    pub(crate) fn entry_dto(&self, e: &Entry) -> TimelineEntry {
        let kind = match &e.body {
            EntryBody::Message { text, attaches, .. } => EntryKind::Message {
                text: text.clone(),
                card: attaches.as_deref().and_then(|i| self.card(i)),
            },
            EntryBody::ItemEdited {
                item,
                version,
                note,
                is_undo,
            } => EntryKind::ItemEdited {
                item_id: item.clone(),
                title: self
                    .state
                    .items
                    .get(item)
                    .map(|i| i.current().content.title())
                    .unwrap_or_default(),
                version: *version,
                note: note.clone(),
                is_undo: *is_undo,
            },
            EntryBody::Request { request } => {
                let r = self.state.requests.get(request);
                EntryKind::Request {
                    request_id: request.clone(),
                    title: r.map(|r| r.req.title.clone()).unwrap_or_default(),
                    approved: r.and_then(|r| match r.status {
                        ReqStatus::Pending => None,
                        ReqStatus::Approved => Some(true),
                        ReqStatus::Denied => Some(false),
                    }),
                }
            }
            EntryBody::RequestResolved { request, approved } => {
                let title = self
                    .state
                    .requests
                    .get(request)
                    .map(|r| r.req.title.clone())
                    .unwrap_or_default();
                EntryKind::System {
                    text: format!(
                        "{} {}",
                        if *approved {
                            ts("Aprovado:", "Approved:")
                        } else {
                            ts("Recusado:", "Declined:")
                        },
                        title
                    ),
                }
            }
            EntryBody::System { text } => EntryKind::System { text: text.clone() },
            EntryBody::Background { spec } => EntryKind::Background {
                background: spec.into(),
            },
        };
        let (reply_to, in_thread) = self.reply_parts(e);
        TimelineEntry {
            id: e.id().to_string(),
            seq: e.seq,
            author: self.persona(&e.author),
            at_ms: e.at_ms,
            kind,
            delivery: self.delivery(e),
            reply_to,
            in_thread,
            thread_replies: 0,
        }
    }

    fn preview(&self, s: &SpaceState) -> (String, Option<Persona>) {
        for e in s.entries.iter().rev() {
            let text = match &e.body {
                EntryBody::Message { text, attaches, .. } => {
                    match attaches.as_deref().and_then(|i| self.card(i)) {
                        Some(c) if text.is_empty() => format!("{} · {}", c.kind_label, c.title),
                        _ => text.clone(),
                    }
                }
                EntryBody::ItemEdited { note, .. } => note.clone(),
                EntryBody::Request { request } => self
                    .state
                    .requests
                    .get(request)
                    .map(|r| tr!("Pedido: {}", "Request: {}", r.req.title))
                    .unwrap_or_default(),
                EntryBody::RequestResolved { .. } | EntryBody::System { .. } => continue,
                EntryBody::Background { spec } => {
                    if spec.style == "photo" || spec.style.starts_with("builtin:") {
                        t(
                            "Definiu uma foto como fundo",
                            "Set a photo as the background",
                        )
                    } else {
                        t("Mudou o fundo da conversa", "Changed the chat background")
                    }
                }
            };
            return (text, Some(self.persona(&e.author)));
        }
        (t("Nenhuma mensagem ainda", "No messages yet"), None)
    }

    pub(crate) fn request_dto(&self, r: &RequestState) -> AgentRequestDto {
        let mut status = match r.status {
            ReqStatus::Pending => RequestStatus::Pending,
            ReqStatus::Approved => RequestStatus::Approved,
            ReqStatus::Denied => RequestStatus::Denied,
        };
        if status == RequestStatus::Pending && !self.request_is_current(&r.req) {
            status = RequestStatus::Stale;
        }
        let decision = self.decide(&r.req.agent, &r.space, &r.req.action, 0);
        AgentRequestDto {
            id: r.req.id.clone(),
            agent: self.persona(&r.req.agent),
            space_id: r.space.clone(),
            space_title: self
                .state
                .spaces
                .get(&r.space)
                .map(|s| s.title.clone())
                .unwrap_or_default(),
            title: r.req.title.clone(),
            detail: r.req.detail.clone(),
            audience: r.req.audience.clone(),
            action_label: action_label(&r.req.action).into(),
            cost_cents: match r.req.action {
                ActionClass::Money { cents } => Some(cents),
                _ => None,
            },
            reason: match decision {
                Decision::Request(reason) | Decision::Block(reason) => {
                    reason_label(&reason, &self.policy)
                }
                Decision::Act { .. } => t(
                    "Pedido aberto antes de um nível de confiança maior",
                    "Request opened before a higher trust level",
                ),
            },
            status,
            opened_ms: r.opened_ms,
            resolved_ms: r.resolved_ms,
            item_id: r.req.item.clone(),
            line_id: r.req.line.clone(),
            action_key: standing_key(&r.req.action).into(),
            can_always_approve: standing_allow_permitted(&r.req.action, &self.policy),
            by_standing: r.resolved_ms.is_some_and(|at| {
                let key = standing_key(&r.req.action);
                self.state.grants.iter().any(|g| {
                    g.at_ms <= at
                        && g.grant.grantee.as_deref() == Some(r.req.agent.as_str())
                        && g.grant.scope == GrantScope::Space(r.space.clone())
                        && matches!(&g.grant.capability, Capability::Standing { action, .. } if action == key)
                })
            }),
        }
    }

    fn request_is_current(&self, req: &AgentRequest) -> bool {
        match (&req.item, &req.line) {
            (Some(item), Some(line)) => self
                .state
                .items
                .get(item)
                .and_then(|it| match &it.current().content {
                    ItemContent::Plan(p) => p.line(line).map(|l| {
                        roda_grants::approval_still_valid(&req.content_hash, &Self::line_hash(l))
                    }),
                    _ => None,
                })
                .unwrap_or(false),
            _ => true,
        }
    }

    // ───────────────────────────── API pública ─────────────────────────────

    pub fn me(&self) -> R<Persona> {
        Ok(self.persona(&self.me_id()?))
    }

    pub fn personas(&self) -> Vec<Persona> {
        self.identity_order
            .iter()
            .map(|id| self.persona(id))
            .collect()
    }

    pub fn spaces(&self) -> Vec<SpaceSummary> {
        let mut out: Vec<SpaceSummary> = self
            .space_order
            .iter()
            .filter_map(|id| self.space_summary(id).ok())
            .collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.last_at_ms));
        out
    }

    pub fn space_summary(&self, id: &str) -> R<SpaceSummary> {
        let s = self.space_state(id)?;
        let kind = match s.kind {
            SpaceKind::Direct => SpaceKindDto::Direct,
            SpaceKind::Group => SpaceKindDto::Group,
            SpaceKind::Community => SpaceKindDto::Community,
            SpaceKind::Personal => return Err(not_found(&t("Espaço", "Space"))),
        };
        let me = self.me.clone().unwrap_or_default();
        let members: Vec<Persona> = s.members.iter().map(|(m, _)| self.persona(m)).collect();
        let counterpart = if s.kind == SpaceKind::Direct {
            members.iter().find(|p| p.id != me).cloned()
        } else {
            None
        };
        let (last_preview, last_author) = match &s.integrity_error {
            Some(err) => (tr!("⚠︎ Log inválido: {err}", "⚠︎ Invalid log: {err}"), None),
            None => self.preview(s),
        };
        let read_seq: Option<u64> = self
            .store
            .meta(&format!("read:{id}"))
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok());
        let unread = s
            .entries
            .iter()
            .filter(|e| e.author != me && read_seq.map(|r| e.seq > r).unwrap_or(true))
            .filter(|e| matches!(e.body, EntryBody::Message { .. }))
            .count() as u32;
        let pending = self
            .state
            .requests
            .values()
            .filter(|r| r.space == id && r.status == ReqStatus::Pending)
            .count() as u32;
        Ok(SpaceSummary {
            id: s.id.clone(),
            title: counterpart
                .as_ref()
                .map(|c| c.name.clone())
                .unwrap_or_else(|| s.title.clone()),
            kind,
            privacy: match s.privacy {
                Privacy::EndToEnd => PrivacyDto::EndToEnd,
                Privacy::Closed => PrivacyDto::Closed,
                Privacy::Public => PrivacyDto::Public,
            },
            members,
            counterpart,
            last_preview,
            last_author,
            last_at_ms: s.last_at_ms,
            unread,
            pending_requests: pending,
            event_count: self.logs.get(id).map(|l| l.len() as u64).unwrap_or(0),
        })
    }

    pub fn member_roles(&self, space: &str) -> R<Vec<MemberRoleDto>> {
        Ok(self
            .space_state(space)?
            .members
            .iter()
            .map(|(identity, role)| MemberRoleDto {
                identity_id: identity.clone(),
                role: match role {
                    Role::Owner => "owner",
                    Role::Admin => "admin",
                    Role::Member => "member",
                    Role::Reader => "reader",
                }
                .into(),
            })
            .collect())
    }

    pub fn mark_read(&self, space: &str) -> R<()> {
        let s = self.space_state(space)?;
        // Max, not last: my own pending messages sit at the end with no relay seq yet.
        if let Some(seq) = s.entries.iter().map(|e| e.seq).max() {
            self.store
                .set_meta(&format!("read:{space}"), &seq.to_string())?;
        }
        Ok(())
    }

    pub fn timeline(&self, space: &str) -> R<Vec<TimelineEntry>> {
        let s = self.space_state(space)?;
        let mut out: Vec<TimelineEntry> = s.entries.iter().map(|e| self.entry_dto(e)).collect();
        crate::replies::count_thread_replies(&mut out);
        Ok(out)
    }

    pub fn send_message(&mut self, space: &str, text: &str) -> R<TimelineEntry> {
        let me = self.me_id()?;
        self.post_as(space, &me, text, None)
    }

    pub(crate) fn post_as(
        &mut self,
        space: &str,
        author: &str,
        text: &str,
        attaches: Option<ItemId>,
    ) -> R<TimelineEntry> {
        if !self.is_member(space, author) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros escrevem neste Espaço",
                    "only members can write in this Space",
                ),
            });
        }
        let text = text.trim();
        if text.is_empty() && attaches.is_none() {
            return Err(CoreError::Invalid {
                reason: t("mensagem vazia", "empty message"),
            });
        }
        self.append(
            space,
            author,
            EventBody::MessagePosted {
                message: new_id("msg"),
                text: text.to_string(),
                attaches,
                reply: None,
            },
        )?;
        let s = self.space_state(space)?;
        Ok(self.entry_dto(s.entries.last().expect("acabou de entrar")))
    }

    /// O agente responde com texto (ação `Reply`, avaliada contra Concessões e orçamento).
    pub fn agent_say(
        &mut self,
        space: &str,
        agent: &str,
        text: &str,
        ai_cost_cents: i64,
    ) -> R<TimelineEntry> {
        match self.decide(agent, space, &ActionClass::Reply, ai_cost_cents) {
            Decision::Block(r) => Err(CoreError::Forbidden {
                reason: reason_label(&r, &self.policy),
            }),
            _ => {
                let e = self.post_as(space, agent, text, None)?;
                if ai_cost_cents > 0 {
                    self.append(
                        space,
                        agent,
                        EventBody::UsageRecorded {
                            agent: agent.into(),
                            cents: ai_cost_cents,
                            what: "resposta".into(),
                        },
                    )?;
                }
                Ok(e)
            }
        }
    }

    /// O agente transforma um pedido num Item (plano). O plano em si vem do cliente
    /// (Foundation Models no aparelho, ou o planejador local de fallback); o núcleo
    /// decide se o agente pode, assina, guarda e versiona.
    pub fn agent_create_plan(
        &mut self,
        space: &str,
        agent: &str,
        prompt: &str,
        plan: PlanDto,
        engine_label: &str,
        ai_cost_cents: i64,
    ) -> R<PlanOutcome> {
        if !self.is_member(space, agent) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "o agente não é membro deste Espaço",
                    "the agent is not a member of this Space",
                ),
            });
        }
        let doc = plan_from_dto(plan);
        if doc.sections.iter().all(|s| s.lines.is_empty()) {
            return Err(CoreError::Invalid {
                reason: t("plano vazio", "empty plan"),
            });
        }
        let decision = self.decide(agent, space, &ActionClass::Reversible, ai_cost_cents);
        let agent_name = self.persona(agent).name;
        match decision {
            Decision::Block(reason) => Ok(PlanOutcome {
                kind: DecisionKind::Block,
                item: None,
                message: reason_label(&reason, &self.policy),
            }),
            Decision::Request(reason) => {
                // Sugerir: o plano fica como proposta em Atividade.
                let req = AgentRequest {
                    id: new_id("rq"),
                    agent: agent.into(),
                    title: tr!("Criar o plano “{}”", "Create the plan “{}”", doc.title),
                    detail: prompt.into(),
                    audience: t("Este Espaço", "This Space"),
                    action: ActionClass::Reversible,
                    content_hash: content_hash(&doc),
                    item: None,
                    line: None,
                };
                self.append(space, agent, EventBody::RequestOpened { request: req })?;
                Ok(PlanOutcome { kind: DecisionKind::Request, item: None, message: tr!("{agent_name} preparou uma proposta e pediu sua aprovação em Atividade · {}", "{agent_name} drafted a proposal and asked for your approval in Activity · {}", reason_label(&reason, &self.policy)) })
            }
            Decision::Act { .. } => {
                let item = new_id("it");
                let total = doc.total_cents();
                let budget = doc.budget_cents;
                let title = doc.title.clone();
                self.append(
                    space,
                    agent,
                    EventBody::ItemCreated {
                        item: item.clone(),
                        kind: ItemKind::Plan,
                        content: ItemContent::Plan(doc),
                        origin: tr!(
                            "{engine_label} · a partir de “{prompt}”",
                            "{engine_label} · from “{prompt}”"
                        ),
                    },
                )?;
                let text = match budget {
                    Some(b) if total <= b => tr!("Montei “{title}”: {} de {} · sobra {}. Toque para editar.", "Here’s “{title}”: {} of {} · {} left. Tap to edit.", money(total), money(b), money(b - total)),
                    Some(b) => tr!("Montei “{title}”, mas deu {} — passou {} do teto de {}. Quer que eu corte algo?", "Here’s “{title}”, but it came to {} — {} over the {} cap. Want me to cut something?", money(total), money(total - b), money(b)),
                    None => tr!("Montei “{title}” · total {}. Toque para editar.", "Here’s “{title}” · total {}. Tap to edit.", money(total)),
                };
                self.post_as(space, agent, &text, Some(item.clone()))?;
                if ai_cost_cents > 0 {
                    self.append(
                        space,
                        agent,
                        EventBody::UsageRecorded {
                            agent: agent.into(),
                            cents: ai_cost_cents,
                            what: "plano".into(),
                        },
                    )?;
                }
                Ok(PlanOutcome {
                    kind: DecisionKind::ActWithUndo,
                    item: Some(self.item(&item)?),
                    message: text,
                })
            }
        }
    }

    pub fn item(&self, id: &str) -> R<ItemDetail> {
        let it = self.item_state(id)?;
        let v = it.current();
        let space_title = self
            .state
            .spaces
            .get(&it.space)
            .map(|s| s.title.clone())
            .unwrap_or_default();
        let (plan, text) = match &v.content {
            ItemContent::Plan(p) => (Some(plan_to_dto(p)), None),
            ItemContent::Text { text } => (None, Some(text.clone())),
            ItemContent::App(_) => (None, None),
            ItemContent::Page(_) => (None, Some(self.page_text(id))),
            ItemContent::File(_) => (None, None),
        };
        let (path, file) = match &v.content {
            ItemContent::Page(p) => (p.path.clone(), None),
            ItemContent::File(f) => (f.path.clone(), Some(self.file_dto(f))),
            _ => (String::new(), None),
        };
        let linked_requests = self
            .state
            .request_order
            .iter()
            .filter_map(|r| self.state.requests.get(r))
            .filter(|r| r.req.item.as_deref() == Some(id))
            .map(|r| self.request_dto(r))
            .collect();
        Ok(ItemDetail {
            id: it.id.clone(),
            space_id: it.space.clone(),
            space_title,
            kind_label: kind_label(it.kind).into(),
            kind_id: kind_id(it.kind).into(),
            title: v.content.title(),
            origin: it.origin.clone(),
            created_by: self.persona(&it.versions[0].author),
            plan,
            text,
            version: v.number,
            versions: it
                .versions
                .iter()
                .rev()
                .map(|v| VersionDto {
                    number: v.number,
                    author: self.persona(&v.author),
                    at_ms: v.at_ms,
                    note: v.note.clone(),
                    is_undo: v.is_undo,
                    total_cents: match &v.content {
                        ItemContent::Plan(p) => Some(p.total_cents()),
                        _ => None,
                    },
                })
                .collect(),
            linked_requests,
            app: self.app_state_dto(it),
            path,
            file,
        })
    }

    pub fn items(&self) -> Vec<ItemDetail> {
        let mut v: Vec<&ItemState> = self.state.items.values().collect();
        v.sort_by_key(|i| std::cmp::Reverse(i.current().at_ms));
        v.into_iter()
            .filter_map(|i| self.item(&i.id).ok())
            .collect()
    }

    /// Edita o plano como "eu". Toda edição é uma versão nova → sempre desfazível.
    fn edit_plan(
        &mut self,
        item: &str,
        note: String,
        f: impl FnOnce(&mut PlanDoc) -> R<()>,
    ) -> R<EditOutcome> {
        let me = self.me_id()?;
        let it = self.item_state(item)?;
        let space = it.space.clone();
        let before = it.current().number;
        let ItemContent::Plan(mut doc) = it.current().content.clone() else {
            return Err(CoreError::Invalid {
                reason: t("este Item não é um plano", "this Item is not a plan"),
            });
        };
        let creator = it.versions[0].author.clone();
        let old_total = doc.total_cents();
        f(&mut doc)?;
        let new_total = doc.total_cents();
        let budget = doc.budget_cents;
        self.append(
            &space,
            &me,
            EventBody::ItemVersioned {
                item: item.into(),
                content: ItemContent::Plan(doc),
                note: note.clone(),
            },
        )?;

        // Reação determinística (regra do núcleo, não IA): quem cuida de dinheiro no
        // Espaço comenta quando o total muda. Preferimos o agente "Financeiro" se ele
        // for membro; senão, o agente que criou o plano.
        let mut reaction = None;
        if new_total != old_total {
            let watcher = self
                .space_state(&space)?
                .members
                .iter()
                .map(|(m, _)| m.clone())
                .find(|m| {
                    self.identities
                        .get(m)
                        .map(|i| i.kind == IdentityKind::Agent && i.handle == "financeiro")
                        .unwrap_or(false)
                })
                .or_else(|| {
                    self.identities
                        .get(&creator)
                        .filter(|i| i.kind == IdentityKind::Agent)
                        .map(|i| i.id.clone())
                });
            if let Some(agent) = watcher {
                let text = match budget {
                    Some(b) if new_total <= b => tr!("Recalculei: {} de {} · sobra {}.", "Recalculated: {} of {} · {} left.", money(new_total), money(b), money(b - new_total)),
                    Some(b) => tr!("Atenção: o plano passou {} do teto de {}. Quer que eu ache algo mais barato?", "Heads up: the plan is {} over the {} cap. Want me to find something cheaper?", money(new_total - b), money(b)),
                    None => tr!("Recalculei: total {}.", "Recalculated: total {}.", money(new_total)),
                };
                if self.agent_say(&space, &agent, &text, 0).is_ok() {
                    reaction = Some(text);
                }
            }
        }
        Ok(EditOutcome {
            item: self.item(item)?,
            undo: UndoToken {
                item_id: item.into(),
                restore_version: before,
                label: note,
            },
            reaction,
        })
    }

    pub fn edit_plan_line(
        &mut self,
        item: &str,
        line: &str,
        text: &str,
        cost_cents: i64,
    ) -> R<EditOutcome> {
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(CoreError::Invalid {
                reason: t("a linha precisa de um texto", "the line needs some text"),
            });
        }
        if cost_cents < 0 {
            return Err(CoreError::Invalid {
                reason: t("custo negativo", "negative cost"),
            });
        }
        let note = tr!("Editou “{}”", "Edited “{}”", text);
        self.edit_plan(item, note, |doc| {
            let l = doc
                .line_mut(line)
                .ok_or_else(|| not_found(&t("linha", "line")))?;
            l.text = text;
            l.cost_cents = cost_cents;
            Ok(())
        })
    }

    pub fn toggle_plan_line(&mut self, item: &str, line: &str) -> R<EditOutcome> {
        let (label, done) = {
            let it = self.item_state(item)?;
            match &it.current().content {
                ItemContent::Plan(p) => {
                    let l = p.line(line).ok_or_else(|| not_found(&t("linha", "line")))?;
                    (l.text.clone(), !l.done)
                }
                _ => {
                    return Err(CoreError::Invalid {
                        reason: t("não é um plano", "not a plan"),
                    })
                }
            }
        };
        let note = format!(
            "{} “{}”",
            if done {
                ts("Concluiu", "Completed")
            } else {
                ts("Reabriu", "Reopened")
            },
            label
        );
        self.edit_plan(item, note, |doc| {
            doc.line_mut(line)
                .ok_or_else(|| not_found(&t("linha", "line")))?
                .done = done;
            Ok(())
        })
    }

    pub fn add_plan_line(
        &mut self,
        item: &str,
        section: u32,
        text: &str,
        cost_cents: i64,
    ) -> R<EditOutcome> {
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(CoreError::Invalid {
                reason: t("a linha precisa de um texto", "the line needs some text"),
            });
        }
        let note = tr!("Adicionou “{}”", "Added “{}”", text);
        self.edit_plan(item, note, |doc| {
            let s = doc
                .sections
                .get_mut(section as usize)
                .ok_or_else(|| not_found(&t("seção", "section")))?;
            s.lines.push(PlanLine {
                id: new_id("ln"),
                text,
                cost_cents: cost_cents.max(0),
                done: false,
            });
            Ok(())
        })
    }

    pub fn remove_plan_line(&mut self, item: &str, line: &str) -> R<EditOutcome> {
        let label = match &self.item_state(item)?.current().content {
            ItemContent::Plan(p) => p
                .line(line)
                .map(|l| l.text.clone())
                .ok_or_else(|| not_found(&t("linha", "line")))?,
            _ => {
                return Err(CoreError::Invalid {
                    reason: t("não é um plano", "not a plan"),
                })
            }
        };
        // "Apagar" uma linha é reversível: a versão anterior continua no log.
        self.edit_plan(
            item,
            tr!("Removeu “{label}”", "Removed “{label}”"),
            |doc| {
                doc.remove_line(line)
                    .map(|_| ())
                    .ok_or_else(|| not_found(&t("linha", "line")))
            },
        )
    }

    /// Desfazer = nova versão igual à anterior. Nada é apagado.
    pub fn undo(&mut self, token: &UndoToken) -> R<ItemDetail> {
        self.restore_version(
            &token.item_id,
            token.restore_version,
            &tr!(
                "Desfez: {}",
                "Undid: {}",
                token
                    .label
                    .trim_start_matches("Desfez: ")
                    .trim_start_matches("Undid: ")
            ),
        )
    }

    pub fn restore_version(&mut self, item: &str, version: u32, note: &str) -> R<ItemDetail> {
        let me = self.me_id()?;
        let it = self.item_state(item)?;
        if !it.versions.iter().any(|v| v.number == version) {
            return Err(not_found(&t("versão", "version")));
        }
        if it.kind == ItemKind::Page {
            // A page's versions are Loro changes: restoring is a new change, not a copy.
            self.page_restore(item, version, note)?;
            return self.item(item);
        }
        let space = it.space.clone();
        self.append(
            &space,
            &me,
            EventBody::ItemReverted {
                item: item.into(),
                to_version: version,
                note: note.into(),
            },
        )?;
        self.item(item)
    }

    // ── pedidos (Atividade) ──

    pub fn requests(&self) -> Vec<AgentRequestDto> {
        let mut v: Vec<AgentRequestDto> = self
            .state
            .request_order
            .iter()
            .filter_map(|id| self.state.requests.get(id))
            .map(|r| self.request_dto(r))
            .collect();
        v.sort_by(|a, b| {
            let rank = |s: RequestStatus| match s {
                RequestStatus::Pending => 0,
                RequestStatus::Stale => 1,
                _ => 2,
            };
            rank(a.status).cmp(&rank(b.status)).then(
                b.resolved_ms
                    .unwrap_or(b.opened_ms)
                    .cmp(&a.resolved_ms.unwrap_or(a.opened_ms)),
            )
        });
        v
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the RequestOpened event fields one to one"
    )]
    pub fn open_request(
        &mut self,
        space: &str,
        agent: &str,
        title: &str,
        detail: &str,
        audience: &str,
        action: ActionClass,
        link: Option<(String, String)>,
    ) -> R<Option<String>> {
        self.open_request_at(
            space,
            agent,
            now_ms(),
            title,
            detail,
            audience,
            action,
            link,
        )
    }

    /// Abre um pedido **só se** o avaliador mandar pedir. `None` = o agente pode fazer direto.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_request_at(
        &mut self,
        space: &str,
        agent: &str,
        at_ms: i64,
        title: &str,
        detail: &str,
        audience: &str,
        action: ActionClass,
        link: Option<(String, String)>,
    ) -> R<Option<String>> {
        match self.decide(agent, space, &action, 0) {
            Decision::Act { .. } => Ok(None),
            Decision::Block(r) => Err(CoreError::Forbidden {
                reason: reason_label(&r, &self.policy),
            }),
            Decision::Request(_) => {
                let content_hash = match &link {
                    Some((item, line)) => match &self.item_state(item)?.current().content {
                        ItemContent::Plan(p) => Self::line_hash(
                            p.line(line).ok_or_else(|| not_found(&t("linha", "line")))?,
                        ),
                        _ => content_hash(&title),
                    },
                    None => content_hash(&(title, detail, audience)),
                };
                let req = AgentRequest {
                    id: new_id("rq"),
                    agent: agent.into(),
                    title: title.into(),
                    detail: detail.into(),
                    audience: audience.into(),
                    action,
                    content_hash,
                    item: link.as_ref().map(|l| l.0.clone()),
                    line: link.map(|l| l.1),
                };
                let id = req.id.clone();
                self.append_at(
                    space,
                    agent,
                    at_ms,
                    EventBody::RequestOpened { request: req },
                )?;
                Ok(Some(id))
            }
        }
    }

    pub fn resolve_request(&mut self, id: &str, approve: bool) -> R<ApproveOutcome> {
        let me = self.me_id()?;
        let r = self
            .state
            .requests
            .get(id)
            .cloned()
            .ok_or_else(|| not_found("pedido"))?;
        if r.status != ReqStatus::Pending {
            return Err(CoreError::Invalid {
                reason: t(
                    "este pedido já foi resolvido",
                    "this request was already resolved",
                ),
            });
        }
        if self
            .identities
            .get(&r.req.agent)
            .and_then(|a| a.owner.clone())
            .as_deref()
            != Some(me.as_str())
        {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só o dono do agente aprova os pedidos dele",
                    "only the agent’s owner can approve its requests",
                ),
            });
        }
        if approve && !self.request_is_current(&r.req) {
            return Err(CoreError::Stale { reason: t("O plano mudou depois do pedido. A aprovação vale só para o conteúdo exato — peça de novo.", "The plan changed after the request. Approval only covers the exact content — ask again.") });
        }
        self.append(
            &r.space,
            &me,
            EventBody::RequestResolved {
                request: id.into(),
                approved: approve,
                content_hash: r.req.content_hash.clone(),
            },
        )?;
        let agent = r.req.agent.clone();
        let message = if approve {
            // Executa o que foi aprovado. Pagamento/envio de verdade não existem neste
            // protótipo: o agente marca a linha do plano e diz que foi simulado.
            if let (Some(item), Some(line)) = (&r.req.item, &r.req.line) {
                if let Ok(it) = self.item_state(item) {
                    if let ItemContent::Plan(mut doc) = it.current().content.clone() {
                        if let Some(l) = doc.line_mut(line) {
                            l.done = true;
                            let note = tr!("{}: feito", "{}: done", r.req.title);
                            self.append(
                                &r.space,
                                &agent,
                                EventBody::ItemVersioned {
                                    item: item.clone(),
                                    content: ItemContent::Plan(doc),
                                    note,
                                },
                            )?;
                        }
                    }
                }
            }
            let text = tr!(
                "Feito: {}. (Simulação — nenhum pagamento ou envio real neste protótipo.)",
                "Done: {}. (Simulated — no real payment or message in this prototype.)",
                r.req.title.to_lowercase_first()
            );
            let _ = self.post_as(&r.space, &agent, &text, None);
            text
        } else {
            let text = tr!(
                "Ok, não vou {}.",
                "Ok, I won’t {}.",
                r.req.title.to_lowercase_first()
            );
            let _ = self.post_as(&r.space, &agent, &text, None);
            text
        };
        let req = self
            .state
            .requests
            .get(id)
            .map(|r| self.request_dto(r))
            .ok_or_else(|| not_found("pedido"))?;
        Ok(ApproveOutcome {
            request: req,
            message,
        })
    }

    /// One swipe on the approvals stack. "Sempre" swipes also issue a standing Grant
    /// (agent + kind of action + this Space) and settle the other pending requests it
    /// covers; from then on `decide` applies it without asking.
    pub fn decide_request(&mut self, id: &str, decision: RequestDecision) -> R<DecideOutcome> {
        let r = self
            .state
            .requests
            .get(id)
            .cloned()
            .ok_or_else(|| not_found("pedido"))?;
        let (approve, standing) = match decision {
            RequestDecision::Approve => (true, false),
            RequestDecision::Deny => (false, false),
            RequestDecision::AlwaysApprove => (true, true),
            RequestDecision::AlwaysDeny => (false, true),
        };
        if standing && approve && !standing_allow_permitted(&r.req.action, &self.policy) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Linha vermelha: isso sempre pede. Dá pra aprovar só esta vez.",
                    "Red line: this always asks. You can approve just this once.",
                ),
            });
        }
        // Resolve first: it checks ownership and that the content is still current.
        let out = self.resolve_request(id, approve)?;
        let mut grant_id = None;
        let mut also = 0u32;
        if standing {
            let me = self.me_id()?;
            let grant = Grant {
                id: new_id("gr"),
                grantor: me.clone(),
                grantee: Some(r.req.agent.clone()),
                scope: GrantScope::Space(r.space.clone()),
                capability: Capability::Standing {
                    action: standing_key(&r.req.action).into(),
                    allow: approve,
                },
                expires_at_ms: None,
            };
            grant_id = Some(grant.id.clone());
            self.append(&r.space, &me, EventBody::GrantIssued { grant })?;
            let key = standing_key(&r.req.action);
            let covered: Vec<String> = self
                .state
                .request_order
                .iter()
                .filter_map(|rid| self.state.requests.get(rid))
                .filter(|o| {
                    o.status == ReqStatus::Pending
                        && o.req.agent == r.req.agent
                        && o.space == r.space
                        && standing_key(&o.req.action) == key
                        && (!approve
                            || (standing_allow_permitted(&o.req.action, &self.policy)
                                && self.request_is_current(&o.req)))
                })
                .map(|o| o.req.id.clone())
                .collect();
            for rid in covered {
                if self.resolve_request(&rid, approve).is_ok() {
                    also += 1;
                }
            }
        }
        let request = self
            .state
            .requests
            .get(id)
            .map(|r| self.request_dto(r))
            .ok_or_else(|| not_found("pedido"))?;
        Ok(DecideOutcome {
            request,
            message: out.message,
            standing_grant_id: grant_id,
            also_resolved: also,
        })
    }

    /// Every standing decision you gave your agents (newest first), to show and revoke.
    pub fn standing_decisions(&self) -> Vec<StandingDecisionDto> {
        let Ok(me) = self.me_id() else { return vec![] };
        self.state
            .grants
            .iter()
            .rev()
            .filter(|g| !g.revoked && g.grant.grantor == me)
            .filter_map(|g| {
                let Capability::Standing { action, allow } = &g.grant.capability else {
                    return None;
                };
                let GrantScope::Space(space) = &g.grant.scope else {
                    return None;
                };
                let agent = g.grant.grantee.as_deref()?;
                Some(StandingDecisionDto {
                    grant_id: g.grant.id.clone(),
                    agent: self.persona(agent),
                    space_id: space.clone(),
                    space_title: self
                        .state
                        .spaces
                        .get(space)
                        .map(|s| s.title.clone())
                        .unwrap_or_default(),
                    action_key: action.clone(),
                    action_label: standing_label(action),
                    allow: *allow,
                    at_ms: g.at_ms,
                })
            })
            .collect()
    }

    /// Revokes a standing decision: the agent asks again next time.
    pub fn revoke_standing(&mut self, grant_id: &str) -> R<()> {
        let g = self
            .state
            .grants
            .iter()
            .find(|g| {
                g.grant.id == grant_id
                    && !g.revoked
                    && matches!(g.grant.capability, Capability::Standing { .. })
            })
            .ok_or_else(|| not_found(&t("decisão", "decision")))?;
        let GrantScope::Space(space) = g.grant.scope.clone() else {
            return Err(CoreError::Invalid {
                reason: t("escopo inválido", "invalid scope"),
            });
        };
        let me = self.me_id()?;
        if g.grant.grantor != me {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só quem decidiu pode revogar",
                    "only whoever decided can revoke it",
                ),
            });
        }
        self.append(
            &space,
            &me,
            EventBody::GrantRevoked {
                grant: grant_id.into(),
            },
        )?;
        Ok(())
    }

    /// Demonstration (showcase seed only): one of your agents asks for something, as if
    /// it came from its run. Goes through the evaluator like any request.
    pub fn demo_open_request(
        &mut self,
        space: &str,
        agent_handle: &str,
        title: &str,
        detail: &str,
        audience: &str,
        action: ActionClass,
    ) -> R<Option<String>> {
        let agent = self
            .identities
            .values()
            .find(|i| i.handle == agent_handle)
            .map(|i| i.id.clone())
            .ok_or_else(|| not_found(&t("agente", "agent")))?;
        self.open_request_at(
            space,
            &agent,
            now_ms(),
            title,
            detail,
            audience,
            action,
            None,
        )
    }

    pub fn approve_all(&mut self, agent: &str) -> R<u32> {
        let ids: Vec<String> = self
            .state
            .request_order
            .iter()
            .filter(|id| {
                self.state
                    .requests
                    .get(*id)
                    .map(|r| {
                        r.req.agent == agent
                            && r.status == ReqStatus::Pending
                            && self.request_is_current(&r.req)
                    })
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        let mut n = 0;
        for id in ids {
            if self.resolve_request(&id, true).is_ok() {
                n += 1;
            }
        }
        Ok(n)
    }

    // ── agentes e Concessões ──

    pub fn agents(&self) -> Vec<AgentProfile> {
        self.identity_order
            .iter()
            .filter_map(|id| self.identities.get(id))
            .filter(|i| i.kind == IdentityKind::Agent)
            .map(|i| self.agent_profile(&i.id))
            .collect()
    }

    pub fn agent_profile(&self, agent: &str) -> AgentProfile {
        let budget = self.budget(agent);
        let spaces = self
            .space_order
            .iter()
            .filter_map(|s| self.state.spaces.get(s))
            .filter(|s| s.kind != SpaceKind::Personal && s.members.iter().any(|(m, _)| m == agent))
            .map(|s| AgentSpaceTrust {
                space_id: s.id.clone(),
                space_title: self
                    .space_summary(&s.id)
                    .map(|x| x.title)
                    .unwrap_or(s.title.clone()),
                level: trust_dto(self.trust(agent, &s.id)),
            })
            .collect();
        let resolved: Vec<&RequestState> = self
            .state
            .request_order
            .iter()
            .filter_map(|r| self.state.requests.get(r))
            .filter(|r| r.req.agent == agent)
            .collect();
        let streak = self
            .state
            .resolved_order
            .iter()
            .rev()
            .filter_map(|r| self.state.requests.get(r))
            .filter(|r| r.req.agent == agent)
            .take_while(|r| r.status == ReqStatus::Approved)
            .count() as u32;
        AgentProfile {
            persona: self.persona(agent),
            budget_limit_cents: budget.map(|b| b.limit_cents),
            budget_spent_cents: budget.map(|b| b.spent_cents),
            near_limit: budget.map(|b| b.near_limit()).unwrap_or(false),
            spaces,
            pending_requests: resolved
                .iter()
                .filter(|r| r.status == ReqStatus::Pending)
                .count() as u32,
            approvals_streak: streak,
            runs_on: t(
                "Neste aparelho · Apple Intelligence quando disponível",
                "On this device · Apple Intelligence when available",
            ),
        }
    }

    pub fn set_trust(&mut self, agent: &str, space: &str, level: TrustLevel) -> R<()> {
        let me = self.me_id()?;
        let owner = self.identities.get(agent).and_then(|a| a.owner.clone());
        if owner.as_deref() != Some(me.as_str()) {
            let owner_name = owner.map(|o| self.persona(&o).name).unwrap_or_default();
            return Err(CoreError::Forbidden {
                reason: tr!(
                    "Só {owner_name} muda a confiança deste agente.",
                    "Only {owner_name} can change this agent’s trust."
                ),
            });
        }
        if !self.is_member(space, agent) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "o agente não é membro deste Espaço",
                    "the agent is not a member of this Space",
                ),
            });
        }
        let grant = Grant {
            id: new_id("gr"),
            grantor: me.clone(),
            grantee: Some(agent.into()),
            scope: GrantScope::Space(space.into()),
            capability: Capability::Trust(level),
            expires_at_ms: None,
        };
        self.append(space, &me, EventBody::GrantIssued { grant })?;
        Ok(())
    }

    pub fn raise_budget(&mut self, agent: &str, extra_cents: i64) -> R<AgentProfile> {
        let me = self.me_id()?;
        let current = self.budget(agent).ok_or_else(|| CoreError::Forbidden {
            reason: t(
                "este orçamento é de outra pessoa",
                "this budget belongs to someone else",
            ),
        })?;
        let personal = self
            .personal_space()
            .ok_or_else(|| not_found(&t("Espaço pessoal", "Personal space")))?;
        let grant = Grant {
            id: new_id("gr"),
            grantor: me.clone(),
            grantee: Some(agent.into()),
            scope: GrantScope::Everywhere,
            capability: Capability::MonthlyBudget {
                cents: current.limit_cents + extra_cents.max(0),
            },
            expires_at_ms: None,
        };
        self.append(&personal, &me, EventBody::GrantIssued { grant })?;
        Ok(self.agent_profile(agent))
    }

    /// Mostra, sem executar nada, o que o avaliador decidiria para ações típicas.
    pub fn preview_decisions(&self, agent: &str, space: &str) -> Vec<DecisionPreview> {
        let name = self.persona(agent).name;
        let cases: Vec<(&str, &str, ActionClass)> = vec![
            (
                ts("Responder", "Reply"),
                ts("Responder quando chamado", "Reply when called"),
                ActionClass::Reply,
            ),
            (
                ts("Editar o plano", "Edit the plan"),
                ts(
                    "Trocar o restaurante do jantar",
                    "Swap the dinner restaurant",
                ),
                ActionClass::Reversible,
            ),
            (
                ts("Mandar para fora", "Send outside"),
                ts(
                    "Enviar o roteiro por WhatsApp",
                    "Send the itinerary on WhatsApp",
                ),
                ActionClass::External,
            ),
            (
                ts("Pagamento pequeno", "Small payment"),
                ts("Pagar estacionamento de R$ 40", "Pay $40 for parking"),
                ActionClass::Money { cents: 4_000 },
            ),
            (
                ts("Pagamento grande", "Large payment"),
                ts("Reservar a pousada por R$ 640", "Book the inn for $640"),
                ActionClass::Money { cents: 64_000 },
            ),
            (
                ts("Apagar de verdade", "Delete for real"),
                ts("Apagar o histórico do plano", "Delete the plan’s history"),
                ActionClass::Irreversible,
            ),
            (
                ts("Publicar", "Publish"),
                ts(
                    "Postar o roteiro no seu perfil público",
                    "Post the itinerary on your public profile",
                ),
                ActionClass::PublicAudience,
            ),
        ];
        cases
            .into_iter()
            .map(|(action, example, class)| {
                let red_line = matches!(class, ActionClass::PublicAudience | ActionClass::ThirdPartyData)
                    || matches!(class, ActionClass::Money { cents } if cents > self.policy.money_ceiling_cents);
                let d = self.decide(agent, space, &class, 0);
                let (kind, explanation) = match &d {
                    Decision::Act { undoable: true } => (DecisionKind::ActWithUndo, tr!("{name} faz e você pode desfazer", "{name} does it and you can undo")),
                    Decision::Act { undoable: false } => (DecisionKind::Act, tr!("{name} faz sozinho", "{name} does it alone")),
                    Decision::Request(r) => (DecisionKind::Request, reason_label(r, &self.policy)),
                    Decision::Block(r) => (DecisionKind::Block, reason_label(r, &self.policy)),
                };
                DecisionPreview { action: action.into(), example: example.into(), kind, explanation, red_line }
            })
            .collect()
    }

    // ── integridade, busca, menções ──

    pub fn verify_log(&self, space: &str) -> LogReport {
        let title = self
            .space_summary(space)
            .map(|s| s.title)
            .or_else(|_| self.space_state(space).map(|s| s.title.clone()))
            .unwrap_or_default();
        // Reverifica a partir do DISCO, não da memória: pega adulteração no arquivo.
        let result: Result<SpaceLog, String> = self
            .store
            .events(space)
            .map_err(|e| e.to_string())
            .and_then(|events| {
                SpaceLog::from_events(space.to_string(), events)
                    .map_err(|e: LogError| e.to_string())
            });
        match result {
            Ok(log) => LogReport {
                space_id: space.into(),
                space_title: title,
                events: log.len() as u64,
                head_hash: log.head_hash().into(),
                valid: true,
                error: None,
            },
            Err(err) => LogReport {
                space_id: space.into(),
                space_title: title,
                events: 0,
                head_hash: String::new(),
                valid: false,
                error: Some(err),
            },
        }
    }

    pub fn verify_all(&self) -> Vec<LogReport> {
        self.space_order
            .iter()
            .map(|s| self.verify_log(s))
            .collect()
    }

    /// Ações de um agente em todos os Espaços locais, mais recentes primeiro.
    /// O custo vem do `UsageRecorded` que o mesmo agente assinou logo em seguida.
    pub fn agent_activity(&self, agent: &str, limit: usize) -> R<Vec<AgentActivityDto>> {
        if !self.identities.contains_key(agent) {
            return Err(not_found("agente"));
        }
        let mut out: Vec<AgentActivityDto> = Vec::new();
        for (space, log) in &self.logs {
            let title = self
                .space_summary(space)
                .map(|s| s.title)
                .unwrap_or_default();
            let evs = log.events();
            for (i, e) in evs.iter().enumerate() {
                if e.author != agent {
                    continue;
                }
                let (label, detail) = match &e.body {
                    EventBody::MessagePosted { text, attaches, .. } => {
                        let short: String = text.chars().take(90).collect();
                        (
                            if attaches.is_some() {
                                t("Entregou um Item", "Delivered an Item")
                            } else {
                                t("Respondeu", "Replied")
                            },
                            short,
                        )
                    }
                    EventBody::ItemCreated { content, .. } => {
                        (t("Criou", "Created"), content.title())
                    }
                    EventBody::ItemVersioned { note, .. } => (t("Editou", "Edited"), note.clone()),
                    EventBody::RequestOpened { request } => (
                        t("Pediu aprovação", "Asked for approval"),
                        request.title.clone(),
                    ),
                    _ => continue,
                };
                let cost = evs[i + 1..].iter().take(3).find_map(|n| match &n.body {
                    EventBody::UsageRecorded {
                        agent: a, cents, ..
                    } if a == agent && n.author == agent => Some(*cents),
                    _ => None,
                });
                out.push(AgentActivityDto {
                    label,
                    detail,
                    space_id: space.clone(),
                    space_title: title.clone(),
                    at_ms: e.at_ms,
                    cost_cents: cost,
                });
            }
        }
        out.sort_by_key(|a| std::cmp::Reverse(a.at_ms));
        out.truncate(limit);
        Ok(out)
    }

    pub fn log_events(&self, space: &str) -> R<Vec<LogEventDto>> {
        let log = self.logs.get(space).ok_or_else(|| not_found("log"))?;
        Ok(log
            .events()
            .iter()
            .rev()
            .map(|e| LogEventDto {
                seq: e.seq,
                label: event_label(&e.body),
                author: self.persona(&e.author),
                at_ms: e.at_ms,
                hash: e.hash.clone(),
                prev: e.prev.clone(),
                signature: e.sig.clone(),
            })
            .collect())
    }

    pub fn search(&self, query: &str) -> Vec<SearchHit> {
        let q = fold(query.trim());
        if q.is_empty() {
            return vec![];
        }
        let mut hits = Vec::new();
        for it in self.state.items.values() {
            let v = it.current();
            let mut hay = v.content.title();
            if let ItemContent::Plan(p) = &v.content {
                for l in p.sections.iter().flat_map(|s| &s.lines) {
                    hay.push_str(" · ");
                    hay.push_str(&l.text);
                }
            }
            if fold(&hay).contains(&q) {
                let snippet = match &v.content {
                    ItemContent::Plan(p) => p
                        .sections
                        .iter()
                        .flat_map(|s| &s.lines)
                        .find(|l| fold(&l.text).contains(&q))
                        .map(|l| format!("{} · {}", l.text, money(l.cost_cents)))
                        .unwrap_or(p.summary.clone()),
                    ItemContent::Text { text } => text.clone(),
                    ItemContent::Page(_) => self.page_text(&it.id),
                    ItemContent::File(f) => f.name.clone(),
                    ItemContent::App(a) => {
                        apps::headline(
                            &a.app,
                            &serde_json::from_str(&a.state_json).unwrap_or(Value::Null),
                            now_ms(),
                        )
                        .0
                    }
                };
                hits.push(SearchHit {
                    space_id: it.space.clone(),
                    space_title: self
                        .space_summary(&it.space)
                        .map(|s| s.title)
                        .unwrap_or_default(),
                    item_id: Some(it.id.clone()),
                    title: v.content.title(),
                    snippet,
                    author: self.persona(&v.author),
                    at_ms: v.at_ms,
                });
            }
        }
        for s in self
            .state
            .spaces
            .values()
            .filter(|s| s.kind != SpaceKind::Personal)
        {
            for e in &s.entries {
                if let EntryBody::Message { text, .. } = &e.body {
                    if fold(text).contains(&q) {
                        hits.push(SearchHit {
                            space_id: s.id.clone(),
                            space_title: self
                                .space_summary(&s.id)
                                .map(|x| x.title)
                                .unwrap_or_default(),
                            item_id: None,
                            title: self.persona(&e.author).name,
                            snippet: text.clone(),
                            author: self.persona(&e.author),
                            at_ms: e.at_ms,
                        });
                    }
                }
            }
        }
        hits.sort_by(|a, b| {
            b.item_id
                .is_some()
                .cmp(&a.item_id.is_some())
                .then(b.at_ms.cmp(&a.at_ms))
        });
        hits.truncate(40);
        hits
    }

    pub fn mentions(&self) -> Vec<Mention> {
        let Some(me) = self.me.clone() else {
            return vec![];
        };
        let handle = format!("@{}", self.persona(&me).name.to_lowercase());
        let mut out = Vec::new();
        for s in self.state.spaces.values() {
            for e in &s.entries {
                if e.author == me {
                    continue;
                }
                if let EntryBody::Message { text, .. } = &e.body {
                    if text.to_lowercase().contains(&handle) {
                        out.push(Mention {
                            space_id: s.id.clone(),
                            space_title: self
                                .space_summary(&s.id)
                                .map(|x| x.title)
                                .unwrap_or_default(),
                            entry: self.entry_dto(e),
                        });
                    }
                }
            }
        }
        out.sort_by_key(|x| std::cmp::Reverse(x.entry.at_ms));
        out
    }

    pub fn stats(&self) -> CoreStats {
        CoreStats {
            spaces: self
                .state
                .spaces
                .values()
                .filter(|s| s.kind != SpaceKind::Personal)
                .count() as u32,
            events: self.store.event_count().unwrap_or(0),
            items: self.state.items.len() as u32,
            identities: self.identities.len() as u32,
            all_logs_valid: self.verify_all().iter().all(|r| r.valid),
            db_path: self.db_path.clone(),
        }
    }

    pub fn invite_link(&mut self, space: &str) -> R<String> {
        let me = self.me_id()?;
        let grant = Grant {
            id: new_secret_id("inv"),
            grantor: me.clone(),
            grantee: None,
            scope: GrantScope::Space(space.into()),
            capability: Capability::Invite { role: Role::Member },
            expires_at_ms: Some(now_ms() + 7 * 86_400_000),
        };
        let id = grant.id.clone();
        self.append(space, &me, EventBody::GrantIssued { grant })?;
        Ok(format!(
            "https://roda.app/c/{}#{}",
            &space[3..15.min(space.len())],
            &id[4..20.min(id.len())]
        ))
    }

    // ── MCP Apps: o núcleo é o servidor MCP local dos mini-apps embutidos ──

    // ── Native capabilities for mini-apps (photos, location, calendar…) ──
    //
    // One Grant per mini-app Item, from you to `app:<item>`, logged in that Space's event
    // log. "Allow once" = the same Grant expiring in a minute. Revoking appends GrantRevoked.

    fn device_grants_for(&self, item: &str) -> impl Iterator<Item = &GrantState> {
        let grantee = format!("app:{item}");
        let now = now_ms();
        self.state.grants.iter().rev().filter(move |g| {
            !g.revoked
                && g.grant.grantee.as_deref() == Some(grantee.as_str())
                && g.grant.expires_at_ms.map(|e| e > now).unwrap_or(true)
                && matches!(g.grant.capability, Capability::Device { .. })
        })
    }

    pub fn app_device_allowed(&self, item: &str, capability: &str) -> bool {
        self.device_grants_for(item).any(|g| matches!(&g.grant.capability, Capability::Device { capability: c, .. } if c == capability))
    }

    pub fn grant_app_device(
        &mut self,
        item: &str,
        capability: &str,
        purpose: &str,
        always: bool,
    ) -> R<String> {
        let space = self
            .state
            .items
            .get(item)
            .map(|it| it.space.clone())
            .ok_or_else(|| not_found(&t("mini-app", "mini-app")))?;
        if !matches!(
            self.state.items.get(item).map(|it| &it.current().content),
            Some(ItemContent::App(_))
        ) {
            return Err(CoreError::Invalid {
                reason: t("não é um mini-app", "not a mini-app"),
            });
        }
        let cap = capability.trim();
        if cap.is_empty()
            || cap.len() > 64
            || !cap
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".:-_".contains(c))
        {
            return Err(CoreError::Invalid {
                reason: t("capacidade inválida", "invalid capability"),
            });
        }
        let me = self.me_id()?;
        let grant = Grant {
            id: new_id("gr"),
            grantor: me.clone(),
            grantee: Some(format!("app:{item}")),
            scope: GrantScope::Item(item.into()),
            capability: Capability::Device {
                capability: cap.into(),
                purpose: purpose.chars().take(160).collect(),
            },
            expires_at_ms: if always {
                None
            } else {
                Some(now_ms() + 60_000)
            },
        };
        let id = grant.id.clone();
        self.append(&space, &me, EventBody::GrantIssued { grant })?;
        Ok(id)
    }

    pub fn revoke_app_device(&mut self, grant_id: &str) -> R<()> {
        let g = self
            .state
            .grants
            .iter()
            .find(|g| g.grant.id == grant_id && !g.revoked)
            .ok_or_else(|| not_found(&t("concessão", "grant")))?;
        let GrantScope::Item(item) = &g.grant.scope else {
            return Err(CoreError::Invalid {
                reason: t("não é de um mini-app", "not a mini-app grant"),
            });
        };
        let space = self
            .state
            .items
            .get(item)
            .map(|it| it.space.clone())
            .ok_or_else(|| not_found(&t("mini-app", "mini-app")))?;
        let me = self.me_id()?;
        self.append(
            &space,
            &me,
            EventBody::GrantRevoked {
                grant: grant_id.into(),
            },
        )?;
        Ok(())
    }

    pub fn app_device_grants(&self) -> Vec<DeviceGrantDto> {
        let now = now_ms();
        self.state
            .grants
            .iter()
            .rev()
            .filter(|g| !g.revoked && g.grant.expires_at_ms.map(|e| e > now).unwrap_or(true))
            .filter_map(|g| {
                let Capability::Device {
                    capability,
                    purpose,
                } = &g.grant.capability
                else {
                    return None;
                };
                let GrantScope::Item(item) = &g.grant.scope else {
                    return None;
                };
                let it = self.state.items.get(item)?;
                let ItemContent::App(doc) = &it.current().content else {
                    return None;
                };
                Some(DeviceGrantDto {
                    grant_id: g.grant.id.clone(),
                    item_id: item.clone(),
                    item_title: doc.title.clone(),
                    app_name: apps::spec(&doc.app)
                        .map(|s| s.name.to_string())
                        .unwrap_or_else(|| doc.app.clone()),
                    space_title: self
                        .state
                        .spaces
                        .get(&it.space)
                        .map(|s| s.title.clone())
                        .unwrap_or_default(),
                    capability: capability.clone(),
                    purpose: purpose.clone(),
                    always: g.grant.expires_at_ms.is_none(),
                    at_ms: g.at_ms,
                })
            })
            .collect()
    }

    fn app_state_dto(&self, it: &ItemState) -> Option<AppStateDto> {
        let ItemContent::App(doc) = &it.current().content else {
            return None;
        };
        let state: Value = serde_json::from_str(&doc.state_json).unwrap_or(Value::Null);
        let now = now_ms();
        let (headline, metrics) = apps::headline(&doc.app, &state, now);
        Some(AppStateDto {
            app_id: doc.app.clone(),
            resource_uri: doc.resource_uri.clone(),
            name: apps::spec(&doc.app)
                .map(|s| s.name.to_string())
                .unwrap_or_default(),
            view_json: apps::view(&doc.app, &state, now).to_string(),
            headline,
            metrics: metrics
                .into_iter()
                .map(|(label, value)| AppMetricDto { label, value })
                .collect(),
            last_action: apps::last_action(&state),
            trust: trust_dto(self.app_trust(&it.id)),
            snapshot_json: apps::snapshot(&doc.app, &state, &it.id, now)
                .map(|v| v.to_string())
                .unwrap_or_default(),
        })
    }

    /// Nível da Concessão do mini-app neste Item (sem Concessão: só ouvir).
    fn app_trust(&self, item: &str) -> TrustLevel {
        let grantee = format!("app:{item}");
        self.state
            .grants
            .iter()
            .rev()
            .filter(|g| {
                !g.revoked
                    && g.grant.grantee.as_deref() == Some(grantee.as_str())
                    && g.grant.scope == GrantScope::Item(item.to_string())
            })
            .find_map(|g| match g.grant.capability {
                Capability::Trust(l) => Some(l),
                _ => None,
            })
            .unwrap_or(TrustLevel::Listen)
    }

    pub fn app_specs(&self) -> Vec<AppSpecDto> {
        apps::specs()
            .into_iter()
            .map(|s| AppSpecDto {
                id: s.id.into(),
                name: s.name.into(),
                description: s.description.into(),
                resource_uri: s.resource_uri.into(),
                has_view: s.html.is_some(),
                tools: s
                    .tools
                    .into_iter()
                    .map(|t| AppToolDto {
                        name: t.name.into(),
                        title: t.title.into(),
                        description: t.description.into(),
                        input_schema_json: t.schema.to_string(),
                        visibility: t.visibility.iter().map(|v| v.to_string()).collect(),
                        action_label: action_label(&t.action).into(),
                        read_only: t.read_only,
                    })
                    .collect(),
            })
            .collect()
    }

    /// `resources/read` de um recurso `ui://` embutido.
    pub fn read_app_resource(&self, uri: &str) -> R<AppResourceDto> {
        let spec = apps::spec_for_uri(uri)
            .ok_or_else(|| not_found(&t("recurso de interface", "UI resource")))?;
        let html = spec.html.ok_or_else(|| {
            not_found(&t(
                "View MCP (este mini-app é desenhado nativamente)",
                "MCP View (this mini-app is drawn natively)",
            ))
        })?;
        Ok(AppResourceDto {
            uri: uri.into(),
            mime_type: apps::MIME.into(),
            text: html.into(),
            prefers_border: false,
            manifest_json: spec.manifest.unwrap_or("").into(),
        })
    }

    /// O agente chama a ferramenta `model` de um mini-app (ex.: `adopt_pet`): o núcleo
    /// avalia a Concessão do agente, cria o Item, concede ao mini-app "Agir" só neste
    /// Item e o agente posta o cartão na conversa.
    pub fn agent_create_app(
        &mut self,
        space: &str,
        agent: &str,
        start_tool: &str,
        args_json: &str,
        engine_label: &str,
        prompt: &str,
    ) -> R<PlanOutcome> {
        if !self.is_member(space, agent) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "o agente não é membro deste Espaço",
                    "the agent is not a member of this Space",
                ),
            });
        }
        let spec = apps::spec_for_start_tool(start_tool).ok_or_else(|| CoreError::Invalid {
            reason: tr!(
                "ferramenta desconhecida: {start_tool}",
                "unknown tool: {start_tool}"
            ),
        })?;
        let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
        let agent_name = self.persona(agent).name;
        let (title, state) = apps::create(spec.id, &args, &agent_name, now_ms())
            .map_err(|reason| CoreError::Invalid { reason })?;
        let doc = AppDoc {
            app: spec.id.into(),
            resource_uri: spec.resource_uri.into(),
            title: title.clone(),
            state_json: state.to_string(),
        };
        match self.decide(agent, space, &ActionClass::Reversible, 0) {
            Decision::Block(reason) => Ok(PlanOutcome {
                kind: DecisionKind::Block,
                item: None,
                message: reason_label(&reason, &self.policy),
            }),
            Decision::Request(reason) => {
                let req = AgentRequest {
                    id: new_id("rq"),
                    agent: agent.into(),
                    title: tr!(
                        "Criar o mini-app “{title}”",
                        "Create the mini-app “{title}”"
                    ),
                    detail: prompt.into(),
                    audience: t("Este Espaço", "This Space"),
                    action: ActionClass::Reversible,
                    content_hash: content_hash(&doc),
                    item: None,
                    line: None,
                };
                self.append(space, agent, EventBody::RequestOpened { request: req })?;
                Ok(PlanOutcome { kind: DecisionKind::Request, item: None, message: tr!("{agent_name} preparou um mini-app e pediu sua aprovação em Atividade · {}", "{agent_name} prepared a mini-app and asked for your approval in Activity · {}", reason_label(&reason, &self.policy)) })
            }
            Decision::Act { .. } => {
                let item = new_id("it");
                self.append(
                    space,
                    agent,
                    EventBody::ItemCreated {
                        item: item.clone(),
                        kind: ItemKind::App,
                        content: ItemContent::App(doc),
                        origin: tr!(
                            "{engine_label} · {} · a partir de “{prompt}”",
                            "{engine_label} · {} · from “{prompt}”",
                            spec.resource_uri
                        ),
                    },
                )?;
                // A Concessão do mini-app: agir (reversível) só neste Item, emitida por você.
                let me = self.me_id()?;
                let grant = Grant {
                    id: new_id("gr"),
                    grantor: me.clone(),
                    grantee: Some(format!("app:{item}")),
                    scope: GrantScope::Item(item.clone()),
                    capability: Capability::Trust(TrustLevel::Act),
                    expires_at_ms: None,
                };
                self.append(space, &me, EventBody::GrantIssued { grant })?;
                let text = match spec.id {
                    "pet" => t("Ele mora aqui agora. Dá para dar comida, jogar a Corrida do Jumento e revezar quem cuida dele.", "He lives here now. You can feed him, play Donkey Dash and take turns looking after him."),
                    "maptap" => t("MapTap. Os mesmos lugares para todo mundo. Um toque cada.", "MapTap. Same places for everyone. One tap each."),
                    "recipe" => tr!("Salvei a {}: vegetariana, 35 minutos, dá para ajustar as porções.", "Saved the {}: vegetarian, 35 minutes, servings adjustable.", title.to_lowercase_first()),
                    "hike" => t("Separei três trilhas boas pra sábado, com mapa e fotos. Comparem e votem: quando fechar, eu monto o roteiro com as caronas.", "Found three good trails for Saturday, with maps and photos. Compare and vote: once it’s locked in, I’ll plan the day and the rides."),
                    "countdown" => t("Contagem regressiva criada.", "Countdown’s up."),
                    "poll" => t("Abri uma enquete. Quando fechar, a vencedora entra no plano.", "Poll’s open. When it closes, the winner goes into the plan."),
                    _ => tr!("Criei a lista “{title}”. Marquem o que vocês levam.", "Made the list “{title}”. Check off what you’re bringing."),
                };
                self.post_as(space, agent, &text, Some(item.clone()))?;
                Ok(PlanOutcome {
                    kind: DecisionKind::ActWithUndo,
                    item: Some(self.item(&item)?),
                    message: text,
                })
            }
        }
    }

    pub fn install_app(&mut self, space: &str, app_id: &str, args_json: &str) -> R<ItemDetail> {
        let me = self.me_id()?;
        if !self.is_member(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "você não é membro deste Espaço",
                    "you are not a member of this Space",
                ),
            });
        }
        let spec = apps::specs()
            .into_iter()
            .find(|spec| spec.id == app_id)
            .ok_or_else(|| CoreError::Invalid {
                reason: t("mini-app desconhecido", "unknown mini-app"),
            })?;
        let args: Value = serde_json::from_str(args_json).map_err(|_| CoreError::Invalid {
            reason: t("argumentos inválidos", "invalid arguments"),
        })?;
        if !args.is_object() {
            return Err(CoreError::Invalid {
                reason: t("argumentos inválidos", "invalid arguments"),
            });
        }
        let (title, state) = apps::create(spec.id, &args, &self.persona(&me).name, now_ms())
            .map_err(|reason| CoreError::Invalid { reason })?;
        let item = new_id("it");
        self.append(
            space,
            &me,
            EventBody::ItemCreated {
                item: item.clone(),
                kind: ItemKind::App,
                content: ItemContent::App(AppDoc {
                    app: spec.id.into(),
                    resource_uri: spec.resource_uri.into(),
                    title,
                    state_json: state.to_string(),
                }),
                origin: t("Instalado por você", "Installed by you"),
            },
        )?;
        self.append(
            space,
            &me,
            EventBody::GrantIssued {
                grant: Grant {
                    id: new_id("gr"),
                    grantor: me.clone(),
                    grantee: Some(format!("app:{item}")),
                    scope: GrantScope::Item(item.clone()),
                    capability: Capability::Trust(TrustLevel::Act),
                    expires_at_ms: None,
                },
            },
        )?;
        self.post_as(
            space,
            &me,
            &t(
                "Mini-app adicionado a este Espaço.",
                "Mini-app added to this Space.",
            ),
            Some(item.clone()),
        )?;
        self.item(&item)
    }

    /// `tools/call` vindo da interface de um mini-app. Toda chamada passa pelo avaliador
    /// de Concessões: reversível roda (e vira versão do Item); irreversível ou externo
    /// volta como `NeedsConfirmation` e só roda com `confirmed = true` (a folha nativa).
    pub fn app_call_tool(
        &mut self,
        item: &str,
        tool_name: &str,
        args_json: &str,
        confirmed: bool,
    ) -> R<AppCallOutcome> {
        let me = self.me_id()?;
        self.app_call_as(item, &me, tool_name, args_json, confirmed)
    }

    /// Demonstração: a ação de outro membro chegando como se viesse do aparelho dele
    /// (assinada pela identidade dele no log). No produto isto vem pela sincronização.
    pub fn demo_member_app_call(
        &mut self,
        item: &str,
        member_handle: &str,
        tool_name: &str,
        args_json: &str,
    ) -> R<AppCallOutcome> {
        let who = self.member_by_handle(item, member_handle)?;
        self.app_call_as(item, &who, tool_name, args_json, false)
    }

    /// Demonstração: mensagem de outro membro (como se tivesse chegado pela sincronização).
    pub fn demo_member_say(
        &mut self,
        space: &str,
        member_handle: &str,
        text: &str,
    ) -> R<TimelineEntry> {
        let who = self
            .personas()
            .into_iter()
            .find(|p| p.handle == member_handle && !p.is_me)
            .map(|p| p.id)
            .ok_or_else(|| not_found("membro"))?;
        self.post_as(space, &who, text, None)
    }

    fn member_by_handle(&self, item: &str, handle: &str) -> R<String> {
        let space = self.item_state(item)?.space.clone();
        self.personas()
            .into_iter()
            .find(|p| p.handle == handle && !p.is_me && self.is_member(&space, &p.id))
            .map(|p| p.id)
            .ok_or_else(|| not_found(&t("membro deste Espaço", "member of this Space")))
    }

    fn app_call_as(
        &mut self,
        item: &str,
        actor_id: &str,
        tool_name: &str,
        args_json: &str,
        confirmed: bool,
    ) -> R<AppCallOutcome> {
        let it = self.item_state(item)?.clone();
        let ItemContent::App(doc) = it.current().content.clone() else {
            return Err(CoreError::Invalid {
                reason: t("este Item não é um mini-app", "this Item is not a mini-app"),
            });
        };
        let spec = apps::spec(&doc.app).ok_or_else(|| not_found("mini-app"))?;
        let tool = spec
            .tools
            .iter()
            .find(|t| t.name == tool_name)
            .ok_or_else(|| CoreError::Invalid {
                reason: tr!(
                    "ferramenta desconhecida: {tool_name}",
                    "unknown tool: {tool_name}"
                ),
            })?;
        // MCP Apps: a interface só chama ferramentas com "app" na visibilidade.
        if !tool.visibility.contains(&"app") {
            return Err(CoreError::Forbidden {
                reason: tr!(
                    "{tool_name} não é visível para a interface (visibility)",
                    "{tool_name} is not visible to the UI (visibility)"
                ),
            });
        }
        let me = actor_id.to_string();
        let actor = self.persona(&me).name;
        let now = now_ms();
        let state: Value = serde_json::from_str(&doc.state_json).unwrap_or(Value::Null);
        if tool.read_only {
            let view = apps::view(&doc.app, &state, now);
            return Ok(AppCallOutcome {
                status: AppCallStatus::Done,
                result_json: apps::call_tool_result(
                    &t("Estado atual.", "Current state."),
                    &view,
                    false,
                )
                .to_string(),
                message: String::new(),
                confirm_title: None,
                confirm_detail: None,
                item: Some(self.item(item)?),
            });
        }
        let level = self.app_trust(item);
        let unlimited = Budget {
            limit_cents: i64::MAX / 4,
            spent_cents: 0,
        };
        let decision = evaluate(level, &tool.action, 0, &unlimited, &self.policy);
        let denied = |msg: String| AppCallOutcome {
            status: AppCallStatus::Denied,
            result_json: apps::call_tool_result(&msg, &Value::Null, true).to_string(),
            message: msg,
            confirm_title: None,
            confirm_detail: None,
            item: None,
        };
        match decision {
            Decision::Block(reason) => return Ok(denied(reason_label(&reason, &self.policy))),
            Decision::Request(reason) if !confirmed => {
                return Ok(AppCallOutcome {
                    status: AppCallStatus::NeedsConfirmation,
                    result_json: apps::call_tool_result(
                        &t(
                            "Esperando sua confirmação.",
                            "Waiting for your confirmation.",
                        ),
                        &Value::Null,
                        true,
                    )
                    .to_string(),
                    message: reason_label(&reason, &self.policy),
                    confirm_title: Some(tool.title.to_string()),
                    confirm_detail: Some(format!(
                        "{} {}",
                        tool.description,
                        reason_label(&reason, &self.policy)
                    )),
                    item: None,
                })
            }
            _ => {}
        }
        let args: Value = serde_json::from_str(args_json).unwrap_or(json!({}));
        let applied = match apps::apply(&doc.app, tool_name, &args, &state, &actor, now) {
            Ok(a) => a,
            Err(msg) => {
                let view = apps::view(&doc.app, &state, now);
                return Ok(AppCallOutcome {
                    status: AppCallStatus::Done,
                    result_json: apps::call_tool_result(&msg, &view, true).to_string(),
                    message: msg,
                    confirm_title: None,
                    confirm_detail: None,
                    item: Some(self.item(item)?),
                });
            }
        };
        let new_doc = AppDoc {
            state_json: applied.state.to_string(),
            title: applied.title.clone().unwrap_or_else(|| doc.title.clone()),
            ..doc.clone()
        };
        self.append(
            &it.space,
            &me,
            EventBody::ItemVersioned {
                item: item.into(),
                content: ItemContent::App(new_doc),
                note: applied.note.clone(),
            },
        )?;
        // O agente que criou o mini-app comenta eventos marcantes (recorde, nome novo…).
        if let Some(comment) = &applied.comment {
            let agent = it.versions[0].author.clone();
            if self.is_member(&it.space, &agent) {
                self.post_as(&it.space, &agent, comment, None)?;
            }
        }
        if let Some(apps::Effect::AddPlanLine { text, cost_cents }) = applied.effect {
            let plan = self
                .state
                .items
                .values()
                .filter(|i| {
                    i.space == it.space && matches!(i.current().content, ItemContent::Plan(_))
                })
                .max_by_key(|i| i.current().at_ms)
                .map(|i| i.id.clone());
            if let Some(plan) = plan {
                self.add_plan_line(&plan, 0, &text, cost_cents)?;
            }
        }
        let view = apps::view(&doc.app, &applied.state, now);
        Ok(AppCallOutcome {
            status: AppCallStatus::Done,
            result_json: apps::call_tool_result(&applied.text, &view, false).to_string(),
            message: applied.text,
            confirm_title: None,
            confirm_detail: None,
            item: Some(self.item(item)?),
        })
    }

    pub fn wipe(&mut self) -> R<()> {
        self.store.wipe()?;
        self.store.wipe_sync()?;
        self.store.wipe_profiles()?;
        // `wipe` empties `meta` too: keep the format marker, or the next open takes this
        // device's fresh v2 logs for old demo data and erases them.
        self.store.set_meta(
            crate::sync::EVENT_FORMAT_META,
            crate::sync::EVENT_FORMAT_VALUE,
        )?;
        self.reload()
    }

    /// Who signs for `author` here: this device's key for the account, or a local
    /// persona's own key (demo personas and on-device agents).
    pub(crate) fn author_for(&self, author: &str) -> R<Author> {
        if let Some(a) = self.net.account_author(author) {
            return Ok(a);
        }
        self.signers
            .get(author)
            .map(|s| Author::root(s.clone()))
            .ok_or_else(|| CoreError::Forbidden {
                reason: t(
                    "esta Identidade não assina neste aparelho",
                    "this Identity doesn’t sign on this device",
                ),
            })
    }
}

trait LowerFirst {
    fn to_lowercase_first(&self) -> String;
}
impl LowerFirst for String {
    fn to_lowercase_first(&self) -> String {
        let mut c = self.chars();
        match c.next() {
            Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    }
}

pub(crate) fn kind_id(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Plan => "plan",
        ItemKind::Task => "task",
        ItemKind::Note => "note",
        ItemKind::App => "app",
        ItemKind::Page => "page",
        ItemKind::File => "file",
    }
}

pub(crate) fn kind_label(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Plan => ts("Plano", "Plan"),
        ItemKind::Task => ts("Tarefa", "Task"),
        ItemKind::Note => ts("Nota", "Note"),
        ItemKind::App => ts("Mini-app", "Mini-app"),
        ItemKind::Page => ts("Página", "Page"),
        ItemKind::File => ts("Arquivo", "File"),
    }
}

fn action_label(a: &ActionClass) -> &'static str {
    match a {
        ActionClass::Reply => ts("Responder", "Reply"),
        ActionClass::Reversible => ts("Editar no Espaço", "Edit in the Space"),
        ActionClass::External => ts("Enviar para fora", "Send outside"),
        ActionClass::Irreversible => ts("Apagar de verdade", "Delete for real"),
        ActionClass::Money { .. } => ts("Pagamento", "Payment"),
        ActionClass::PublicAudience => ts("Publicar", "Publish"),
        ActionClass::ThirdPartyData => ts("Dados de terceiros", "Third-party data"),
    }
}

/// Plain words for a standing decision's kind of action (`roda_grants::standing_key`).
pub(crate) fn standing_label(key: &str) -> String {
    match key {
        "reply" => t("responder", "replies"),
        "reversible" => t("mudanças neste Espaço", "changes in this Space"),
        "external" => t("enviar para fora do Zoen", "sending outside Zoen"),
        "irreversible" => t("apagar de vez", "deleting for good"),
        "money" => t("pagamentos", "payments"),
        "public_audience" => t("publicar para mais gente", "posting to a wider audience"),
        "third_party_data" => t("dados de outras pessoas", "other people's data"),
        other => other.to_string(),
    }
}

pub(crate) fn reason_label(r: &Reason, p: &Policy) -> String {
    match r {
        Reason::TrustTooLow { needed } => tr!(
            "Precisa do nível {}",
            "Needs the {} level",
            trust_label(*needed)
        ),
        Reason::LeavesTheSpace => t(
            "Sai do Espaço ou não tem desfazer: pede antes",
            "Leaves the Space or can’t be undone: asks first",
        ),
        Reason::MoneyAboveCeiling { .. } => tr!(
            "Linha vermelha: dinheiro acima de {} sempre pede",
            "Red line: money above {} always asks",
            money(p.money_ceiling_cents)
        ),
        Reason::NewPublicAudience => t(
            "Linha vermelha: audiência pública nova sempre pede",
            "Red line: a new public audience always asks",
        ),
        Reason::ThirdPartyData => t(
            "Linha vermelha: dados de terceiros sempre pedem",
            "Red line: third-party data always asks",
        ),
        Reason::OverBudget { remaining_cents } => tr!(
            "Orçamento do mês no fim (resta {})",
            "Monthly budget almost gone ({} left)",
            money(*remaining_cents)
        ),
        Reason::StandingDeny => t(
            "Você escolheu sempre negar isso deste agente aqui",
            "You chose to always deny this from this agent here",
        ),
    }
}

pub(crate) fn trust_label(l: TrustLevel) -> &'static str {
    match l {
        TrustLevel::Listen => ts("Ouvir", "Listen"),
        TrustLevel::Suggest => ts("Sugerir", "Suggest"),
        TrustLevel::Act => ts("Agir", "Act"),
        TrustLevel::Autonomous => ts("Autônomo", "Autonomous"),
    }
}

pub(crate) fn trust_dto(l: TrustLevel) -> TrustLevelDto {
    match l {
        TrustLevel::Listen => TrustLevelDto::Listen,
        TrustLevel::Suggest => TrustLevelDto::Suggest,
        TrustLevel::Act => TrustLevelDto::Act,
        TrustLevel::Autonomous => TrustLevelDto::Autonomous,
    }
}

pub(crate) fn trust_from_dto(l: TrustLevelDto) -> TrustLevel {
    match l {
        TrustLevelDto::Listen => TrustLevel::Listen,
        TrustLevelDto::Suggest => TrustLevel::Suggest,
        TrustLevelDto::Act => TrustLevel::Act,
        TrustLevelDto::Autonomous => TrustLevel::Autonomous,
    }
}

fn event_label(b: &EventBody) -> String {
    match b {
        EventBody::SpaceCreated { title, .. } => {
            tr!("Espaço criado: {title}", "Space created: {title}")
        }
        EventBody::MemberAdded { .. } => t("Membro adicionado", "Member added"),
        EventBody::MemberRemoved { .. } => t("Membro removido", "Member removed"),
        EventBody::MessagePosted {
            attaches: Some(_), ..
        } => t("Mensagem com Item", "Message with Item"),
        EventBody::MessagePosted { .. } => t("Mensagem", "Message"),
        EventBody::ItemCreated { content, .. } => {
            tr!("Item criado: {}", "Item created: {}", content.title())
        }
        EventBody::ItemVersioned { note, .. } => tr!("Nova versão: {note}", "New version: {note}"),
        EventBody::ItemReverted { to_version, .. } => {
            tr!("Restaurou a v{to_version}", "Restored v{to_version}")
        }
        EventBody::GrantIssued { grant } => match &grant.capability {
            Capability::Trust(l) => tr!(
                "Concessão: confiança {}",
                "Grant: trust {}",
                trust_label(*l)
            ),
            Capability::MonthlyBudget { cents } => tr!(
                "Concessão: orçamento {}/mês",
                "Grant: budget {}/month",
                money(*cents)
            ),
            Capability::Invite { .. } => t("Concessão: link de convite", "Grant: invite link"),
            Capability::Device { capability, .. } => tr!(
                "Concessão: {} para um mini-app",
                "Grant: {} for a mini-app",
                capability
            ),
            Capability::ConsentMode { mode } => tr!(
                "Permissões: {}",
                "Permissions: {}",
                consent_mode_label(*mode)
            ),
            Capability::AutoApproved { capability, .. } => tr!(
                "Aprovado automaticamente: {}",
                "Auto-approved: {}",
                capability
            ),
            Capability::Standing { action, allow } => {
                if *allow {
                    tr!(
                        "Sempre aprovar: {}",
                        "Always approve: {}",
                        standing_label(action)
                    )
                } else {
                    tr!(
                        "Sempre negar: {}",
                        "Always deny: {}",
                        standing_label(action)
                    )
                }
            }
        },
        EventBody::GrantRevoked { .. } => t("Concessão revogada", "Grant revoked"),
        EventBody::RequestOpened { request } => tr!("Pedido: {}", "Request: {}", request.title),
        EventBody::RequestResolved { approved, .. } => {
            if *approved {
                t("Pedido aprovado", "Request approved")
            } else {
                t("Pedido recusado", "Request declined")
            }
        }
        EventBody::UsageRecorded { cents, .. } => {
            tr!("Uso de IA: {}", "AI usage: {}", money(*cents))
        }
        EventBody::BackgroundSet { background } => match &background.media {
            Some(m) => tr!(
                "Fundo: foto {}…",
                "Background: photo {}…",
                &m.sha256[..m.sha256.len().min(12)]
            ),
            None => tr!("Fundo: {}", "Background: {}", background.style),
        },
        EventBody::ProfileKeyShared { shares, .. } => tr!(
            "Chave de perfil compartilhada com {}",
            "Profile key shared with {}",
            shares.len()
        ),
        EventBody::Sealed { kind } => tr!("Cifrado: {kind}", "Encrypted: {kind}"),
        EventBody::SpaceEncrypted => t(
            "Criptografia de ponta a ponta ativada",
            "End-to-end encryption turned on",
        ),
        EventBody::Checkpoint { epoch, .. } => {
            tr!(
                "Ponto de verificação: época {epoch}",
                "Checkpoint: epoch {epoch}"
            )
        }
        EventBody::Unsupported { kind } => tr!(
            "Evento de uma versão mais nova: {kind}",
            "Event from a newer version: {kind}"
        ),
    }
}

fn consent_mode_label(m: roda_types::ConsentMode) -> String {
    match m {
        roda_types::ConsentMode::Ask => t("Perguntar", "Ask"),
        roda_types::ConsentMode::Auto => t("Automático", "Auto"),
        roda_types::ConsentMode::Trusted => t("Confiável", "Trusted"),
    }
}

pub(crate) fn plan_from_dto(p: PlanDto) -> PlanDoc {
    PlanDoc {
        title: p.title.trim().to_string(),
        summary: p.summary.trim().to_string(),
        budget_cents: p.budget_cents.filter(|b| *b > 0),
        sections: p
            .sections
            .into_iter()
            .map(|s| PlanSection {
                title: s.title,
                lines: s
                    .lines
                    .into_iter()
                    .filter(|l| !l.text.trim().is_empty())
                    .map(|l| PlanLine {
                        id: if l.id.is_empty() { new_id("ln") } else { l.id },
                        text: l.text.trim().to_string(),
                        cost_cents: l.cost_cents.max(0),
                        done: l.done,
                    })
                    .collect(),
            })
            .filter(|s| !s.lines.is_empty())
            .collect(),
    }
}

pub(crate) fn plan_to_dto(p: &PlanDoc) -> PlanDto {
    PlanDto {
        title: p.title.clone(),
        summary: p.summary.clone(),
        budget_cents: p.budget_cents,
        total_cents: p.total_cents(),
        sections: p
            .sections
            .iter()
            .map(|s| PlanSectionDto {
                title: s.title.clone(),
                lines: s
                    .lines
                    .iter()
                    .map(|l| PlanLineDto {
                        id: l.id.clone(),
                        text: l.text.clone(),
                        cost_cents: l.cost_cents,
                        done: l.done,
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
