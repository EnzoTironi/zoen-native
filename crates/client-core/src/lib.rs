//! # client-core
//!
//! Interface do núcleo compartilhado para os clientes nativos e Web.
//! Mantém identidade, permissões, criptografia, documentos e sincronização no
//! núcleo Rust; UniFFI e a ponte JSON expõem comandos e visões para os clientes.

use std::sync::{Arc, Mutex, MutexGuard};

mod agent_binary_documents;
#[path = "agent_pages/contract.rs"]
pub mod agent_page_tools;
mod agent_pages;
pub mod agent_scenes;
pub use agent_pages::approvals::ApprovedPageTurn;
pub use agent_pages::{AgentPageDelivery, AgentPageResult, AgentPageSnapshot, AgentPageTurn};
pub mod agent_app_creation;
mod agent_control;
mod agent_definition;
mod agent_owner_scope;
pub use agent_control::{AgentControlDto, AgentControlOutcome};
pub mod agent_sites;
pub use agent_definition::{
    AgentDefinitionDto, AgentDefinitionPreviewDto, AgentDefinitionUpdateDto,
    AgentDefinitionUpdateOutcome,
};
mod agent_enrollment;
pub use agent_enrollment::{
    AgentEnrollmentDeliveryDto, AgentEnrollmentDeliveryOutcome, AgentEnrollmentHostOffer,
    AgentEnrollmentHostProgress, AgentEnrollmentHostRequest, AgentEnrollmentPreviewDto,
    AgentEnrollmentStatusDto,
};
pub use miniapps::binary::agent_tools as binary_agent_tools;
/// Closed, shared schema validation only; this module grants no execution authority.
pub use miniapps::deckcraft::agent_tools as deckcraft_agent_tools;
mod agent_notes;
pub mod image_tasks;
pub mod owner_policy;
pub use agent_binary_documents::{
    AgentBinaryTurn, AgentToolApprovalInput, ApprovedToolTurn, AGENT_BINARY_DOCUMENT_LIMIT,
};
mod app_binary_documents;
pub use app_binary_documents::AppBinaryDocumentDto;
mod app_current_share;
mod app_documents;
mod app_edit_requests;
mod app_resources;
pub use app_documents::AppDocumentDto;
mod api;
pub use api::call_turn::CallTurnDto;
mod backup;
mod backup_api;
pub mod blob_transport;
mod call_transport_api;
mod client_api;
pub mod relay_output_transport;
pub use backup_api::BackupStatusDto;
#[cfg(feature = "client-types")]
pub use client_api::export_typescript;
mod files;
mod forwarding;
pub use forwarding::{ForwardReview, ForwardScope, ForwardSource};
mod growth;
pub use growth::{AcquisitionDto, GrowthSyncDto, OnboardingPlanDto};
mod file_demand;
mod file_edit_drafts;
pub use file_edit_drafts::{
    FileEditDraftCapture, FileEditDraftCommitResult, FileEditDraftDto, FileEditDraftSaveResult,
    FileEditDraftScope,
};
mod files_api;
mod liveview;
mod sites;
pub use liveview::{LiveViewDemoVm, LiveViewInput, LiveViewKey, LiveViewSession};
mod local_sealed;
mod media;
mod media_drafts;
mod recent_attachments;
pub use media_drafts::{
    MediaDraftAsset, MediaDraftAssetInput, MediaDraftDto, MediaDraftSaveResult, MediaDraftScope,
    MediaDraftSendResult,
};
pub use recent_attachments::{
    RecentAttachment, RecentAttachmentKind, RecentAttachmentPage, RecentAttachmentReference,
    RecentAttachmentSourceBinding,
};
mod chat_mute;
mod message_drafts;
mod native_surfaces;
pub use chat_mute::ChatMuteMode;
pub use message_drafts::{MessageDraftDto, MessageDraftSendResult};
mod pages;
pub use files::FileDto;
pub use pages::{
    MarkdownFileDto, PageBlockDto, PageDto, PageEditResult, PageInputAck, PageInputApplyResult,
    PageInputBindingDto, PageInputCaptureDto, PageInputDto, PageInputMode, PageInputTarget,
    PlanPageBlockDto, PlanPageContentDto, PlanPageDto, PlanPageReadSourceDto, PlanProseKindDto,
    TextSpanDto,
};
mod https;
mod link;
mod link_api;
mod link_pickup;
mod linking;
mod mls;
mod mls_opening;
mod mls_recovery;
mod net;
mod profile;
mod provider_auth;
mod replies;
mod selected_quotes;
pub use selected_quotes::{QuoteSpan, ReplySelection};
pub mod runtime_core;
pub use profile::{PhotoChange, ProfileDto, ProfilePhotoBytes};
mod sync;
pub use api::*;
pub use link_api::{
    DeviceDto, HistoryDto, HistoryStatusDto, LinkRequestDto, LinkedDto, OlderDto, TransferListener,
};
pub use sync::Ingest;

mod approvals;
pub use localization::tr;
use miniapps as apps;
pub mod dto;
pub mod engine;
pub mod i18n;
mod seed;
mod universal;

pub use dto::*;
pub use engine::Engine;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{}{message}", crate::i18n::ts("Armazenamento: ", "Storage: "))]
    Storage { message: String },
    #[error("{}{what}", crate::i18n::ts("Não encontrado: ", "Not found: "))]
    NotFound { what: String },
    #[error("{reason}")]
    Forbidden { reason: String },
    #[error("{reason}")]
    Invalid { reason: String },
    #[error("{reason}")]
    Stale { reason: String },
    /// Only a current, unaccepted private-draft intent refused before append.
    #[error("{reason}")]
    DraftRejected { reason: String },
}

#[derive(uniffi::Object)]
pub struct Client {
    inner: Arc<Mutex<Engine>>,
    lang: i18n::Lang,
    /// The relay connection, while sync runs.
    net: Mutex<Option<net::Net>>,
    history_receive: link_api::HistoryControl,
    link_pickup: link_pickup::PickupControl,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.link_pickup.retire();
    }
}

impl Client {
    fn lock(&self) -> MutexGuard<'_, Engine> {
        // Um panic no meio de uma escrita não deixa estado meio-aplicado na projeção
        // (o disco é a fonte da verdade); recuperamos o lock e seguimos.
        // Every call goes through here, so the core speaks the device language on
        // whichever thread Swift calls from.
        i18n::set(self.lang);
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[uniffi::export]
impl Client {
    /// Opens (or creates) the database at `path` (":memory:" in tests). `locale` is the
    /// device language (BCP-47, e.g. "en-US", "pt-BR"): agent output, demo data and money
    /// formatting follow it. Anything that isn't Portuguese falls back to English.
    #[uniffi::constructor]
    pub fn open(path: String, locale: String) -> Result<Arc<Self>, CoreError> {
        let lang = i18n::Lang::from_tag(&locale);
        let _language = i18n::enter(lang);
        Ok(Arc::new(Self {
            inner: Arc::new(Mutex::new(Engine::open(&path)?)),
            lang,
            net: Mutex::new(None),
            history_receive: link_api::HistoryControl::default(),
            link_pickup: link_pickup::PickupControl::default(),
        }))
    }

    /// Language the core is speaking ("en" or "pt-BR").
    pub fn language(&self) -> String {
        self.lang.tag().to_string()
    }

    /// Cria o elenco e a história da demo se o banco estiver vazio. Devolve `true` se semeou.
    pub fn seed_demo_if_empty(&self) -> Result<bool, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        let mut e = self.lock();
        if !e.is_empty() {
            // Demo data follows the device language: if the story was seeded in another
            // language, start it over (this database only ever holds the demo).
            let seeded_in = e
                .store
                .meta("seed_lang")
                .ok()
                .flatten()
                .unwrap_or_else(|| "pt-BR".into());
            if seeded_in == self.lang.tag() {
                return Ok(false);
            }
            e.wipe()?;
        }
        seed::seed(&mut e)?;
        e.store.set_meta("seed_lang", self.lang.tag())?;
        Ok(true)
    }

    /// Apaga tudo e recomeça a demo.
    pub fn reset_demo(&self) -> Result<(), CoreError> {
        let _language = crate::i18n::enter(self.lang);
        let mut e = self.lock();
        e.wipe()?;
        seed::seed(&mut e)?;
        e.store.set_meta("seed_lang", self.lang.tag())?;
        Ok(())
    }

    pub fn me(&self) -> Result<Persona, CoreError> {
        self.lock().me()
    }

    pub fn personas(&self) -> Vec<Persona> {
        self.lock().personas()
    }

    /// Universal search on this device (FTS5, prefix, diacritics folded). `kinds` empty =
    /// all of message, person, agent, space, app, item; `limit` per kind.
    pub fn universal_search(
        &self,
        query: String,
        kinds: Vec<String>,
        limit: u32,
    ) -> Result<UniversalResults, CoreError> {
        self.lock().universal_search(&query, &kinds, limit)
    }

    pub fn agent_say(
        &self,
        space_id: String,
        agent_id: String,
        text: String,
        ai_cost_cents: i64,
    ) -> Result<TimelineEntry, CoreError> {
        self.lock()
            .agent_say(&space_id, &agent_id, &text, ai_cost_cents)
    }

    pub fn agent_create_plan(
        &self,
        space_id: String,
        agent_id: String,
        prompt: String,
        plan: PlanDto,
        engine_label: String,
        ai_cost_cents: i64,
    ) -> Result<PlanOutcome, CoreError> {
        self.lock().agent_create_plan(
            &space_id,
            &agent_id,
            &prompt,
            plan,
            &engine_label,
            ai_cost_cents,
        )
    }

    pub fn item(&self, item_id: String) -> Result<ItemDetail, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock().item(&item_id)
    }

    pub fn item_at(&self, item_id: String, version: u32) -> Result<ItemDetail, CoreError> {
        self.lock().item_at(&item_id, version)
    }

    pub fn items(&self) -> Vec<ItemDetail> {
        self.lock().items()
    }

    pub fn edit_plan_line(
        &self,
        item_id: String,
        line_id: String,
        text: String,
        cost_cents: i64,
        observed_hash: String,
    ) -> Result<EditOutcome, CoreError> {
        self.lock()
            .edit_plan_line(&item_id, &line_id, &text, cost_cents, &observed_hash)
    }

    pub fn toggle_plan_line(
        &self,
        item_id: String,
        line_id: String,
        observed_hash: String,
    ) -> Result<EditOutcome, CoreError> {
        self.lock()
            .toggle_plan_line(&item_id, &line_id, &observed_hash)
    }

    pub fn add_plan_line(
        &self,
        item_id: String,
        section_index: u32,
        text: String,
        cost_cents: i64,
        observed_hash: String,
    ) -> Result<EditOutcome, CoreError> {
        self.lock()
            .add_plan_line(&item_id, section_index, &text, cost_cents, &observed_hash)
    }

    pub fn remove_plan_line(
        &self,
        item_id: String,
        line_id: String,
        observed_hash: String,
    ) -> Result<EditOutcome, CoreError> {
        self.lock()
            .remove_plan_line(&item_id, &line_id, &observed_hash)
    }

    pub fn undo(&self, token: UndoToken) -> Result<ItemDetail, CoreError> {
        self.lock().undo(&token)
    }

    pub fn restore_plan_version(
        &self,
        item_id: String,
        version: u32,
        observed_hash: String,
    ) -> Result<ItemDetail, CoreError> {
        self.lock()
            .restore_plan_version(&item_id, version, &observed_hash)
    }

    pub fn restore_version(&self, item_id: String, version: u32) -> Result<ItemDetail, CoreError> {
        self.lock().restore_version(
            &item_id,
            version,
            &tr!("Restaurou a v{version}", "Restored v{version}"),
        )
    }

    pub fn requests(&self) -> Vec<AgentRequestDto> {
        self.lock().requests()
    }

    /// A swipe on the approvals stack: approve / deny / always approve / always deny.
    pub fn decide_request(
        &self,
        request_id: String,
        decision: RequestDecision,
    ) -> Result<DecideOutcome, CoreError> {
        self.lock().decide_request(&request_id, decision)
    }

    pub fn decide_tool_request(
        &self,
        request_id: String,
        observed_content_hash: String,
        decision: ToolRequestDecision,
    ) -> Result<DecideOutcome, CoreError> {
        self.lock()
            .decide_tool_request(&request_id, &observed_content_hash, decision)
    }

    /// Standing "always approve / always deny" decisions you gave your agents.
    pub fn standing_decisions(&self) -> Vec<StandingDecisionDto> {
        self.lock().standing_decisions()
    }

    /// Revokes a standing decision: the agent asks again next time.
    pub fn revoke_standing(&self, grant_id: String) -> Result<(), CoreError> {
        self.lock().revoke_standing(&grant_id)
    }

    pub fn approve_request(&self, request_id: String) -> Result<ApproveOutcome, CoreError> {
        self.lock().resolve_request(&request_id, true)
    }

    pub fn deny_request(&self, request_id: String) -> Result<ApproveOutcome, CoreError> {
        self.lock().resolve_request(&request_id, false)
    }

    pub fn approve_all(&self, agent_id: String) -> Result<u32, CoreError> {
        self.lock().approve_all(&agent_id)
    }

    pub fn agents(&self) -> Vec<AgentProfile> {
        self.lock().agents()
    }

    pub fn agent_profile(&self, agent_id: String) -> AgentProfile {
        self.lock().agent_profile(&agent_id)
    }

    pub fn set_trust(
        &self,
        agent_id: String,
        space_id: String,
        level: TrustLevelDto,
    ) -> Result<(), CoreError> {
        self.lock()
            .set_trust(&agent_id, &space_id, engine::trust_from_dto(level))
    }

    pub fn raise_budget(
        &self,
        agent_id: String,
        extra_cents: i64,
    ) -> Result<AgentProfile, CoreError> {
        self.lock().raise_budget(&agent_id, extra_cents)
    }

    pub fn preview_decisions(&self, agent_id: String, space_id: String) -> Vec<DecisionPreview> {
        self.lock().preview_decisions(&agent_id, &space_id)
    }

    pub fn verify_log(&self, space_id: String) -> LogReport {
        self.lock().verify_log(&space_id)
    }

    pub fn verify_all(&self) -> Vec<LogReport> {
        self.lock().verify_all()
    }

    pub fn agent_activity(&self, agent_id: String) -> Result<Vec<AgentActivityDto>, CoreError> {
        self.lock().agent_activity(&agent_id, 30)
    }

    pub fn log_events(&self, space_id: String) -> Result<Vec<LogEventDto>, CoreError> {
        self.lock().log_events(&space_id)
    }

    /// A bounded page of message matches within one conversation. A stale cursor
    /// returns an error; callers restart instead of displaying an incomplete page.
    pub fn search_messages(
        &self,
        space_id: String,
        query: String,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<MessageSearchPage, CoreError> {
        self.lock()
            .search_messages(&space_id, &query, cursor.as_deref(), limit)
    }

    pub fn search(&self, query: String) -> Vec<SearchHit> {
        self.lock().search(&query)
    }

    pub fn mentions(&self) -> Vec<Mention> {
        self.lock().mentions()
    }

    pub fn stats(&self) -> CoreStats {
        self.lock().stats()
    }

    // ── MCP Apps ──

    /// Os mini-apps embutidos e suas ferramentas (`tools/list` com `_meta.ui`).
    pub fn app_specs(&self) -> Vec<AppSpecDto> {
        self.lock().app_specs()
    }

    /// Does this mini-app hold a live Grant for a native capability?
    pub fn app_device_allowed(&self, item_id: String, capability: String) -> bool {
        self.lock().app_device_allowed(&item_id, &capability)
    }

    /// You allowed a native capability for a mini-app (`always` = until you revoke it).
    pub fn grant_app_device(
        &self,
        item_id: String,
        capability: String,
        purpose: String,
        always: bool,
    ) -> Result<String, CoreError> {
        self.lock()
            .grant_app_device(&item_id, &capability, &purpose, always)
    }

    pub fn revoke_app_device(&self, grant_id: String) -> Result<(), CoreError> {
        self.lock().revoke_app_device(&grant_id)
    }

    /// Every live native-capability Grant held by a mini-app.
    pub fn app_device_grants(&self) -> Vec<DeviceGrantDto> {
        self.lock().app_device_grants()
    }

    /// `resources/read` de um recurso `ui://` (HTML `text/html;profile=mcp-app`).
    pub fn read_app_resource(&self, uri: String) -> Result<AppResourceDto, CoreError> {
        self.lock().read_app_resource(&uri)
    }

    /// O agente cria um mini-app no Espaço (ferramenta com visibilidade `model`).
    pub fn agent_create_app(
        &self,
        space_id: String,
        agent_id: String,
        start_tool: String,
        args_json: String,
        engine_label: String,
        prompt: String,
    ) -> Result<PlanOutcome, CoreError> {
        self.lock().agent_create_app(
            &space_id,
            &agent_id,
            &start_tool,
            &args_json,
            &engine_label,
            &prompt,
        )
    }

    /// Installs a catalog mini-app as the current user, with an Item-scoped grant.
    pub fn install_app(
        &self,
        space_id: String,
        app_id: String,
        args_json: String,
    ) -> Result<ItemDetail, CoreError> {
        self.lock().install_app(&space_id, &app_id, &args_json)
    }

    /// `tools/call` da interface do mini-app, pelo crivo de Concessões.
    pub fn app_call_tool(
        &self,
        item_id: String,
        tool: String,
        args_json: String,
        confirmed: bool,
    ) -> Result<AppCallOutcome, CoreError> {
        self.lock()
            .app_call_tool(&item_id, &tool, &args_json, confirmed)
    }

    /// Demonstração: a ação de outro membro chegando como se viesse do aparelho dele.
    pub fn demo_member_app_call(
        &self,
        item_id: String,
        member_handle: String,
        tool: String,
        args_json: String,
    ) -> Result<AppCallOutcome, CoreError> {
        self.lock()
            .demo_member_app_call(&item_id, &member_handle, &tool, &args_json)
    }

    /// Demonstration (Debug showcase only): one of your agents asks for something.
    /// `action` is a standing key ("external", "third_party_data", "reversible", "money"…);
    /// `cents` is used for "money". Goes through the evaluator like any request.
    #[allow(clippy::too_many_arguments)]
    pub fn demo_open_request(
        &self,
        space_id: String,
        agent_handle: String,
        title: String,
        detail: String,
        audience: String,
        action: String,
        cents: i64,
    ) -> Result<Option<String>, CoreError> {
        use types::ActionClass as A;
        let class = match action.as_str() {
            "reply" => A::Reply,
            "reversible" => A::Reversible,
            "external" => A::External,
            "irreversible" => A::Irreversible,
            "money" => A::Money { cents },
            "public_audience" => A::PublicAudience,
            "third_party_data" => A::ThirdPartyData,
            other => {
                return Err(CoreError::Invalid {
                    reason: format!("unknown action {other}"),
                });
            }
        };
        self.lock()
            .demo_open_request(&space_id, &agent_handle, &title, &detail, &audience, class)
    }

    /// Demonstração: mensagem de outro membro, como se tivesse chegado pela sincronização.
    pub fn demo_member_say(
        &self,
        space_id: String,
        member_handle: String,
        text: String,
    ) -> Result<TimelineEntry, CoreError> {
        self.lock()
            .demo_member_say(&space_id, &member_handle, &text)
    }
}

// Keep UniFFI module_path! identical to the existing crate-root exports.
include!("client_conversations.rs");

/// Formats cents as Brazilian reais ("R$ 1.348", "R$ 12,40").
#[uniffi::export]
pub fn format_brl(cents: i64) -> String {
    engine::brl(cents)
}

/// Formats the existing plan/grant domain's BRL cents for the chosen locale.
#[uniffi::export]
pub fn format_money(cents: i64, locale: String) -> String {
    i18n::money_in(i18n::Lang::from_tag(&locale), cents)
}

/// Versão do núcleo (para a tela Sobre).
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod recovery_ui_tests;
#[cfg(test)]
mod tests;
