//! # roda-ffi
//!
//! A fachada do núcleo para os apps (Swift hoje, Kotlin em 2027) via UniFFI.
//! API **síncrona** e pequena: o app chama, o núcleo avalia Concessões, assina,
//! grava e devolve visões prontas para a UI.
//!
//! Seams reservados (não implementados neste protótipo): Roda Sync (relay + outbox),
//! MLS (OpenMLS) nos Espaços privados, Loro para Itens, callbacks de mudança → AsyncStream.

use std::sync::{Arc, Mutex, MutexGuard};

mod api;
mod files;
mod growth;
pub use growth::{AcquisitionDto, GrowthSyncDto, OnboardingPlanDto};
mod files_api;
mod liveview;
pub use liveview::{LiveViewDemoVm, LiveViewInput, LiveViewKey, LiveViewSession};
mod media;
mod pages;
pub use files::FileDto;
pub use pages::{MarkdownFileDto, PageBlockDto, PageDto, TextSpanDto};
mod mls;
mod net;
mod profile;
mod replies;
pub use profile::{PhotoChange, ProfileDto};
mod sync;
pub use api::*;

mod apps;
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
    #[error("{}{reason}", crate::i18n::ts("Armazenamento: ", "Storage: "))]
    Storage { reason: String },
    #[error("{}{what}", crate::i18n::ts("Não encontrado: ", "Not found: "))]
    NotFound { what: String },
    #[error("{reason}")]
    Forbidden { reason: String },
    #[error("{reason}")]
    Invalid { reason: String },
    #[error("{reason}")]
    Stale { reason: String },
}

#[derive(uniffi::Object)]
pub struct RodaEngine {
    inner: Arc<Mutex<Engine>>,
    lang: i18n::Lang,
    /// The relay connection, while sync runs.
    net: Mutex<Option<net::Net>>,
}

impl RodaEngine {
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
impl RodaEngine {
    /// Opens (or creates) the database at `path` (":memory:" in tests). `locale` is the
    /// device language (BCP-47, e.g. "en-US", "pt-BR"): agent output, demo data and money
    /// formatting follow it. Anything that isn't Portuguese falls back to English.
    #[uniffi::constructor]
    pub fn open(path: String, locale: String) -> Result<Arc<Self>, CoreError> {
        let lang = i18n::Lang::from_tag(&locale);
        i18n::set(lang);
        Ok(Arc::new(Self {
            inner: Arc::new(Mutex::new(Engine::open(&path)?)),
            lang,
            net: Mutex::new(None),
        }))
    }

    /// Language the core is speaking ("en" or "pt-BR").
    pub fn language(&self) -> String {
        self.lang.tag().to_string()
    }

    /// Cria o elenco e a história da demo se o banco estiver vazio. Devolve `true` se semeou.
    pub fn seed_demo_if_empty(&self) -> Result<bool, CoreError> {
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

    pub fn spaces(&self) -> Vec<SpaceSummary> {
        self.lock().spaces()
    }

    pub fn space(&self, space_id: String) -> Result<SpaceSummary, CoreError> {
        self.lock().space_summary(&space_id)
    }

    pub fn timeline(&self, space_id: String) -> Result<Vec<TimelineEntry>, CoreError> {
        self.lock().timeline(&space_id)
    }

    pub fn mark_read(&self, space_id: String) -> Result<(), CoreError> {
        self.lock().mark_read(&space_id)
    }

    pub fn send_message(&self, space_id: String, text: String) -> Result<TimelineEntry, CoreError> {
        self.lock().send_message(&space_id, &text)
    }

    /// Stores an attachment by content hash (sha256) and returns its reference.
    pub fn put_media(
        &self,
        bytes: Vec<u8>,
        mime: String,
        width: u32,
        height: u32,
    ) -> Result<MediaRefDto, CoreError> {
        self.lock().put_media(&bytes, &mime, width, height)
    }

    /// The attachment's bytes, verified against the hash (`None` if missing or tampered).
    pub fn media(&self, sha256: String) -> Result<Option<Vec<u8>>, CoreError> {
        self.lock().media(&sha256)
    }

    /// Sets the chat's background for everyone (a signed `BackgroundSet` event).
    pub fn set_background(
        &self,
        space_id: String,
        background: BackgroundDto,
    ) -> Result<TimelineEntry, CoreError> {
        self.lock().set_background(&space_id, &background)
    }

    /// The chat's current shared background, if any.
    pub fn background(&self, space_id: String) -> Result<Option<BackgroundDto>, CoreError> {
        self.lock().background(&space_id)
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
        self.lock().item(&item_id)
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
    ) -> Result<EditOutcome, CoreError> {
        self.lock()
            .edit_plan_line(&item_id, &line_id, &text, cost_cents)
    }

    pub fn toggle_plan_line(
        &self,
        item_id: String,
        line_id: String,
    ) -> Result<EditOutcome, CoreError> {
        self.lock().toggle_plan_line(&item_id, &line_id)
    }

    pub fn add_plan_line(
        &self,
        item_id: String,
        section_index: u32,
        text: String,
        cost_cents: i64,
    ) -> Result<EditOutcome, CoreError> {
        self.lock()
            .add_plan_line(&item_id, section_index, &text, cost_cents)
    }

    pub fn remove_plan_line(
        &self,
        item_id: String,
        line_id: String,
    ) -> Result<EditOutcome, CoreError> {
        self.lock().remove_plan_line(&item_id, &line_id)
    }

    pub fn undo(&self, token: UndoToken) -> Result<ItemDetail, CoreError> {
        self.lock().undo(&token)
    }

    pub fn restore_version(&self, item_id: String, version: u32) -> Result<ItemDetail, CoreError> {
        self.lock()
            .restore_version(&item_id, version, &format!("Restaurou a v{version}"))
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

    pub fn search(&self, query: String) -> Vec<SearchHit> {
        self.lock().search(&query)
    }

    pub fn mentions(&self) -> Vec<Mention> {
        self.lock().mentions()
    }

    pub fn stats(&self) -> CoreStats {
        self.lock().stats()
    }

    pub fn invite_link(&self, space_id: String) -> Result<String, CoreError> {
        self.lock().invite_link(&space_id)
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
        use roda_types::ActionClass as A;
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
                })
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

/// Formats cents as Brazilian reais ("R$ 1.348", "R$ 12,40").
#[uniffi::export]
pub fn format_brl(cents: i64) -> String {
    engine::brl(cents)
}

/// Formats cents for a locale: "$1,348" (en) or "R$ 1.348" (pt-BR).
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
mod tests;
