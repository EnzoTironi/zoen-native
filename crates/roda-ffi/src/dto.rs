//! Tipos expostos ao Swift (UniFFI). São "visões" da projeção, nunca a fonte da verdade.

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PersonaKind {
    Person,
    Agent,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Persona {
    pub id: String,
    pub kind: PersonaKind,
    pub name: String,
    pub handle: String,
    pub initials: String,
    pub tint_hex: String,
    pub glyph: Option<String>,
    pub bio: String,
    pub owner_id: Option<String>,
    pub owner_name: Option<String>,
    pub owner_tint_hex: Option<String>,
    pub is_me: bool,
    /// O agente é meu (eu pago, eu configuro).
    pub is_mine: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SpaceKindDto {
    Direct,
    Group,
    Community,
}

/// An end-to-end chat's MLS group as this device has it. Members compare `digest` for
/// the same `epoch` to know they share one group (ADR 0026).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GroupKeysDto {
    pub epoch: u64,
    pub digest: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PrivacyDto {
    EndToEnd,
    Closed,
    Public,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, uniffi::Enum)]
pub enum TrustLevelDto {
    Listen,
    Suggest,
    Act,
    Autonomous,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SpaceSummary {
    pub id: String,
    pub title: String,
    pub kind: SpaceKindDto,
    pub privacy: PrivacyDto,
    pub members: Vec<Persona>,
    /// Para DMs: a outra ponta (pessoa ou agente).
    pub counterpart: Option<Persona>,
    pub last_preview: String,
    pub last_author: Option<Persona>,
    pub last_at_ms: i64,
    pub unread: u32,
    pub pending_requests: u32,
    pub event_count: u64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ItemCard {
    pub item_id: String,
    pub title: String,
    pub summary: String,
    pub kind_label: String,
    /// Stable kind id for code paths ("plan", "task", "note", "app"); `kind_label` is for people.
    pub kind_id: String,
    pub version: u32,
    pub total_cents: Option<i64>,
    pub budget_cents: Option<i64>,
    pub line_count: u32,
    pub done_count: u32,
    /// Mini-app (MCP App), quando o Item é um.
    pub app: Option<AppStateDto>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
#[expect(
    clippy::large_enum_variant,
    reason = "uniffi enums cross the FFI by value; a Box isn't representable"
)]
pub enum EntryKind {
    Message {
        text: String,
        card: Option<ItemCard>,
    },
    ItemEdited {
        item_id: String,
        title: String,
        version: u32,
        note: String,
        is_undo: bool,
    },
    Request {
        request_id: String,
        title: String,
        approved: Option<bool>,
    },
    System {
        text: String,
    },
    /// The chat background changed (a typed log event, not a text marker).
    Background {
        background: BackgroundDto,
    },
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TimelineEntry {
    pub id: String,
    pub seq: u64,
    pub author: Persona,
    pub at_ms: i64,
    pub kind: EntryKind,
    /// Where this entry is on its way to the others.
    pub delivery: Delivery,
    /// Inline reply: a quote of the message this one answers.
    #[uniffi(default = None)]
    pub reply_to: Option<ReplyQuote>,
    /// Thread reply: the root message's id. The main timeline leaves these out and shows
    /// `thread_replies` under the root instead.
    #[uniffi(default = None)]
    pub in_thread: Option<String>,
    /// On a thread root: how many replies its thread has.
    #[uniffi(default = 0)]
    pub thread_replies: u32,
}

/// What an inline reply shows of the message it answers.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ReplyQuote {
    pub id: String,
    pub author: Persona,
    pub text: String,
}

/// `Local`: a Space that lives only on this device. `Sending`: signed and queued for the
/// relay (offline-safe). `Sent`: the relay ordered it. `Failed`: the relay refused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Delivery {
    Local,
    Sending,
    Sent,
    Failed,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlanLineDto {
    pub id: String,
    pub text: String,
    pub cost_cents: i64,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlanSectionDto {
    pub title: String,
    pub lines: Vec<PlanLineDto>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlanDto {
    pub title: String,
    pub summary: String,
    pub budget_cents: Option<i64>,
    pub sections: Vec<PlanSectionDto>,
    /// Calculado pelo núcleo (ignorado na entrada).
    pub total_cents: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct VersionDto {
    pub number: u32,
    pub author: Persona,
    pub at_ms: i64,
    pub note: String,
    pub is_undo: bool,
    pub total_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ItemDetail {
    pub id: String,
    pub space_id: String,
    pub space_title: String,
    pub kind_label: String,
    /// Stable kind id for code paths ("plan", "task", "note", "app"); `kind_label` is for people.
    pub kind_id: String,
    pub title: String,
    pub origin: String,
    pub created_by: Persona,
    pub plan: Option<PlanDto>,
    pub text: Option<String>,
    pub version: u32,
    pub versions: Vec<VersionDto>,
    /// Pedidos de agentes ligados a linhas deste Item (linha → status).
    pub linked_requests: Vec<AgentRequestDto>,
    pub app: Option<AppStateDto>,
}

// ───────────────────────────── MCP Apps (mini-apps) ─────────────────────────────

/// Uma ferramenta de um mini-app, no formato do MCP (`Tool` + `_meta.ui`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AppToolDto {
    pub name: String,
    pub title: String,
    pub description: String,
    pub input_schema_json: String,
    /// `model` (o agente pode chamar) e/ou `app` (a interface pode chamar).
    pub visibility: Vec<String>,
    /// Classe da ação para o avaliador de Concessões (Reversível, Externa…).
    pub action_label: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AppSpecDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub resource_uri: String,
    /// Tem View MCP (HTML em `resources/read`). Sem View = só interface nativa.
    pub has_view: bool,
    pub tools: Vec<AppToolDto>,
}

/// Conteúdo de `resources/read` para um recurso `ui://`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AppResourceDto {
    pub uri: String,
    pub mime_type: String,
    pub text: String,
    pub prefers_border: bool,
    /// Mini-app manifest (JSON: capabilities with purposes, allowed domains, sha256 of
    /// `text`). Empty for the older hand-written apps. The host refuses a bundle whose hash
    /// doesn't match.
    pub manifest_json: String,
}

/// A native capability a mini-app holds (Settings → Mini-app permissions).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DeviceGrantDto {
    pub grant_id: String,
    pub item_id: String,
    pub item_title: String,
    pub app_name: String,
    pub space_title: String,
    pub capability: String,
    pub purpose: String,
    /// `false` = "Allow once" (expires in a minute).
    pub always: bool,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AppMetricDto {
    pub label: String,
    /// 0…1
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AppStateDto {
    pub app_id: String,
    pub resource_uri: String,
    pub name: String,
    /// Estado como a interface deve ver agora (vai em `structuredContent`).
    pub view_json: String,
    pub headline: String,
    pub metrics: Vec<AppMetricDto>,
    pub last_action: Option<String>,
    /// Nível da Concessão do mini-app neste Item.
    pub trust: TrustLevelDto,
    /// Widget snapshot JSON (native template for the Home strip and WidgetKit); empty = none.
    pub snapshot_json: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AppCallStatus {
    /// Rodou (ou era só leitura).
    Done,
    /// Irreversível ou externo: o app nativo confirma com você antes.
    NeedsConfirmation,
    /// A Concessão não cobre, nem pedindo.
    Denied,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AppCallOutcome {
    pub status: AppCallStatus,
    /// `CallToolResult` do MCP em JSON (content + structuredContent + isError).
    pub result_json: String,
    pub message: String,
    pub confirm_title: Option<String>,
    pub confirm_detail: Option<String>,
    pub item: Option<ItemDetail>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct UndoToken {
    pub item_id: String,
    /// Versão para a qual "Desfazer" volta.
    pub restore_version: u32,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EditOutcome {
    pub item: ItemDetail,
    pub undo: UndoToken,
    /// Reação do agente (regra determinística do núcleo, não IA).
    pub reaction: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RequestStatus {
    Pending,
    Approved,
    Denied,
    /// O conteúdo mudou depois do pedido: a aprovação não vale mais.
    Stale,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AgentRequestDto {
    pub id: String,
    pub agent: Persona,
    pub space_id: String,
    pub space_title: String,
    pub title: String,
    pub detail: String,
    pub audience: String,
    pub action_label: String,
    pub cost_cents: Option<i64>,
    pub reason: String,
    pub status: RequestStatus,
    pub opened_ms: i64,
    pub resolved_ms: Option<i64>,
    pub item_id: Option<String>,
    pub line_id: Option<String>,
    /// What a standing decision on this request would cover (`roda_grants::standing_key`).
    pub action_key: String,
    /// False for red lines and irreversible actions: those always ask, so the stack
    /// doesn't offer "always approve".
    pub can_always_approve: bool,
    /// Resolved by a standing decision ("always approve/deny") rather than one by one.
    pub by_standing: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AgentSpaceTrust {
    pub space_id: String,
    pub space_title: String,
    pub level: TrustLevelDto,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AgentProfile {
    pub persona: Persona,
    /// `None` quando o orçamento é de outra pessoa (quem é dono paga).
    pub budget_limit_cents: Option<i64>,
    pub budget_spent_cents: Option<i64>,
    pub near_limit: bool,
    pub spaces: Vec<AgentSpaceTrust>,
    pub pending_requests: u32,
    pub approvals_streak: u32,
    pub runs_on: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DecisionKind {
    Act,
    ActWithUndo,
    Request,
    Block,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DecisionPreview {
    pub action: String,
    pub example: String,
    pub kind: DecisionKind,
    pub explanation: String,
    pub red_line: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlanOutcome {
    pub kind: DecisionKind,
    pub item: Option<ItemDetail>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ApproveOutcome {
    pub request: AgentRequestDto,
    pub message: String,
}

/// The four swipes on the approvals stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RequestDecision {
    /// Right: approve this one.
    Approve,
    /// Left: deny this one.
    Deny,
    /// Up: approve and keep approving this kind of action from this agent here.
    AlwaysApprove,
    /// Down: deny and keep denying this kind of action from this agent here.
    AlwaysDeny,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DecideOutcome {
    pub request: AgentRequestDto,
    pub message: String,
    /// The standing Grant a "Sempre" swipe issued (revocable from Permissions).
    pub standing_grant_id: Option<String>,
    /// Other pending requests the standing decision resolved right away.
    pub also_resolved: u32,
}

/// A standing "always approve / always deny" an owner gave one of their agents.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct StandingDecisionDto {
    pub grant_id: String,
    pub agent: Persona,
    pub space_id: String,
    pub space_title: String,
    pub action_key: String,
    pub action_label: String,
    pub allow: bool,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LogReport {
    pub space_id: String,
    pub space_title: String,
    pub events: u64,
    pub head_hash: String,
    pub valid: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LogEventDto {
    pub seq: u64,
    pub label: String,
    pub author: Persona,
    pub at_ms: i64,
    pub hash: String,
    pub prev: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchHit {
    pub space_id: String,
    pub space_title: String,
    pub item_id: Option<String>,
    pub title: String,
    pub snippet: String,
    pub author: Persona,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Mention {
    pub space_id: String,
    pub space_title: String,
    pub entry: TimelineEntry,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreStats {
    pub spaces: u32,
    pub events: u64,
    pub items: u32,
    pub identities: u32,
    pub all_logs_valid: bool,
    pub db_path: String,
}

/// Uma ação de um agente, lida do log assinado (tela "Atividade do agente").
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AgentActivityDto {
    pub label: String,
    pub detail: String,
    pub space_id: String,
    pub space_title: String,
    pub at_ms: i64,
    /// Custo de IA registrado logo depois desta ação (UsageRecorded), se houver.
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MediaRefDto {
    pub sha256: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

/// A chat background (see `roda_types::BackgroundSpec`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BackgroundDto {
    pub style: String,
    pub media: Option<MediaRefDto>,
    pub zoom_pm: u32,
    pub offset_x_pm: i32,
    pub offset_y_pm: i32,
    pub dim_pm: Option<u32>,
    pub blur_pm: u32,
    pub appearance: String,
}

impl From<&roda_types::BackgroundSpec> for BackgroundDto {
    fn from(b: &roda_types::BackgroundSpec) -> Self {
        BackgroundDto {
            style: b.style.clone(),
            media: b.media.as_ref().map(|m| MediaRefDto {
                sha256: m.sha256.clone(),
                mime: m.mime.clone(),
                width: m.width,
                height: m.height,
                bytes: m.bytes,
            }),
            zoom_pm: b.zoom_pm,
            offset_x_pm: b.offset_x_pm,
            offset_y_pm: b.offset_y_pm,
            dim_pm: b.dim_pm,
            blur_pm: b.blur_pm,
            appearance: b.appearance.clone(),
        }
    }
}

impl From<&BackgroundDto> for roda_types::BackgroundSpec {
    fn from(b: &BackgroundDto) -> Self {
        roda_types::BackgroundSpec {
            style: b.style.clone(),
            media: b.media.as_ref().map(|m| roda_types::MediaRef {
                sha256: m.sha256.clone(),
                mime: m.mime.clone(),
                width: m.width,
                height: m.height,
                bytes: m.bytes,
                key: None,
                blob: None,
            }),
            zoom_pm: b.zoom_pm.clamp(1000, 5000),
            offset_x_pm: b.offset_x_pm.clamp(-1000, 1000),
            offset_y_pm: b.offset_y_pm.clamp(-1000, 1000),
            dim_pm: b.dim_pm.map(|d| d.min(900)),
            blur_pm: b.blur_pm.min(1000),
            appearance: b.appearance.clone(),
        }
    }
}

/// One universal-search hit. `kind`: "message", "person", "agent", "space", "item" or
/// "app". Snippets mark the matched text with `[[` and `]]`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct UniversalHit {
    pub kind: String,
    pub ref_id: String,
    pub space_id: Option<String>,
    pub space_title: Option<String>,
    pub title: String,
    pub title_snippet: String,
    pub snippet: String,
    pub at_ms: i64,
    pub persona: Option<Persona>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct UniversalResults {
    pub hits: Vec<UniversalHit>,
    /// Query time in microseconds (index refresh included).
    pub took_us: u64,
}
