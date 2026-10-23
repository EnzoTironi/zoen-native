//! Conversation and agent-control application operations.
use super::*;

impl Client {
    pub(super) async fn dispatch_client(
        &self,
        command: Command,
        vault: Option<Arc<dyn SecretVault>>,
    ) -> Result<String, CoreError> {
        use Command::*;
        match command {
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
            NewMutationId {} => encode(&types::new_id("client_edit")),
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
            other => self.dispatch_content(other, vault).await,
        }
    }
}
