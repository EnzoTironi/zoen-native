//! Conversation and agent-control application operations.
use super::{
    account_vault, definition_target, encode, invalid, timeline_page, Arc, Client, Command,
    CoreError, SecretVault,
};

impl Client {
    pub(super) async fn dispatch_client(
        &self,
        command: Command,
        vault: Option<Arc<dyn SecretVault>>,
    ) -> Result<String, CoreError> {
        use Command::{
            AddAdmin, AddMember, AddMemberReviewed, AgentControlChange, AgentControlRead,
            AgentControlRetry, AgentDefinitionDismiss, AgentDefinitionPreview, AgentDefinitionRead,
            AgentDefinitionReconcile, AgentDefinitionUpdate, AgentEnrollmentApprove,
            AgentEnrollmentDismiss, AgentEnrollmentPreview, AgentEnrollmentStatus, AgentStatus,
            AppCurrentShareContext, AppEditTargets, CallHistory, ChatLayout, ControlAgent,
            CreateAgentInvite, CreateCommunity, CreateGroup, CreateInvite, DeleteMessage,
            DeleteMessages, EditMessage, EncryptChat, FindAccountPolicyOptions,
            FindMemberCandidates, FindOwnerPolicyOptions, FindOwnerPolicySubmission, FindPeople,
            ForwardMessage, Forwarding, InviteCall, JoinInvite, LeaveSpace, MarkRead, Message,
            MessageDateTarget, MessageDraft, NewMutationId, OpenAgentLink, OwnerPolicyOptions,
            OwnerPolicyStatus, PageBlockMarkdown, PageParseMarkdown, PrepareOwnerPolicy,
            PreviewInvite, ProviderSignIn, React, RecoverFailedMediaDraft, RemoveMember,
            RemoveMemberReviewed, RequestAccountPolicyOptions, RequestAppEdit,
            RequestOwnerPolicyOptions, RespondCall, ReviewOwnerPolicy, ReviewReplySelection,
            ReviewSharedReply, RevokeAgentInvite, SaveMessageDraft, SavedMessages, SendMessage,
            SendMessageDraft, SendMessageEditDraft, SetChatMute, SetChatMuted, SetChatPrefs,
            SetMemberRoleReviewed, SetTyping, ShareAppCurrent, Snapshot, SpaceMemberContext,
            StartDirect, StartOwnerPolicyDirect, SubmitOwnerPolicy, Thread, Timeline,
            UniversalSearch, UniversalSearchPage,
        };
        match command {
            MessageDateTarget(input) => encode(&self.lock().message_date_target(&input)?),
            AppCurrentShareContext { scope } => encode(&self.app_current_share_context(scope)?),
            ShareAppCurrent {
                context,
                mutation_id,
            } => self.share_app_current_bridge(context, &mutation_id),
            StartOwnerPolicyDirect {
                expected_identity_id,
                expected_device_id,
                agent_id,
            } => encode(&self.start_owner_policy_direct(
                expected_identity_id,
                expected_device_id,
                agent_id,
            )?),
            RequestAccountPolicyOptions { scope } => {
                encode(&self.request_account_policy_options(scope)?)
            }
            FindAccountPolicyOptions { scope } => encode(&self.find_account_policy_options(scope)?),
            FindOwnerPolicyOptions {
                scope,
                request_id,
                expected_hash,
            } => encode(&self.find_owner_policy_options(scope, request_id, expected_hash)?),
            RequestOwnerPolicyOptions {
                scope,
                request_id,
                expected_hash,
            } => encode(&self.request_owner_policy_options(scope, request_id, expected_hash)?),
            OwnerPolicyOptions { scope, source } => {
                encode(&self.owner_policy_options(scope, source)?)
            }
            PrepareOwnerPolicy {
                scope,
                source,
                options_digest,
                limit_units,
                max_attempt_units,
                enabled,
            } => encode(&self.prepare_owner_policy(
                scope,
                source,
                options_digest,
                limit_units,
                max_attempt_units,
                enabled,
            )?),
            FindOwnerPolicySubmission {
                scope,
                policy_digest,
            } => encode(&self.find_owner_policy_submission(scope, policy_digest)?),
            ReviewOwnerPolicy { scope, policy_json } => {
                encode(&self.review_owner_policy(scope, policy_json)?)
            }
            SubmitOwnerPolicy {
                scope,
                policy_json,
                confirmation_digest,
            } => encode(&self.submit_owner_policy(scope, policy_json, confirmation_digest)?),
            OwnerPolicyStatus { scope, source } => {
                encode(&self.owner_policy_status(scope, source)?)
            }
            ProviderSignIn {
                relay_url,
                proof_token,
                name,
            } => encode(
                &self
                    .provider_sign_in(relay_url, proof_token, name, account_vault(&vault)?)
                    .await?,
            ),
            NewMutationId {} => encode(&types::new_id("client_edit")?),
            PageParseMarkdown { markdown } => {
                encode(&crate::pages::parse_editor_markdown(&markdown)?)
            }
            PageBlockMarkdown { block } => encode(&crate::pages::editor_block_markdown(&block)?),
            CallHistory { space_id } => encode(&self.lock().call_history(&space_id)?),
            InviteCall {
                expected_identity_id,
                expected_device_id,
                space_id,
                video,
            } => encode(&self.lock().invite_call(
                &expected_identity_id,
                &expected_device_id,
                &space_id,
                video,
            )?),
            RespondCall {
                expected_identity_id,
                expected_device_id,
                space_id,
                call_id,
                invitation,
                action,
            } => encode(&self.lock().respond_call(
                &expected_identity_id,
                &expected_device_id,
                &space_id,
                &call_id,
                &invitation,
                &action,
            )?),
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
            command @ (MessageDraft { .. }
            | SaveMessageDraft { .. }
            | SendMessageDraft { .. }
            | SendMessageEditDraft { .. }
            | ReviewReplySelection { .. }
            | ReviewSharedReply { .. }) => self.dispatch_draft(command),
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
            command @ RequestAppEdit { .. } => self.request_app_edit_command(command),
            AppEditTargets { input } => encode(&self.lock().app_edit_targets(&input)?),
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
            EditMessage { edit } => encode(&self.edit_reviewed_message(edit)?),
            DeleteMessage {
                space_id,
                message_id,
            } => encode(&self.delete_message(space_id, message_id)?),
            DeleteMessages { selection } => encode(&self.delete_messages(selection)?),
            React {
                space_id,
                message_id,
                emoji,
            } => encode(&self.react(space_id, message_id, emoji)?),
            Forwarding(command) => self.dispatch_forwarding(command),
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
            UniversalSearchPage(input) => encode(&self.universal_search_page(
                input.query,
                input.kinds,
                input.cursor,
                input.limit,
                input.expected_identity_id,
                input.expected_device_id,
            )?),
            UniversalSearch(input) => {
                encode(&self.universal_search(input.query, input.kinds, input.limit.min(100))?)
            }
            command @ (SetChatPrefs(..)
            | SetChatMuted(..)
            | SetChatMute(..)
            | ChatLayout { .. }) => self.dispatch_chat_preferences(command),
            SavedMessages {
                expected_identity_id,
                expected_device_id,
            } => encode(
                &self
                    .saved_messages(expected_identity_id, expected_device_id)
                    .await?,
            ),
            StartDirect { identity_id } => encode(&self.start_direct(identity_id)?),
            CreateGroup { title, member_ids } => encode(&self.create_group(title, member_ids)?),
            CreateCommunity { title } => encode(&self.create_community(title)?),
            FindPeople { query } => encode(&self.find_people(query).await?),
            CreateInvite { space_id } => encode(&self.create_invite(space_id).await?),
            PreviewInvite { code } => encode(&self.preview_invite(code).await?),
            JoinInvite { code } => encode(&self.join_invite(code).await?),
            command @ (AddMember { .. }
            | FindMemberCandidates { .. }
            | AddMemberReviewed { .. }
            | AddAdmin { .. }
            | SpaceMemberContext { .. }
            | SetMemberRoleReviewed { .. }
            | RemoveMemberReviewed { .. }
            | RemoveMember { .. }) => self.dispatch_member_command(command).await,
            LeaveSpace { space_id } => encode(&self.leave_space(space_id)?),
            EncryptChat { space_id } => encode(&self.encrypt_chat(space_id)?),
            AgentControlRetry {
                agent_id,
                expected_identity_id,
                expected_device_id,
                observed_revision,
                observed_operation_id,
                action,
            } => encode(
                &self
                    .agent_control_retry(
                        definition_target(agent_id, expected_identity_id, expected_device_id),
                        &observed_revision,
                        &observed_operation_id,
                        action,
                    )
                    .await?,
            ),
            AgentControlRead {
                agent_id,
                expected_identity_id,
                expected_device_id,
            } => encode(
                &self
                    .agent_control_read(definition_target(
                        agent_id,
                        expected_identity_id,
                        expected_device_id,
                    ))
                    .await?,
            ),
            AgentControlChange {
                agent_id,
                expected_identity_id,
                expected_device_id,
                observed_revision,
                action,
            } => encode(
                &self
                    .agent_control_change(
                        definition_target(agent_id, expected_identity_id, expected_device_id),
                        &observed_revision,
                        action,
                    )
                    .await?,
            ),
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
            other => self.dispatch_content(other, vault).await,
        }
    }
}
