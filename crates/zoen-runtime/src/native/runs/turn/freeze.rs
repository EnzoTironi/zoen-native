//! Freeze a tool result through the existing native activation/continuity journal.
use super::*;

impl NativeCustody {
    pub(crate) async fn continue_search_reply(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        scope: &dyn NativeOutputAuthority,
    ) -> Result<Option<String>, RuntimeError> {
        if let Some(next) = runtime
            .execution
            .next_search_reply(run, &runtime.custody)
            .await?
        {
            return Ok(Some(next));
        }
        let loaded = self.load_reply(runtime, run).await?;
        let result = Box::pin(self.freeze_search_reply(runtime, &loaded, scope)).await;
        let released = runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await;
        let next = result?;
        released?;
        Ok(next)
    }

    async fn freeze_search_reply(
        &self,
        runtime: &RuntimeAuthority,
        loaded: &LoadedReply,
        scope: &dyn NativeOutputAuthority,
    ) -> Result<Option<String>, RuntimeError> {
        if !matches!(loaded.record.format, 3 | 5) {
            return Ok(None);
        }
        // The captured proposal is data; the fresh native read below supplies
        // authorization. Neither a model argument nor this capture grants it.
        let tx = loaded.policy_snapshot(runtime, scope).await?;
        let Some((capture, blocks)) = runtime
            .execution
            .reply_search_proposal_in(&tx, &loaded.record, &loaded.record_digest, &runtime.custody)
            .await?
        else {
            return Ok(None);
        };
        drop(tx);
        let calls = search_calls(&blocks);
        let mut reads = loaded.record.history_reads()?;
        let mut results = Vec::new();
        for call in &calls {
            let page = Box::pin(self.search_loaded_with_history(
                runtime,
                loaded,
                call.arguments,
                scope,
                if loaded.record.format >= 5 {
                    Some(&reads)
                } else {
                    None
                },
            ))
            .await?;
            let text = serde_json::to_string(&page).map_err(|_| RuntimeError::InvalidBinding)?;
            reads.push(client_core::runtime_core::ReplyHistoryRead {
                arguments: call.arguments.clone(),
                page,
            });
            results.push(InputMessage::ToolResult {
                call_id: call.id.into(),
                name: call.name.into(),
                text,
            });
        }
        let parent = &loaded.record;
        let principal = &parent.original.context.principal;
        let credential = self.credential(&principal.agent, &principal.device)?;
        let capsule = runtime
            .execution
            .read_native(principal, Some(&loaded.fences.device))
            .await?;
        if capsule.root != loaded.current {
            return Err(RuntimeError::Denied);
        }
        let image = self.open_image(principal, capsule)?;
        let mut core = DeviceCore::restore(image, credential.unlocked()?).map_err(native_error)?;
        let tx = loaded.policy_snapshot(runtime, scope).await?;
        let (gated, _) = runtime
            .execution
            .approved_tools_in(runtime, &tx, &loaded.record, &mut core, false, None)
            .await?;
        let image = core.image().map_err(native_error)?;
        drop(core);
        let next = self.seal_image(principal, Some(&loaded.current), &image)?;
        let locator = ReplyRun {
            run: types::new_id("run").map_err(|_| RuntimeError::Unavailable)?,
            attempt: types::new_id("attempt").map_err(|_| RuntimeError::Unavailable)?,
            wake: types::new_id("wake").map_err(|_| RuntimeError::Unavailable)?,
        };
        let mut request = parent.request.clone();
        request.context.run_id.clone_from(&locator.run);
        request.context.attempt_id.clone_from(&locator.attempt);
        request.context.authority_version = loaded.facts.frontier().hash().into();
        request
            .context
            .price_version
            .clone_from(&runtime.price.version);
        request.messages.push(InputMessage::Assistant {
            blocks: blocks.clone(),
        });
        request.messages.extend(results);
        let used = if parent.format == 3 {
            0
        } else {
            parent
                .tool_reads()?
                .checked_add(u8::try_from(calls.len()).map_err(|_| RuntimeError::InvalidBinding)?)
                .ok_or(RuntimeError::InvalidBinding)?
        };
        if used > 3 {
            return Err(RuntimeError::InvalidBinding);
        }
        let format = if parent.format == 3 {
            4
        } else if used == 3 {
            6
        } else {
            5
        };
        request.tool_mode = if matches!(format, 4 | 6) {
            zoen_models::ToolMode::FinalAnswer
        } else {
            zoen_models::ToolMode::Auto
        };
        request.max_output_tokens = runtime.price.max_output_tokens;
        let dispatch = runtime
            .gateway
            .preflight(&request)
            .map_err(|_| RuntimeError::InvalidProfile)?;
        let (period_start, policy_digest) = runtime
            .finance
            .reply_budget(&principal.owner, &runtime.price)
            .await?;
        let binding = Binding {
            context: request.context.clone(),
            device_cert: credential.certificate.clone(),
            frontier: request.context.authority_version.clone(),
            period_start,
            policy_digest,
            request_digest: dispatch.request_digest.clone(),
            requested_output_tokens: request.max_output_tokens,
            hold_units: runtime.price.quote(&dispatch)?,
            price: runtime.price.clone().into(),
        };
        let record = RunRecord {
            format,
            source: parent.source.clone(),
            locator: locator.clone(),
            request,
            binding,
            intent: parent.intent.clone(),
            original: next.root.clone(),
            head: Seen {
                seq: loaded.facts.frontier().seq(),
                hash: loaded.facts.frontier().hash().into(),
            },
            epoch: loaded.facts.frontier().epoch(),
            agent_context: parent.agent_context.clone(),
            continuation: Some(Continuation {
                parent: parent.locator.run.clone(),
                original: loaded.record_digest.clone(),
                capture,
                used,
                gated,
            }),
        };
        record.validate(&locator.run, &parent.source)?;
        record.verify_input(&loaded.facts)?;
        let (_, sealed) = runtime
            .custody
            .seal(&format!("native-run/1/{}", locator.run), &record)?;
        let (_, binding) = runtime
            .custody
            .seal(&format!("prepared/{}", locator.attempt), &record.binding)?;
        let (_, wake) = runtime.custody.seal(
            &format!("native-wake/1/{}", locator.wake),
            &Wake {
                locator: locator.clone(),
                record_digest: hash(&sealed),
            },
        )?;
        let batch = Batch {
            source: parent.source.clone(),
            space: parent.intent.space().into(),
            head: record.head.clone(),
            through: record.head.clone(),
            runs: vec![PreparedRun {
                trigger: parent.intent.trigger().into(),
                parent: Some(parent.locator.run.clone()),
                locator,
                record: sealed,
                binding,
                wake,
            }],
            #[cfg(test)]
            cut: runtime.fault.load(std::sync::atomic::Ordering::SeqCst),
        };
        let _snapshot = loaded.policy_snapshot(runtime, scope).await?;
        let mut definition = runtime.finance.continuity_transaction().await?;
        parent.current_definition_in(&mut definition).await?;
        definition.commit().await?;
        let staged = runtime
            .execution
            .stage_native(
                principal,
                Some(loaded.current.clone()),
                &loaded.fences.device,
                next,
            )
            .await?;
        let (_, runs) = if runtime.runtime_continuity.is_some() {
            self.activate_discovery(runtime, &credential, staged, &loaded.fences.device, batch)
                .await?
        } else {
            let authority = batch.scope(&runtime.execution, &staged, &loaded.fences.device)?;
            let admission = runtime
                .finance
                .native_directory(
                    &credential.agent,
                    &credential.owner,
                    &principal.device,
                    &credential.certificate,
                )
                .await?
                .authorize(authority)
                .await?;
            runtime
                .execution
                .activate_replies(
                    staged,
                    &loaded.fences.device,
                    admission,
                    batch,
                    &runtime.custody,
                )
                .await?
        };
        let [run] = runs.as_slice() else {
            return Err(RuntimeError::InvalidBinding);
        };
        Ok(Some(run.run.clone()))
    }
}
