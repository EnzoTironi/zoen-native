//! Content, device and recovery application operations.
use super::*;

impl Client {
    pub(super) async fn dispatch_content(
        &self,
        command: Command,
        vault: Option<Arc<dyn SecretVault>>,
    ) -> Result<String, CoreError> {
        use Command::*;
        match command {
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
            _ => Err(invalid("Command routed to the wrong application handler")),
        }
    }
}
