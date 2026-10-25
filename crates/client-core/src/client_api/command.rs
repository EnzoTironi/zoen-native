//! The closed application command contract.
use super::*;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Command {
    StartOwnerPolicyDirect {
        expected_identity_id: String,
        expected_device_id: String,
        agent_id: String,
    },
    RequestAccountPolicyOptions {
        scope: crate::owner_policy::OwnerPolicyScope,
    },
    FindAccountPolicyOptions {
        scope: crate::owner_policy::OwnerPolicyScope,
    },
    FindOwnerPolicyOptions {
        scope: crate::owner_policy::OwnerPolicyScope,
        request_id: String,
        expected_hash: String,
    },
    RequestOwnerPolicyOptions {
        scope: crate::owner_policy::OwnerPolicyScope,
        request_id: String,
        expected_hash: String,
    },
    OwnerPolicyOptions {
        scope: crate::owner_policy::OwnerPolicyScope,
        source: String,
    },
    PrepareOwnerPolicy {
        scope: crate::owner_policy::OwnerPolicyScope,
        source: String,
        options_digest: String,
        limit_units: String,
        max_attempt_units: String,
        enabled: bool,
    },
    FindOwnerPolicySubmission {
        scope: crate::owner_policy::OwnerPolicyScope,
        policy_digest: String,
    },
    ReviewOwnerPolicy {
        scope: crate::owner_policy::OwnerPolicyScope,
        policy_json: String,
    },
    SubmitOwnerPolicy {
        scope: crate::owner_policy::OwnerPolicyScope,
        policy_json: String,
        confirmation_digest: String,
    },
    OwnerPolicyStatus {
        scope: crate::owner_policy::OwnerPolicyScope,
        source: String,
    },
    SiteToolReview {
        expected_identity_id: String,
        expected_device_id: String,
        request_id: String,
        expected_hash: String,
    },
    StageImageGeneration {
        scope: crate::image_tasks::ImageTaskScope,
        source_id: String,
        serving_id: String,
        input: crate::image_tasks::ImageTaskInput,
    },
    SiteRead {
        scope: crate::sites::SiteScope,
        item_id: String,
        version: u32,
        expected_sha256: String,
    },
    SiteCreate {
        scope: crate::sites::SiteScope,
        name: String,
        html: String,
        mutation_id: String,
    },
    SiteSave {
        scope: crate::sites::SiteScope,
        source: protocol::sites::SourceVersion,
        html: String,
    },
    SiteReview {
        scope: crate::sites::SiteScope,
        source: protocol::sites::SourceVersion,
        unpublish: bool,
    },
    SiteStatus {
        scope: crate::sites::SiteScope,
        source: protocol::sites::SourceVersion,
    },
    SitePrepare {
        review: crate::sites::SiteReview,
    },
    SitePublish {
        review: crate::sites::SiteReview,
    },
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
    CallHistory {
        space_id: String,
    },
    InviteCall {
        expected_identity_id: String,
        expected_device_id: String,
        space_id: String,
        video: bool,
    },
    RespondCall {
        expected_identity_id: String,
        expected_device_id: String,
        space_id: String,
        call_id: String,
        invitation: String,
        action: String,
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
    RequestAppEdit {
        expected_identity_id: String,
        expected_device_id: String,
        space_id: String,
        item_id: String,
        expected_version: u32,
        expected_sha256: String,
        text: String,
        mutation_id: String,
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
    SetChatMuted {
        space_id: String,
        muted: bool,
        expected_identity_id: String,
        expected_device_id: String,
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
    SpaceMemberContext {
        space_id: String,
        identity_id: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
    RemoveMemberReviewed {
        space_id: String,
        identity_id: String,
        expected_identity_id: String,
        expected_device_id: String,
        observed_revision: String,
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
    ItemDiscussion {
        space_id: String,
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
    PageExportVersion {
        space_id: String,
        item_id: String,
        version: u32,
        observed_hash: String,
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
    AppBinaryCreateFor {
        app_id: String,
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
