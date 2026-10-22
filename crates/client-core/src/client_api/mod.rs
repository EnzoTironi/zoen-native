//! The application boundary for the shared Expo client.
//!
//! Native adapters own the engine and SecretVault. JavaScript can request domain
//! operations, never private keys, database paths, or arbitrary native execution.
//! Every mutation below goes through the same engine methods as the native apps.

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

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    ProviderSignIn {
        relay_url: String,
        proof_token: String,
        name: String,
    },
    NewMutationId {},
    PageParseMarkdown {
        markdown: String,
    },
    PageBlockMarkdown {
        block: PageBlockDto,
    },
    Snapshot {},
    Timeline {
        space_id: String,
        #[serde(default)]
        before: Option<String>,
        #[serde(default = "page_size")]
        limit: u32,
    },
    MessageDraft {
        expected_identity_id: String,
        expected_device_id: String,
        space_id: String,
        thread_root_id: Option<String>,
    },
    SaveMessageDraft {
        expected_identity_id: String,
        expected_device_id: String,
        space_id: String,
        thread_root_id: Option<String>,
        expected_revision: String,
        text: String,
        reply_to: Option<String>,
    },
    SendMessageDraft {
        expected_identity_id: String,
        expected_device_id: String,
        space_id: String,
        thread_root_id: Option<String>,
        expected_revision: String,
    },
    RecoverFailedMediaDraft {
        scope: crate::MediaDraftScope,
        message_id: String,
        expected_revision: String,
        expected_message_revision: String,
    },
    SendMessage {
        space_id: String,
        text: String,
        #[serde(default)]
        reply_to: Option<String>,
        #[serde(default)]
        thread: bool,
    },
    EditMessage {
        space_id: String,
        message_id: String,
        text: String,
    },
    DeleteMessage {
        space_id: String,
        message_id: String,
    },
    React {
        space_id: String,
        message_id: String,
        emoji: Option<String>,
    },
    ForwardMessage {
        source_space: String,
        message_id: String,
        target_space: String,
    },
    MarkRead {
        space_id: String,
    },
    SetTyping {
        space_id: String,
        typing: bool,
    },
    Thread {
        space_id: String,
        root_id: String,
    },
    Message {
        space_id: String,
        message_id: String,
    },
    UniversalSearch {
        query: String,
        kinds: Vec<String>,
        limit: u32,
    },
    RecentAttachments {
        scope: crate::MediaDraftScope,
        kind: crate::RecentAttachmentKind,
        query: String,
        cursor: Option<String>,
        limit: u32,
    },
    AttachmentReferenceValidate {
        scope: crate::MediaDraftScope,
        kind: crate::RecentAttachmentKind,
        reference: crate::RecentAttachmentReference,
    },
    SetChatPrefs {
        space_id: String,
        pinned: bool,
        archived: bool,
        muted: bool,
    },
    StartDirect {
        identity_id: String,
    },
    CreateGroup {
        title: String,
        member_ids: Vec<String>,
    },
    CreateCommunity {
        title: String,
    },
    FindPeople {
        query: String,
    },
    CreateInvite {
        space_id: String,
    },
    PreviewInvite {
        code: String,
    },
    JoinInvite {
        code: String,
    },
    AddMember {
        space_id: String,
        identity_id: String,
    },
    AddAdmin {
        space_id: String,
        identity_id: String,
    },
    RemoveMember {
        space_id: String,
        identity_id: String,
    },
    LeaveSpace {
        space_id: String,
    },
    EncryptChat {
        space_id: String,
    },
    AgentStatus {
        agent_id: String,
    },
    AgentDefinitionRead {
        agent_id: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
    AgentDefinitionPreview {
        agent_id: String,
        expected_identity_id: String,
        expected_device_id: String,
        observed_version: u32,
        observed_definition_sha256: String,
        persona: String,
        rules: Vec<String>,
    },
    AgentDefinitionUpdate {
        agent_id: String,
        expected_identity_id: String,
        expected_device_id: String,
        observed_version: u32,
        observed_definition_sha256: String,
        persona: String,
        rules: Vec<String>,
        observed_preview_digest: String,
    },
    AgentDefinitionReconcile {
        agent_id: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
    AgentDefinitionDismiss {
        agent_id: String,
        expected_identity_id: String,
        expected_device_id: String,
        receipt_id: String,
        observed_preview_digest: String,
    },
    AgentEnrollmentPreview {
        code: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
    AgentEnrollmentApprove {
        code: String,
        observed_offer_digest: String,
        confirmed_pairing_check: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
    AgentEnrollmentStatus {
        expected_identity_id: String,
        expected_device_id: String,
    },
    AgentEnrollmentDismiss {
        receipt_id: String,
        observed_offer_digest: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
    OpenAgentLink {
        target: String,
    },
    ControlAgent {
        agent_id: String,
        action: crate::AgentControl,
    },
    CreateAgentInvite {
        agent_id: String,
    },
    RevokeAgentInvite {
        agent_id: String,
    },
    Search {
        query: String,
    },
    SearchMessages {
        space_id: String,
        query: String,
        #[serde(default)]
        cursor: Option<String>,
        #[serde(default = "page_size")]
        limit: u32,
    },
    Items {},
    NativeSurfaceItems {
        expected_identity_id: String,
        expected_device_id: String,
        item_ids: Vec<String>,
    },
    Item {
        item_id: String,
    },
    ItemAt {
        item_id: String,
        version: u32,
    },
    Page {
        item_id: String,
    },
    PlanPage {
        item_id: String,
    },
    PageAt {
        item_id: String,
        version: u32,
    },
    PageCreate {
        space_id: String,
        title: String,
    },
    PageImportMarkdown {
        space_id: String,
        path: String,
        markdown: String,
    },
    PageApply {
        item_id: String,
        mutation_id: String,
        edit_context: String,
        order: Vec<String>,
        changed: Vec<PageBlockDto>,
    },
    PageCommit {
        item_id: String,
        note: String,
    },
    PageMarkdown {
        item_id: String,
    },
    TogglePlanLine {
        item_id: String,
        line_id: String,
        observed_hash: String,
    },
    EditPlanLine {
        item_id: String,
        line_id: String,
        text: String,
        cost_cents: String,
        observed_hash: String,
    },
    AddPlanLine {
        item_id: String,
        section_index: u32,
        text: String,
        cost_cents: String,
        observed_hash: String,
    },
    RemovePlanLine {
        item_id: String,
        line_id: String,
        observed_hash: String,
    },
    RestorePlanVersion {
        item_id: String,
        version: u32,
        observed_hash: String,
    },
    RestoreVersion {
        item_id: String,
        version: u32,
    },
    Undo {
        token: UndoToken,
    },
    Requests {},
    DecideRequest {
        request_id: String,
        decision: RequestDecision,
    },
    DecideToolRequest {
        request_id: String,
        observed_content_hash: String,
        decision: ToolRequestDecision,
    },
    StandingDecisions {},
    RevokeStanding {
        grant_id: String,
    },
    Agents {},
    AgentProfile {
        agent_id: String,
    },
    PreviewDecisions {
        agent_id: String,
        space_id: String,
    },
    AgentActivity {
        agent_id: String,
    },
    Mentions {},
    SetTrust {
        agent_id: String,
        space_id: String,
        level: TrustLevelDto,
    },
    RaiseBudget {
        agent_id: String,
        extra_cents: String,
    },
    AppSpecs {},
    AppBinaryRead {
        item_id: String,
        version: Option<u32>,
        expected_sha256: Option<String>,
    },
    AppBinaryCreate {
        space_id: String,
        title: String,
        data_base64: String,
        mutation_id: String,
    },
    AppBinarySave {
        item_id: String,
        expected_version: u32,
        expected_sha256: String,
        data_base64: String,
        mutation_id: String,
        intent_sha256: String,
    },
    AppBinaryResult {
        item_id: String,
        expected_version: u32,
        expected_sha256: String,
        mutation_id: String,
        intent_sha256: String,
    },
    AppDocumentRead {
        item_id: String,
    },
    AppDocumentCopy {
        item_id: String,
        scene_json: String,
        mutation_id: String,
    },
    AppDocumentSave {
        item_id: String,
        expected_version: u32,
        expected_sha256: String,
        scene_json: String,
        mutation_id: String,
    },
    InstallApp {
        space_id: String,
        app_id: String,
        args_json: String,
    },
    AppCallTool {
        item_id: String,
        tool: String,
        args_json: String,
        confirmed: bool,
    },
    AppResource {
        uri: String,
    },
    AppDeviceGrants {},
    AppDeviceAllowed {
        item_id: String,
        capability: String,
    },
    GrantAppDevice {
        item_id: String,
        capability: String,
        purpose: String,
        always: bool,
    },
    RevokeAppDevice {
        grant_id: String,
    },
    Devices {},
    UnlinkDevice {
        device_id: String,
    },
    Profile {
        identity_id: String,
    },
    UpdateProfile {
        name: String,
        bio: String,
    },
    UpdateHandle {
        handle: String,
    },
    BlockPerson {
        identity_id: String,
    },
    UnblockPerson {
        identity_id: String,
    },
    BlockedPeople {},
    VerifyLog {
        space_id: String,
    },
    LogEvents {
        space_id: String,
    },
    LogEventsPage {
        space_id: String,
        before: Option<String>,
        limit: u32,
    },
    BackupStatus {},
    LinkRequest {
        relay_url: String,
    },
    LinkCodeCheck {
        code: String,
    },
    LinkWait {
        timeout_ms: u32,
        expected_identity_id: Option<String>,
    },
    LinkDevice {
        code: String,
    },
    LinkProgress {
        device_id: String,
    },
    SendHistory {
        device_id: String,
    },
    ReceiveHistory {
        timeout_ms: u32,
    },
    HistoryStatus {},
    LoadOlder {
        space_id: String,
        timeout_ms: u32,
    },
    BackupCreateRecoveryKey {},
    BackupEnableRecoveryKey {
        recovery_key: String,
    },
    BackupPassword {
        password: String,
    },
    BackupNow {},
    BackupTurnOff {},
    RestoreBackupPassword {
        relay_url: String,
        handle: String,
        password: String,
        expected_identity_id: Option<String>,
    },
    RestoreBackupRecoveryKey {
        relay_url: String,
        handle: String,
        recovery_key: String,
        expected_identity_id: Option<String>,
    },
    RegisterPush {
        platform: String,
        token: String,
    },
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
    async fn dispatch_client(
        &self,
        command: Command,
        vault: Option<Arc<dyn SecretVault>>,
    ) -> Result<String, CoreError> {
        use Command::*;
        match command {
            ProviderSignIn {
                relay_url,
                proof_token,
                name,
            } => encode(
                &self
                    .provider_sign_in(relay_url, proof_token, name, account_vault(&vault)?)
                    .await?,
            ),
            NewMutationId {} => encode(&types::new_id("client_edit")),
            PageParseMarkdown { markdown } => {
                encode(&crate::pages::parse_editor_markdown(&markdown)?)
            }
            PageBlockMarkdown { block } => encode(&crate::pages::editor_block_markdown(&block)?),
            Snapshot {} => encode(&self.client_snapshot()),
            Timeline {
                space_id,
                before,
                limit,
            } => encode(&timeline_page(
                self.timeline(space_id)?
                    .into_iter()
                    .filter(|entry| entry.in_thread.is_none())
                    .collect(),
                before,
                limit,
            )?),
            MessageDraft {
                expected_identity_id,
                expected_device_id,
                space_id,
                thread_root_id,
            } => {
                let scope = crate::message_drafts::DraftScope {
                    expected_identity_id,
                    expected_device_id,
                    space_id,
                    thread_root_id,
                };
                encode(&self.lock().message_draft(&scope)?)
            }
            SaveMessageDraft {
                expected_identity_id,
                expected_device_id,
                space_id,
                thread_root_id,
                expected_revision,
                text,
                reply_to,
            } => {
                let scope = crate::message_drafts::DraftScope {
                    expected_identity_id,
                    expected_device_id,
                    space_id,
                    thread_root_id,
                };
                encode(&self.lock().save_message_draft(
                    &scope,
                    &expected_revision,
                    text,
                    reply_to,
                )?)
            }
            SendMessageDraft {
                expected_identity_id,
                expected_device_id,
                space_id,
                thread_root_id,
                expected_revision,
            } => {
                let scope = crate::message_drafts::DraftScope {
                    expected_identity_id,
                    expected_device_id,
                    space_id,
                    thread_root_id,
                };
                encode(&self.lock().send_message_draft(&scope, &expected_revision)?)
            }
            RecoverFailedMediaDraft {
                scope,
                message_id,
                expected_revision,
                expected_message_revision,
            } => {
                let draft = self.lock().recover_failed_media_draft(
                    &scope,
                    &message_id,
                    &expected_revision,
                    &expected_message_revision,
                )?;
                encode(&draft)
            }
            SendMessage {
                space_id,
                text,
                reply_to,
                thread,
            } => {
                if let Some(to) = reply_to {
                    encode(&self.send_reply(space_id, text, to, thread)?)
                } else if thread {
                    Err(invalid("A thread needs its root message."))
                } else {
                    encode(&self.send_message(space_id, text)?)
                }
            }
            EditMessage {
                space_id,
                message_id,
                text,
            } => encode(&self.edit_message(space_id, message_id, text)?),
            DeleteMessage {
                space_id,
                message_id,
            } => encode(&self.delete_message(space_id, message_id)?),
            React {
                space_id,
                message_id,
                emoji,
            } => encode(&self.react(space_id, message_id, emoji)?),
            ForwardMessage {
                source_space,
                message_id,
                target_space,
            } => encode(&self.forward_message(source_space, message_id, target_space)?),
            MarkRead { space_id } => encode(&self.mark_read(space_id)?),
            SetTyping { space_id, typing } => {
                self.set_typing(space_id, typing);
                encode(&())
            }
            Thread { space_id, root_id } => encode(&self.thread(space_id, root_id)?),
            Message {
                space_id,
                message_id,
            } => {
                let entry = self
                    .timeline(space_id)?
                    .into_iter()
                    .find(|entry| entry.id == message_id)
                    .ok_or_else(|| CoreError::NotFound {
                        what: "Message".into(),
                    })?;
                encode(&entry)
            }
            UniversalSearch {
                query,
                kinds,
                limit,
            } => encode(&self.universal_search(query, kinds, limit.min(100))?),
            SetChatPrefs {
                space_id,
                pinned,
                archived,
                muted,
            } => encode(&self.set_chat_prefs(
                space_id,
                pinned,
                archived,
                muted.then_some(i64::MAX),
            )?),
            StartDirect { identity_id } => encode(&self.start_direct(identity_id)?),
            CreateGroup { title, member_ids } => encode(&self.create_group(title, member_ids)?),
            CreateCommunity { title } => encode(&self.create_community(title)?),
            FindPeople { query } => encode(&self.find_people(query).await?),
            CreateInvite { space_id } => encode(&self.create_invite(space_id).await?),
            PreviewInvite { code } => encode(&self.preview_invite(code).await?),
            JoinInvite { code } => encode(&self.join_invite(code).await?),
            AddMember {
                space_id,
                identity_id,
            } => encode(&self.add_member(space_id, identity_id)?),
            AddAdmin {
                space_id,
                identity_id,
            } => encode(&self.add_admin(space_id, identity_id)?),
            RemoveMember {
                space_id,
                identity_id,
            } => encode(&self.remove_member(space_id, identity_id)?),
            LeaveSpace { space_id } => encode(&self.leave_space(space_id)?),
            EncryptChat { space_id } => encode(&self.encrypt_chat(space_id)?),
            AgentStatus { agent_id } => encode(&self.agent_status(agent_id).await?),
            AgentDefinitionRead {
                agent_id,
                expected_identity_id,
                expected_device_id,
            } => encode(
                &self
                    .agent_definition_read(definition_target(
                        agent_id,
                        expected_identity_id,
                        expected_device_id,
                    ))
                    .await?,
            ),
            AgentDefinitionPreview {
                agent_id,
                expected_identity_id,
                expected_device_id,
                observed_version,
                observed_definition_sha256,
                persona,
                rules,
            } => encode(
                &self
                    .agent_definition_preview(
                        definition_target(agent_id, expected_identity_id, expected_device_id),
                        crate::agent_definition::DefinitionEdit {
                            version: observed_version,
                            sha256: observed_definition_sha256,
                            persona,
                            rules,
                        },
                    )
                    .await?,
            ),
            AgentDefinitionUpdate {
                agent_id,
                expected_identity_id,
                expected_device_id,
                observed_version,
                observed_definition_sha256,
                persona,
                rules,
                observed_preview_digest,
            } => encode(
                &self
                    .agent_definition_update(
                        definition_target(agent_id, expected_identity_id, expected_device_id),
                        crate::agent_definition::DefinitionEdit {
                            version: observed_version,
                            sha256: observed_definition_sha256,
                            persona,
                            rules,
                        },
                        &observed_preview_digest,
                    )
                    .await?,
            ),
            AgentDefinitionReconcile {
                agent_id,
                expected_identity_id,
                expected_device_id,
            } => encode(
                &self
                    .agent_definition_reconcile(definition_target(
                        agent_id,
                        expected_identity_id,
                        expected_device_id,
                    ))
                    .await?,
            ),
            AgentDefinitionDismiss {
                agent_id,
                expected_identity_id,
                expected_device_id,
                receipt_id,
                observed_preview_digest,
            } => encode(&self.agent_definition_dismiss(
                definition_target(agent_id, expected_identity_id, expected_device_id),
                &receipt_id,
                &observed_preview_digest,
            )?),
            AgentEnrollmentPreview {
                code,
                expected_identity_id,
                expected_device_id,
            } => encode(&self.agent_enrollment_preview(
                &code,
                &expected_identity_id,
                &expected_device_id,
            )?),
            AgentEnrollmentApprove {
                code,
                observed_offer_digest,
                confirmed_pairing_check,
                expected_identity_id,
                expected_device_id,
            } => encode(
                &self
                    .agent_enrollment_approve(
                        &code,
                        &observed_offer_digest,
                        &expected_identity_id,
                        &expected_device_id,
                        &confirmed_pairing_check,
                    )
                    .await?,
            ),
            AgentEnrollmentStatus {
                expected_identity_id,
                expected_device_id,
            } => encode(&self.agent_enrollment_status(&expected_identity_id, &expected_device_id)?),
            AgentEnrollmentDismiss {
                receipt_id,
                observed_offer_digest,
                expected_identity_id,
                expected_device_id,
            } => encode(&self.agent_enrollment_dismiss(
                &receipt_id,
                &observed_offer_digest,
                &expected_identity_id,
                &expected_device_id,
            )?),
            OpenAgentLink { target } => encode(&self.open_agent_link(target).await?),
            ControlAgent { agent_id, action } => {
                encode(&self.control_agent(agent_id, action).await?)
            }
            CreateAgentInvite { agent_id } => encode(&self.create_agent_invite(agent_id).await?),
            RevokeAgentInvite { agent_id } => encode(&self.revoke_agent_invite(agent_id).await?),
            Search { query } => encode(&self.search(query)),
            SearchMessages {
                space_id,
                query,
                cursor,
                limit,
            } => encode(&self.search_messages(space_id, query, cursor, limit.min(100))?),
            RecentAttachments {
                scope,
                kind,
                query,
                cursor,
                limit,
            } => encode(&self.recent_attachments(scope, kind, query, cursor, limit)?),
            AttachmentReferenceValidate {
                scope,
                kind,
                reference,
            } => encode(&self.attachment_reference_validate(scope, kind, reference)?),
            Items {} => encode(&self.items()),
            NativeSurfaceItems {
                expected_identity_id,
                expected_device_id,
                item_ids,
            } => encode(&self.native_surface_items(
                expected_identity_id,
                expected_device_id,
                item_ids,
            )?),
            Item { item_id } => encode(&self.item(item_id)?),
            ItemAt { item_id, version } => encode(&self.item_at(item_id, version)?),
            Page { item_id } => encode(&self.page(item_id)?),
            PlanPage { item_id } => encode(&self.plan_page(item_id)?),
            PageAt { item_id, version } => encode(&self.page_at(item_id, version)?),
            PageCreate { space_id, title } => encode(&self.page_create(space_id, title)?),
            PageImportMarkdown {
                space_id,
                path,
                markdown,
            } => encode(&self.page_import_markdown(space_id, path, markdown)?),
            PageApply {
                item_id,
                mutation_id,
                edit_context,
                order,
                changed,
            } => {
                encode(&self.page_apply_from(item_id, mutation_id, edit_context, order, changed)?)
            }
            PageCommit { item_id, note } => encode(&self.page_commit(item_id, note)?),
            PageMarkdown { item_id } => encode(&self.page_markdown(item_id)?),
            TogglePlanLine {
                item_id,
                line_id,
                observed_hash,
            } => encode(&self.toggle_plan_line(item_id, line_id, observed_hash)?),
            EditPlanLine {
                item_id,
                line_id,
                text,
                cost_cents,
                observed_hash,
            } => encode(&self.edit_plan_line(
                item_id,
                line_id,
                text,
                cents(cost_cents)?,
                observed_hash,
            )?),
            AddPlanLine {
                item_id,
                section_index,
                text,
                cost_cents,
                observed_hash,
            } => encode(&self.add_plan_line(
                item_id,
                section_index,
                text,
                cents(cost_cents)?,
                observed_hash,
            )?),
            RemovePlanLine {
                item_id,
                line_id,
                observed_hash,
            } => encode(&self.remove_plan_line(item_id, line_id, observed_hash)?),
            RestorePlanVersion {
                item_id,
                version,
                observed_hash,
            } => encode(&self.restore_plan_version(item_id, version, observed_hash)?),
            RestoreVersion { item_id, version } => encode(&self.restore_version(item_id, version)?),
            Undo { token } => encode(&self.undo(token)?),
            Requests {} => encode(&self.requests()),
            DecideRequest {
                request_id,
                decision,
            } => encode(&self.decide_request(request_id, decision)?),
            DecideToolRequest {
                request_id,
                observed_content_hash,
                decision,
            } => encode(&self.decide_tool_request(request_id, observed_content_hash, decision)?),
            StandingDecisions {} => encode(&self.standing_decisions()),
            RevokeStanding { grant_id } => encode(&self.revoke_standing(grant_id)?),
            Agents {} => encode(&self.agents()),
            AgentProfile { agent_id } => encode(&self.agent_profile(agent_id)),
            PreviewDecisions { agent_id, space_id } => {
                encode(&self.preview_decisions(agent_id, space_id))
            }
            AgentActivity { agent_id } => encode(&self.agent_activity(agent_id)?),
            Mentions {} => encode(&self.mentions()),
            SetTrust {
                agent_id,
                space_id,
                level,
            } => encode(&self.set_trust(agent_id, space_id, level)?),
            RaiseBudget {
                agent_id,
                extra_cents,
            } => encode(&self.raise_budget(agent_id, cents(extra_cents)?)?),
            AppSpecs {} => encode(&self.app_specs()),
            AppBinaryRead {
                item_id,
                version,
                expected_sha256,
            } => encode(&self.app_binary_read(item_id, version, expected_sha256)?),
            AppBinaryCreate {
                space_id,
                title,
                data_base64,
                mutation_id,
            } => encode(&self.app_binary_create(space_id, title, data_base64, mutation_id)?),
            AppBinarySave {
                item_id,
                expected_version,
                expected_sha256,
                data_base64,
                mutation_id,
                intent_sha256,
            } => encode(&self.app_binary_save(
                item_id,
                expected_version,
                expected_sha256,
                data_base64,
                mutation_id,
                intent_sha256,
            )?),
            AppBinaryResult {
                item_id,
                expected_version,
                expected_sha256,
                mutation_id,
                intent_sha256,
            } => encode(&self.app_binary_result(
                item_id,
                expected_version,
                expected_sha256,
                mutation_id,
                intent_sha256,
            )?),
            AppDocumentRead { item_id } => encode(&self.app_document_read(item_id)?),
            AppDocumentCopy {
                item_id,
                scene_json,
                mutation_id,
            } => encode(&self.app_document_copy(item_id, scene_json, mutation_id)?),
            AppDocumentSave {
                item_id,
                expected_version,
                expected_sha256,
                scene_json,
                mutation_id,
            } => encode(&self.app_document_save(
                item_id,
                expected_version,
                expected_sha256,
                scene_json,
                mutation_id,
            )?),
            InstallApp {
                space_id,
                app_id,
                args_json,
            } => encode(&self.install_app(space_id, app_id, args_json)?),
            AppCallTool {
                item_id,
                tool,
                args_json,
                confirmed,
            } => encode(&self.app_call_tool(item_id, tool, args_json, confirmed)?),
            AppResource { uri } => encode(&self.read_app_resource(uri)?),
            AppDeviceGrants {} => encode(&self.app_device_grants()),
            AppDeviceAllowed {
                item_id,
                capability,
            } => encode(&self.app_device_allowed(item_id, capability)),
            GrantAppDevice {
                item_id,
                capability,
                purpose,
                always,
            } => encode(&self.grant_app_device(item_id, capability, purpose, always)?),
            RevokeAppDevice { grant_id } => encode(&self.revoke_app_device(grant_id)?),
            Devices {} => encode(&self.devices().await?),
            UnlinkDevice { device_id } => encode(&self.unlink_device(device_id).await?),
            Profile { identity_id } => encode(&self.get_profile(identity_id)?),
            UpdateProfile { name, bio } => {
                encode(&self.update_my_profile(name, bio, crate::PhotoChange::Keep)?)
            }
            UpdateHandle { handle } => encode(&self.client_update_handle(handle).await?),
            BlockPerson { identity_id } => encode(&self.block_person(identity_id)?),
            UnblockPerson { identity_id } => encode(&self.unblock_person(identity_id)?),
            BlockedPeople {} => encode(&self.blocked_people()?),
            VerifyLog { space_id } => self.client_verify_log(space_id),
            LogEvents { space_id } => {
                self.client_verify_log(space_id.clone())?;
                encode(&self.log_events(space_id)?)
            }
            LogEventsPage {
                space_id,
                before,
                limit,
            } => self.client_log_events_page(space_id, before, limit),
            BackupStatus {} => encode(&self.backup_status()),
            LinkRequest { relay_url } => {
                encode(&self.link_request(relay_url, account_vault(&vault)?)?)
            }
            LinkCodeCheck { code } => encode(&self.link_code_check(code)?),
            LinkWait {
                timeout_ms,
                expected_identity_id,
            } => encode(
                &self
                    .link_wait_expected(
                        account_vault(&vault)?,
                        transfer_timeout(timeout_ms)?,
                        expected_identity_id,
                    )
                    .await?,
            ),
            LinkDevice { code } => encode(&self.link_device(code, account_vault(&vault)?).await?),
            LinkProgress { device_id } => encode(&self.link_progress(device_id).await),
            SendHistory { device_id } => encode(&self.send_history(device_id, None).await?),
            ReceiveHistory { timeout_ms } => encode(
                &self
                    .receive_history(transfer_timeout(timeout_ms)?, None)
                    .await?,
            ),
            HistoryStatus {} => encode(&self.history_status()?),
            LoadOlder {
                space_id,
                timeout_ms,
            } => encode(
                &self
                    .load_older(space_id, transfer_timeout(timeout_ms)?)
                    .await?,
            ),
            BackupCreateRecoveryKey {} => {
                let _ = account_vault(&vault)?;
                encode(&self.backup_create_recovery_key())
            }
            BackupEnableRecoveryKey { recovery_key } => encode(
                &self
                    .backup_enable_recovery_key(recovery_key, account_vault(&vault)?)
                    .await?,
            ),
            BackupPassword { password } => encode(
                &self
                    .backup_turn_on_password(password, account_vault(&vault)?)
                    .await?,
            ),
            BackupNow {} => encode(&self.backup_now(account_vault(&vault)?).await?),
            BackupTurnOff {} => encode(&self.backup_turn_off(account_vault(&vault)?).await?),
            RestoreBackupPassword {
                relay_url,
                handle,
                password,
                expected_identity_id,
            } => encode(
                &self
                    .restore_backup_expected(
                        relay_url,
                        handle,
                        password,
                        account_vault(&vault)?,
                        false,
                        expected_identity_id,
                    )
                    .await?,
            ),
            RestoreBackupRecoveryKey {
                relay_url,
                handle,
                recovery_key,
                expected_identity_id,
            } => encode(
                &self
                    .restore_backup_expected(
                        relay_url,
                        handle,
                        recovery_key,
                        account_vault(&vault)?,
                        true,
                        expected_identity_id,
                    )
                    .await?,
            ),
            RegisterPush { platform, token } => encode(&self.register_push(platform, token).await?),
        }
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
