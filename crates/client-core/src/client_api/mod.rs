//! The application boundary for the shared Expo client.
//!
//! Native adapters own the engine and SecretVault. JavaScript can request domain
//! operations, never private keys, database paths, or arbitrary native execution.
//! Every mutation below goes through the same engine methods as the native apps.

mod command;
mod content_dispatch;
mod conversation_dispatch;
use command::Command;

mod handle;
mod integrity;
mod wire;

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    AccountDto, AgentProfile, AgentRequestDto, Client, ConnectionDto, CoreError, PageBlockDto,
    Persona, RequestDecision, SecretVault, SpaceSummary, TimelineEntry, ToolRequestDecision,
    TrustLevelDto, UndoToken,
};

pub(crate) const MAX_REQUEST_BYTES: usize = 1 << 20;
const MAX_TIMELINE_PAGE: u32 = 200;

#[derive(Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
struct Snapshot {
    account: Option<AccountDto>,
    me: Option<Persona>,
    connection: ConnectionDto,
    conversations: Vec<SpaceSummary>,
    integrity_warnings: std::collections::BTreeMap<String, String>,
    has_unreadable_local_history: bool,
    requests: Vec<AgentRequestDto>,
    agents: Vec<AgentProfile>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
struct TimelinePage {
    entries: Vec<TimelineEntry>,
    before: Option<String>,
    has_more: bool,
}

#[derive(Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
struct LogEventsPage {
    events: Vec<crate::LogEventDto>,
    before: Option<String>,
}

fn page_size() -> u32 {
    80
}

/// Export the exact Rust input/output contract; the UI never copies DTO shapes.
#[cfg(feature = "client-types")]
pub fn export_typescript(path: &std::path::Path) -> Result<(), ts_rs::ExportError> {
    use ts_rs::TS;
    let config = ts_rs::Config::new()
        .with_large_int("string")
        .with_out_dir(path);
    Command::export_all(&config)?;
    Snapshot::export_all(&config)?;
    crate::MessageDraftDto::export_all(&config)?;
    crate::MessageDraftSendResult::export_all(&config)?;
    TimelinePage::export_all(&config)?;
    LogEventsPage::export_all(&config)?;
    crate::ItemDetail::export_all(&config)?;
    crate::NativeSurfaceItemDto::export_all(&config)?;
    crate::NativeSurfaceAppDto::export_all(&config)?;
    crate::EditOutcome::export_all(&config)?;
    crate::PageDto::export_all(&config)?;
    crate::PlanPageDto::export_all(&config)?;
    crate::SearchHit::export_all(&config)?;
    crate::MessageSearchPage::export_all(&config)?;
    crate::RecentAttachmentPage::export_all(&config)?;
    crate::RecentAttachmentSourceBinding::export_all(&config)?;
    crate::AppSpecDto::export_all(&config)?;
    crate::AppDocumentDto::export_all(&config)?;
    crate::AppBinaryDocumentDto::export_all(&config)?;
    crate::AppCallOutcome::export_all(&config)?;
    crate::AppResourceDto::export_all(&config)?;
    crate::DeviceGrantDto::export_all(&config)?;
    crate::DecisionPreview::export_all(&config)?;
    crate::AgentActivityDto::export_all(&config)?;
    crate::Mention::export_all(&config)?;
    crate::DeviceDto::export_all(&config)?;
    crate::LinkRequestDto::export_all(&config)?;
    crate::LinkedDto::export_all(&config)?;
    crate::HistoryDto::export_all(&config)?;
    crate::HistoryStatusDto::export_all(&config)?;
    crate::OlderDto::export_all(&config)?;
    crate::BackupStatusDto::export_all(&config)?;
    crate::InviteDto::export_all(&config)?;
    crate::InvitePreviewDto::export_all(&config)?;
    crate::ProfileDto::export_all(&config)?;
    crate::StandingDecisionDto::export_all(&config)?;
    crate::LogReport::export_all(&config)?;
    crate::LogEventDto::export_all(&config)?;
    crate::DecideOutcome::export_all(&config)?;
    crate::PageEditResult::export_all(&config)?;
    crate::AgentStatus::export_all(&config)?;
    crate::AgentDefinitionDto::export_all(&config)?;
    crate::AgentDefinitionPreviewDto::export_all(&config)?;
    crate::AgentDefinitionUpdateDto::export_all(&config)?;
    crate::AgentDefinitionUpdateOutcome::export_all(&config)?;
    crate::AgentEnrollmentPreviewDto::export_all(&config)?;
    crate::AgentEnrollmentDeliveryDto::export_all(&config)?;
    crate::AgentEnrollmentStatusDto::export_all(&config)?;
    crate::UniversalResults::export_all(&config)?;
    Ok(())
}

fn definition_target(
    agent: String,
    identity: String,
    device: String,
) -> crate::agent_definition::DefinitionTarget {
    crate::agent_definition::DefinitionTarget {
        agent,
        identity,
        device,
    }
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Invalid {
        reason: message.into(),
    }
}

fn encode(value: &impl Serialize) -> Result<String, CoreError> {
    wire::to_json(value).map_err(|_| invalid("Couldn't encode the application response."))
}

fn decode(request: &str) -> Result<Command, CoreError> {
    // Only document writes carry a bounded full scene. Inspect their borrowed op
    // before allocating a large command; every existing operation keeps its ceiling.
    if request.len() > crate::app_documents::MAX_SCENE_BYTES * 6 + 8 * 1024 {
        return Err(invalid("Application request is too large."));
    }
    if request.len() > MAX_REQUEST_BYTES + 8 * 1024 {
        #[derive(serde::Deserialize)]
        struct Envelope<'a> {
            #[serde(borrow)]
            op: &'a str,
        }
        let envelope: Envelope<'_> =
            serde_json::from_str(request).map_err(|_| invalid("Invalid application command."))?;
        let binary = matches!(envelope.op, "app_binary_create" | "app_binary_save");
        if (binary && request.len() > crate::app_binary_documents::MAX_BASE64_BYTES + 8 * 1024)
            || (!binary && !matches!(envelope.op, "app_document_save" | "app_document_copy"))
        {
            return Err(invalid("Application request is too large."));
        }
    }
    let command: Command =
        serde_json::from_str(request).map_err(|_| invalid("Invalid application command."))?;
    if request.len() > MAX_REQUEST_BYTES
        && !matches!(
            command,
            Command::AppDocumentSave { .. }
                | Command::AppDocumentCopy { .. }
                | Command::AppBinarySave { .. }
                | Command::AppBinaryCreate { .. }
        )
        && !(request.len() <= MAX_REQUEST_BYTES + 8 * 1024
            && matches!(
                command,
                Command::MessageDraft { .. }
                    | Command::SaveMessageDraft { .. }
                    | Command::SendMessageDraft { .. }
            ))
    {
        return Err(invalid("Application request is too large."));
    }
    Ok(command)
}

fn transfer_timeout(milliseconds: u32) -> Result<u64, CoreError> {
    if !(1_000..=120_000).contains(&milliseconds) {
        return Err(invalid("Transfer waits must be between 1 and 120 seconds."));
    }
    Ok(u64::from(milliseconds))
}

fn account_vault(vault: &Option<Arc<dyn SecretVault>>) -> Result<Arc<dyn SecretVault>, CoreError> {
    vault
        .clone()
        .ok_or_else(|| invalid("This operation requires the account's protected native vault."))
}

fn cents(value: String) -> Result<i64, CoreError> {
    if value.trim() != value || value.starts_with('+') || value.is_empty() {
        return Err(invalid("Invalid signed cent value."));
    }
    value
        .parse()
        .map_err(|_| invalid("Invalid signed cent value."))
}

fn timeline_page(
    entries: Vec<TimelineEntry>,
    before: Option<String>,
    limit: u32,
) -> Result<TimelinePage, CoreError> {
    if limit == 0 || limit > MAX_TIMELINE_PAGE {
        return Err(invalid("Timeline pages contain 1 to 200 entries."));
    }
    let end = match before {
        None => entries.len(),
        Some(id) => entries
            .iter()
            .position(|entry| entry.id == id)
            .ok_or_else(|| CoreError::Stale {
                reason: "The timeline changed. Reload this conversation.".into(),
            })?,
    };
    let start = end.saturating_sub(limit as usize);
    let entries = entries[start..end].to_vec();
    Ok(TimelinePage {
        before: entries.first().map(|entry| entry.id.clone()),
        entries,
        has_more: start > 0,
    })
}

#[uniffi::export(async_runtime = "tokio")]
impl Client {
    /// Typed domain commands for the Expo app. Native code retains keys and lifetime.
    /// i64/u64 response values are decimal strings, never lossy JS numbers.
    pub async fn client_request(&self, request: String) -> Result<String, CoreError> {
        crate::i18n::scope(self.lang, async {
            self.dispatch_client(decode(&request)?, None).await
        })
        .await
    }

    /// The native account session injects its own vault. JavaScript may request
    /// specific account operations but cannot name, read or replace vault entries.
    pub async fn client_vault_request(
        &self,
        request: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<String, CoreError> {
        crate::i18n::scope(self.lang, async {
            self.dispatch_client(decode(&request)?, Some(vault)).await
        })
        .await
    }
}

impl Client {
    fn client_snapshot(&self) -> Snapshot {
        // A broken signed log has no trusted membership to expose. Report only
        // its presence, without deriving names or audit access from bad events.
        let has_unreadable_local_history = !self.lock().broken_spaces.is_empty();
        Snapshot {
            account: self.account(),
            me: self.me().ok(),
            connection: self.connection(),
            conversations: self.spaces(),
            integrity_warnings: self.client_integrity_warnings(),
            has_unreadable_local_history,
            requests: self.requests(),
            agents: self.agents(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};

    #[derive(Default)]
    struct Vault(Mutex<HashMap<String, Vec<u8>>>);
    impl SecretVault for Vault {
        fn load(&self, key: String) -> Option<Vec<u8>> {
            self.0.lock().unwrap().get(&key).cloned()
        }
        fn save(&self, key: String, value: Vec<u8>) -> bool {
            self.0.lock().unwrap().insert(key, value);
            true
        }
        fn delete(&self, key: String) {
            self.0.lock().unwrap().remove(&key);
        }
    }

    #[tokio::test]
    async fn account_commands_require_the_injected_vault_and_never_expose_its_keys() {
        let core = Client::open(":memory:".into(), "pt-BR".into()).unwrap();
        let request = serde_json::json!({"op":"link_request","relay_url":"http://127.0.0.1:9000"})
            .to_string();
        assert!(core
            .client_request(request.clone())
            .await
            .unwrap_err()
            .to_string()
            .contains("protected native vault"));
        let vault = Arc::new(Vault::default());
        let result: serde_json::Value = serde_json::from_str(
            &core
                .client_vault_request(request, vault.clone())
                .await
                .unwrap(),
        )
        .unwrap();
        let code = result["code"].as_str().unwrap();
        assert_eq!(result["check"], core.link_code_check(code.into()).unwrap());
        assert_eq!(result.as_object().unwrap().len(), 2);
        {
            let keys = vault.0.lock().unwrap();
            assert_eq!(keys.len(), 1);
            assert!(keys.contains_key(crate::link::VAULT_PENDING));
        }
        let invalid = serde_json::json!({"op":"backup_password","password":"keep-this-secret","vault_key":"arbitrary"}).to_string();
        let error = core
            .client_vault_request(invalid, vault.clone())
            .await
            .unwrap_err()
            .to_string();
        assert!(!error.contains("keep-this-secret"));
        assert_eq!(vault.0.lock().unwrap().len(), 1);
        assert!(core
            .client_vault_request("x".repeat(MAX_REQUEST_BYTES + 1), vault)
            .await
            .is_err());
    }

    #[test]
    fn transfer_waits_are_bounded_before_network_work() {
        assert!(transfer_timeout(999).is_err());
        assert_eq!(transfer_timeout(1_000).unwrap(), 1_000);
        assert_eq!(transfer_timeout(120_000).unwrap(), 120_000);
        assert!(transfer_timeout(120_001).is_err());
    }

    #[test]
    fn commands_refuse_unknown_fields_and_unsafe_cent_numbers() {
        assert!(
            serde_json::from_str::<Command>(r#"{"op":"snapshot","database":"/private/keys"}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<Command>(
            r#"{"op":"raise_budget","agent_id":"a","extra_cents":9007199254740993}"#
        )
        .is_err());
        assert_eq!(cents("9223372036854775807".into()).unwrap(), i64::MAX);
        assert_eq!(cents("-9223372036854775808".into()).unwrap(), i64::MIN);
        assert!(cents("9223372036854775808".into()).is_err());
        assert!(cents(" 1".into()).is_err());
    }

    #[tokio::test]
    async fn application_boundary_reads_and_mutates_the_real_engine() {
        let core = Client::open(":memory:".into(), "pt-BR".into()).unwrap();
        core.seed_demo_if_empty().unwrap();
        let snapshot: serde_json::Value = serde_json::from_str(
            &core
                .client_request(r#"{"op":"snapshot"}"#.into())
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!snapshot["conversations"].as_array().unwrap().is_empty());
        let space = snapshot["conversations"][0]["id"].as_str().unwrap();
        let request = serde_json::json!({ "op": "send_message", "space_id": space, "text": "Olá do cliente compartilhado 🪴" });
        let sent: serde_json::Value =
            serde_json::from_str(&core.client_request(request.to_string()).await.unwrap()).unwrap();
        assert!(sent["seq"].is_string());
        let entries = core.timeline(space.into()).unwrap();
        assert!(entries
            .iter()
            .any(|entry| entry.id == sent["id"].as_str().unwrap()));
        assert!(core
            .client_request(r#"{"op":"timeline","space_id":"missing","limit":0}"#.into())
            .await
            .is_err());
        assert!(core
            .client_request("x".repeat(MAX_REQUEST_BYTES + 1))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn main_timeline_excludes_thread_replies_but_exact_message_lookup_keeps_them() {
        let core = Client::open(":memory:".into(), "pt-BR".into()).unwrap();
        core.seed_demo_if_empty().unwrap();
        let space = core.spaces()[0].id.clone();
        let root = core
            .send_message(space.clone(), "Mensagem original".into())
            .unwrap();
        let reply = core
            .send_reply(space.clone(), "Só na thread".into(), root.id.clone(), true)
            .unwrap();
        let timeline: serde_json::Value = serde_json::from_str(
            &core
                .client_request(serde_json::json!({"op":"timeline", "space_id":space}).to_string())
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(timeline["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["id"] == root.id));
        assert!(!timeline["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["id"] == reply.id));
        let exact: serde_json::Value = serde_json::from_str(
            &core
                .client_request(
                    serde_json::json!({"op":"message", "space_id":space, "message_id":reply.id})
                        .to_string(),
                )
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(exact["in_thread"], root.id);
    }

    #[test]
    fn native_editor_markdown_roundtrips_unicode_marks_and_block_structure() {
        for markdown in [
            "## Ação **junta** 🪴",
            "- [x] Conferir *amanhã*",
            "> Uma [referência](https://example.com)",
            "```rust\nfn main() {}\n```",
            "Antes `do código` e ~~depois~~",
        ] {
            let blocks = crate::pages::parse_editor_markdown(markdown).unwrap();
            assert!(!blocks.is_empty());
            for block in blocks {
                let encoded = crate::pages::editor_block_markdown(&block).unwrap();
                let decoded = crate::pages::parse_editor_markdown(&encoded).unwrap();
                assert_eq!(decoded.len(), 1, "{encoded}");
                let mut restored = decoded.into_iter().next().unwrap();
                restored.id = block.id.clone();
                assert_eq!(restored, block, "{encoded}");
            }
        }
    }
}

#[cfg(test)]
mod session_language_tests {
    use super::*;

    #[tokio::test]
    async fn two_engine_languages_cover_prelock_network_errors_and_sync_lifecycle_errors() {
        crate::i18n::set(crate::i18n::Lang::En);
        let portuguese = Client::open(":memory:".into(), "pt-BR".into()).unwrap();
        let english = Client::open(":memory:".into(), "en".into()).unwrap();
        assert_eq!(crate::i18n::lang(), crate::i18n::Lang::En);
        for (engine, no_key, offline) in [
            (
                &portuguese,
                "Sem chave deste aparelho.",
                "Sem conexão com o servidor agora.",
            ),
            (
                &english,
                "This device's key isn't loaded.",
                "Can't reach the server right now.",
            ),
        ] {
            assert_eq!(engine.start_sync(None).unwrap_err().to_string(), no_key);
            assert_eq!(crate::i18n::lang(), crate::i18n::Lang::En);
            tokio::task::yield_now().await;
            // This command returns a localized relay error before it ever locks
            // the engine. Decode and the async dispatch must already be scoped.
            let error = engine
                .client_request(
                    r#"{"op":"find_people","query":"synthetic-language-fixture"}"#.into(),
                )
                .await
                .unwrap_err();
            assert_eq!(error.to_string(), offline);
            assert_eq!(engine.start_sync(None).unwrap_err().to_string(), no_key);
            assert_eq!(crate::i18n::lang(), crate::i18n::Lang::En);
        }
    }
}
