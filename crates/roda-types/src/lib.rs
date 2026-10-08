//! # roda-types
//!
//! Os **5 primitivos** do Roda e o substrato de eventos.
//!
//! | Primitivo   | Aqui                                   |
//! |-------------|----------------------------------------|
//! | Identidade  | [`Identity`] (pessoa, agente ou Espaço) |
//! | Espaço      | [`EventBody::SpaceCreated`] + projeção  |
//! | Membro      | [`EventBody::MemberAdded`] / [`Role`]   |
//! | Item        | [`ItemContent`] versionado por eventos  |
//! | Concessão   | [`Grant`] / [`Capability`]              |
//!
//! Tudo que acontece num Espaço vira um [`Event`] assinado no log daquele Espaço
//! (ver `roda-log`). Nenhuma tela mostra o log; ele é a fonte da verdade.

use serde::{Deserialize, Serialize};

/// Chave pública Ed25519 em hex (32 bytes → 64 caracteres).
pub type IdentityId = String;
pub type SpaceId = String;
pub type ItemId = String;
pub type GrantId = String;
pub type RequestId = String;

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A readable prefix plus a lowercase ULID: `sp_01jq3…` (ADR 0017). Ids made later sort
/// later, across devices to the millisecond and on one device strictly.
pub fn new_id(prefix: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    format!("{prefix}_{}", new_ulid(now).to_ascii_lowercase())
}

/// 128 random bits in hex, for ids that double as a capability (invite links): never a
/// ULID, whose leading characters are a guessable timestamp.
pub fn new_secret_id(prefix: &str) -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("fonte de entropia do sistema");
    format!("{prefix}_{}", hex::encode(bytes))
}

/// A ULID (26 chars, Crockford base32): 48 bits of milliseconds + 80 random bits.
/// Monotonic within this process: a second id in the same millisecond (or after the clock
/// steps back) increments the previous one instead of drawing fresh bits, so ids made on one
/// device always sort in creation order. Used as every event's idempotency key.
pub fn new_ulid(now_ms: i64) -> String {
    static LAST: std::sync::Mutex<u128> = std::sync::Mutex::new(0);
    let mut rnd = [0u8; 10];
    getrandom::getrandom(&mut rnd).expect("fonte de entropia do sistema");
    let mut v: u128 = (now_ms.max(0) as u128 & 0xFFFF_FFFF_FFFF) << 80;
    for (i, b) in rnd.iter().enumerate() {
        v |= (*b as u128) << (8 * (9 - i));
    }
    let mut last = LAST.lock().unwrap_or_else(|p| p.into_inner());
    if v >> 80 <= *last >> 80 {
        v = last.wrapping_add(1);
    }
    *last = v;
    drop(last);
    let mut out = [0u8; 26];
    for i in (0..26).rev() {
        out[i] = CROCKFORD[(v & 31) as usize];
        v >>= 5;
    }
    String::from_utf8(out.to_vec()).expect("ascii")
}

/// When a ULID-based id was made (ms since the epoch), or `None` for ids from before
/// ADR 0017 (random hex) and anything else.
pub fn id_time_ms(id: &str) -> Option<i64> {
    let ulid = id.rsplit('_').next()?;
    if ulid.len() != 26 {
        return None;
    }
    let mut v: u128 = 0;
    for c in ulid.bytes() {
        let d = CROCKFORD
            .iter()
            .position(|&a| a == c.to_ascii_uppercase())?;
        v = (v << 5) | d as u128;
    }
    Some((v >> 80) as i64)
}

// ───────────────────────────── Identidade ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdentityKind {
    Person,
    Agent,
}

/// Uma Identidade: um par de chaves com nome e rosto.
/// Pessoa e agente são o mesmo objeto; o agente tem, a mais, um dono.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub id: IdentityId,
    pub kind: IdentityKind,
    pub name: String,
    pub handle: String,
    /// Cor da Identidade (agentes: gradiente do squircle; pessoas: fundo do avatar).
    pub tint_hex: String,
    /// Glifo monocromático (nome de SF Symbol no cliente Apple). Só agentes.
    pub glyph: Option<String>,
    /// Dono do agente. "Quem é dono do agente paga o agente."
    pub owner: Option<IdentityId>,
    /// Uma linha sobre o agente/pessoa.
    pub bio: String,
}

// ───────────────────────────── Espaço & Membro ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Privacy {
    /// 🔒 Ponta a ponta (MLS). Agentes são leitores declarados.
    EndToEnd,
    /// 🛡️ Fechada: legível pelo servidor para moderar, buscar e rodar agentes.
    Closed,
    /// 🌐 Pública.
    Public,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpaceKind {
    /// Conversa a dois (pessoa ↔ pessoa ou pessoa ↔ agente).
    Direct,
    Group,
    Community,
    /// Seu contexto pessoal: só você. Guarda orçamentos dos seus agentes e Memória.
    /// Não aparece em Conversas.
    Personal,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    Owner,
    Admin,
    Member,
    /// Lê, mas não escreve (ex.: agente em modo Ouvir numa comunidade).
    Reader,
}

// ───────────────────────────── Item ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Plan,
    Task,
    Note,
    /// Mini-app (MCP App): uma interface interativa cujo estado é este Item.
    App,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanLine {
    pub id: String,
    pub text: String,
    /// Centavos de real. Inteiros, nunca float.
    pub cost_cents: i64,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanSection {
    pub title: String,
    pub lines: Vec<PlanLine>,
}

/// Um plano/documento estruturado. No v1 completo isto vira um documento Loro (CRDT);
/// aqui a versão inteira vai em cada evento, o que mantém Versões/Desfazer triviais.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanDoc {
    pub title: String,
    pub summary: String,
    pub budget_cents: Option<i64>,
    pub sections: Vec<PlanSection>,
}

impl PlanDoc {
    pub fn total_cents(&self) -> i64 {
        self.sections
            .iter()
            .flat_map(|s| &s.lines)
            .map(|l| l.cost_cents)
            .sum()
    }

    pub fn line(&self, id: &str) -> Option<&PlanLine> {
        self.sections
            .iter()
            .flat_map(|s| &s.lines)
            .find(|l| l.id == id)
    }

    pub fn line_mut(&mut self, id: &str) -> Option<&mut PlanLine> {
        self.sections
            .iter_mut()
            .flat_map(|s| s.lines.iter_mut())
            .find(|l| l.id == id)
    }

    pub fn remove_line(&mut self, id: &str) -> Option<PlanLine> {
        for s in &mut self.sections {
            if let Some(pos) = s.lines.iter().position(|l| l.id == id) {
                return Some(s.lines.remove(pos));
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ItemContent {
    Text { text: String },
    Plan(PlanDoc),
    App(AppDoc),
}

impl ItemContent {
    pub fn title(&self) -> String {
        match self {
            ItemContent::Text { text } => text.lines().next().unwrap_or_default().to_string(),
            ItemContent::Plan(p) => p.title.clone(),
            ItemContent::App(a) => a.title.clone(),
        }
    }
}

/// Estado de um mini-app (MCP App) dentro de um Espaço. A interface vem de um
/// recurso `ui://`; o estado compartilhado mora aqui, versionado e assinado como
/// qualquer Item: todo mundo vê o mesmo bichinho e quem fez o quê.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AppDoc {
    /// Qual mini-app (ex.: "pet", "poll", "list").
    pub app: String,
    /// Recurso de interface (`ui://…`), como no MCP Apps.
    pub resource_uri: String,
    pub title: String,
    /// Estado em JSON canônico (o servidor do mini-app define o formato).
    pub state_json: String,
}

// ───────────────────────────── Concessão ─────────────────────────────

/// Nível de confiança de um agente num Espaço. Um controle só.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrustLevel {
    /// Lê só quando chamado e responde com texto.
    Listen,
    /// Prepara rascunhos e propostas; nada sai sem você.
    Suggest,
    /// Faz o que é reversível dentro do Espaço e do orçamento, sempre com desfazer.
    Act,
    /// Também age fora (agenda, e-mail, pagamentos pequenos) dentro dos limites.
    Autonomous,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum GrantScope {
    Space(SpaceId),
    Item(ItemId),
    Everywhere,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Capability {
    Trust(TrustLevel),
    /// Orçamento mensal de IA, em centavos de real.
    MonthlyBudget {
        cents: i64,
    },
    /// Convite/link de compartilhamento: quem tiver o link entra com este papel.
    Invite {
        role: Role,
    },
    /// A native device capability for a mini-app (`photos.pick`, `location`,
    /// `calendar.freebusy`, `net:tiles.openfreemap.org`…), with the purpose it showed you.
    /// "Allow once" is the same grant with a short expiry, so it's logged too.
    Device {
        capability: String,
        purpose: String,
    },
    /// How much Zoen asks before a mini-app or agent uses something (Ask / Auto / Trusted).
    /// Issued to a subject: `consent:global`, `space:<id>`, an agent id or `app:<item>`.
    /// The newest unrevoked one for a subject wins.
    ConsentMode {
        mode: ConsentMode,
    },
    /// A request the auto reviewer approved on its own (with the policy rule that let it).
    /// Short-lived like "Allow once"; revoking it is the toast's Undo.
    AutoApproved {
        capability: String,
        purpose: String,
        rule: String,
    },
}

/// Consent mode. `Auto` is the default; `Trusted` is only for agents or mini-apps you mark.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConsentMode {
    /// Always show Zoen's sheet.
    Ask,
    /// The reviewer approves reads and reversible in-Space writes that match what you just did.
    Auto,
    /// Approves everything except the always-ask list.
    Trusted,
}

/// Uma Concessão: "quem pode fazer o quê em qual Espaço/Item, com quais limites".
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub id: GrantId,
    pub grantor: IdentityId,
    /// `None` = portador (links de convite).
    pub grantee: Option<IdentityId>,
    pub scope: GrantScope,
    pub capability: Capability,
    pub expires_at_ms: Option<i64>,
}

/// Classe de uma ação proposta por um agente. É isso que o avaliador olha.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ActionClass {
    /// Responder com texto.
    Reply,
    /// Criar/editar/mover dentro do Espaço. Tudo é versionado → desfazível.
    Reversible,
    /// Mandar algo para fora (e-mail, WhatsApp, agenda).
    External,
    /// Apagar de verdade.
    Irreversible,
    /// Gastar dinheiro (não o orçamento de IA: dinheiro de verdade).
    Money { cents: i64 },
    /// Publicar para uma audiência nova/maior.
    PublicAudience,
    /// Ler ou usar dados de terceiros.
    ThirdPartyData,
}

/// Um pedido de agente (o "cartão de pedido"): o que muda, para quem e quanto custa.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentRequest {
    pub id: RequestId,
    pub agent: IdentityId,
    pub title: String,
    pub detail: String,
    pub audience: String,
    pub action: ActionClass,
    /// Hash do conteúdo sendo aprovado. Mudou o conteúdo, a aprovação não vale mais.
    pub content_hash: String,
    /// Opcional: a linha de plano que este pedido executa.
    pub item: Option<ItemId>,
    pub line: Option<String>,
}

// ───────────────────────────── Evento (substrato) ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum EventBody {
    SpaceCreated {
        title: String,
        kind: SpaceKind,
        privacy: Privacy,
    },
    MemberAdded {
        identity: IdentityId,
        role: Role,
    },
    MemberRemoved {
        identity: IdentityId,
    },
    MessagePosted {
        message: ItemId,
        text: String,
        attaches: Option<ItemId>,
    },
    ItemCreated {
        item: ItemId,
        kind: ItemKind,
        content: ItemContent,
        origin: String,
    },
    ItemVersioned {
        item: ItemId,
        content: ItemContent,
        note: String,
    },
    /// Desfazer/restaurar nunca apaga: cria uma versão nova igual a uma anterior.
    ItemReverted {
        item: ItemId,
        to_version: u32,
        note: String,
    },
    GrantIssued {
        grant: Grant,
    },
    GrantRevoked {
        grant: GrantId,
    },
    RequestOpened {
        request: AgentRequest,
    },
    RequestResolved {
        request: RequestId,
        approved: bool,
        content_hash: String,
    },
    /// Uso de IA de um agente (debitado do orçamento do dono).
    UsageRecorded {
        agent: IdentityId,
        cents: i64,
        what: String,
    },
    /// The chat's background changed (shared with everyone in the chat). Photos travel as a
    /// content-addressed attachment: the event carries only its hash and framing.
    BackgroundSet {
        background: BackgroundSpec,
    },
    /// The author's profile key, sealed separately to each recipient's agreement key
    /// (ADR 0016). `version` is the first profile version the key opens. Only the named
    /// recipient can open a share; from M2 on the whole event rides inside MLS.
    ProfileKeyShared {
        version: u64,
        shares: Vec<ProfileKeyShare>,
    },
    /// A kind this build doesn't know yet (a newer client wrote it). The signed bytes are
    /// kept verbatim, so the event still verifies, syncs and chains; it just isn't shown.
    Unsupported {
        kind: String,
    },
}

/// One recipient's copy of a profile key: X25519 with a fresh ephemeral key, then
/// XChaCha20-Poly1305 (hex fields).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProfileKeyShare {
    pub to: IdentityId,
    pub ephemeral: String,
    pub sealed: String,
}

/// A content-addressed attachment (sha256 of the bytes, lowercase hex).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MediaRef {
    pub sha256: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    /// In relay-synced chats: the key (hex, 32 bytes) that opens the encrypted copy on the
    /// relay. Carried inside the chat's log, which MLS encrypts from M2 on (ADR 0007).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// sha256 of the encrypted copy: its address in the relay's blob store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

/// A chat background. Numbers are integers in thousandths so the event stays exact
/// (no floats in the signed log).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BackgroundSpec {
    /// "none", "color:<key>", "gradient:<key>", "doodles:<key>", "builtin:<key>" or "photo".
    pub style: String,
    /// The photo (style "photo").
    pub media: Option<MediaRef>,
    /// Photo framing: zoom (1000 = fill the screen) and the pan as a fraction of the
    /// overflow on each axis (-1000…1000, 0 = centered).
    pub zoom_pm: u32,
    pub offset_x_pm: i32,
    pub offset_y_pm: i32,
    /// Dim over the photo in thousandths; `None` = automatic (from the photo's luminance).
    pub dim_pm: Option<u32>,
    pub blur_pm: u32,
    /// "auto", "light" or "dark": the chat's appearance over this background.
    pub appearance: String,
}

/// The newest relay-ordered event the author had applied in this Space when signing: a
/// causal link inside the signed bytes. A sequencer can't place the event before it or show
/// the author's words on a different history without breaking the link.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub seq: u64,
    pub hash: String,
}

/// The per-Space chain: whoever sequences (the relay, or this device for local-only
/// Spaces) links each event to the previous one. `wire` is the hash of what actually
/// traveled: the signed content for plaintext events, the sealed envelope for E2EE ones.
#[derive(Serialize, Clone, Debug)]
pub struct ChainLink<'a> {
    pub space: &'a str,
    pub seq: u64,
    pub prev: &'a str,
    pub wire: &'a str,
}

pub const EVENT_FORMAT: u8 = 3;

/// One event. `content` is the exact byte string the author signed (format v3, see
/// `roda_log::content`); the other fields are views decoded from it, plus what the
/// sequencer added around it (`seq`, `prev`, `hash`). Nobody re-encodes `content`, so a
/// field a newer client added survives every hop and the signature never depends on the
/// verifier's schema.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub space: SpaceId,
    /// Sequence per Space, assigned by the relay (or by this device for local-only Spaces).
    pub seq: u64,
    /// Chain hash of the previous event (64 zeros at genesis).
    pub prev: String,
    pub author: IdentityId,
    pub at_ms: i64,
    pub body: EventBody,
    /// Chain hash: SHA-256 of [`ChainLink`].
    pub hash: String,
    /// Ed25519 signature (by `device`, or by `author` when `device` is empty) over the
    /// content hash of `content`.
    pub sig: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// The identity's signature over the device key ("this device is me").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen: Option<Seen>,
    #[serde(with = "hex_bytes")]
    pub content: Vec<u8>,
    /// Hash of the sealed envelope when the event traveled end-to-end encrypted.
    /// `None` = it traveled in the clear and `wire` is the content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed_wire: Option<String>,
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        hex::decode(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

pub const GENESIS_PREV: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[cfg(test)]
mod tests {

    #[test]
    fn ids_sort_by_creation_and_carry_their_time() {
        let ids: Vec<String> = (0..2000).map(|_| new_id("sp")).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "same-millisecond ids keep creation order");
        assert!(ids.iter().all(|i| i.len() == 29 && i.starts_with("sp_")));
        let t = id_time_ms(&ids[0]).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!((now - t).abs() < 5_000);
        assert_eq!(id_time_ms(&format!("sp_{}", "ab".repeat(16))), None);
        let before = new_ulid(now);
        assert!(
            new_ulid(now - 60_000) > before,
            "a clock step back still sorts later"
        );
        assert_eq!(
            id_time_ms(&new_ulid(1_700_000_000_123)).map(|t| t >= 1_700_000_000_123),
            Some(true)
        );
    }

    use super::*;

    #[test]
    fn plan_total_and_line_ops() {
        let mut p = PlanDoc {
            title: "Paraty".into(),
            summary: String::new(),
            budget_cents: Some(150_000),
            sections: vec![PlanSection {
                title: "Hospedagem".into(),
                lines: vec![
                    PlanLine {
                        id: "a".into(),
                        text: "Pousada".into(),
                        cost_cents: 42_000,
                        done: false,
                    },
                    PlanLine {
                        id: "b".into(),
                        text: "Taxa".into(),
                        cost_cents: 1_000,
                        done: false,
                    },
                ],
            }],
        };
        assert_eq!(p.total_cents(), 43_000);
        p.line_mut("b").unwrap().cost_cents = 2_000;
        assert_eq!(p.total_cents(), 44_000);
        assert!(p.remove_line("a").is_some());
        assert_eq!(p.total_cents(), 2_000);
        assert!(p.line("a").is_none());
    }

    #[test]
    fn ulids_sort_by_time_and_are_unique() {
        let a = new_ulid(1_000);
        let b = new_ulid(2_000);
        assert_eq!(a.len(), 26);
        assert!(a < b);
        assert_ne!(new_ulid(5), new_ulid(5));
    }

    #[test]
    fn ids_are_unique_and_prefixed() {
        let a = new_id("sp");
        let b = new_id("sp");
        assert_ne!(a, b);
        assert!(a.starts_with("sp_"));
        assert_eq!(a.len(), 3 + 26);
    }

    #[test]
    fn event_body_roundtrips_through_json() {
        let body = EventBody::RequestOpened {
            request: AgentRequest {
                id: "rq_1".into(),
                agent: "ab".into(),
                title: "Reservar".into(),
                detail: "2 noites".into(),
                audience: "Pousada".into(),
                action: ActionClass::Money { cents: 42_000 },
                content_hash: "h".into(),
                item: None,
                line: None,
            },
        };
        let json = serde_json::to_string(&body).unwrap();
        let back: EventBody = serde_json::from_str(&json).unwrap();
        assert_eq!(body, back);
    }
}
