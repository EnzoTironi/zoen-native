//! Gate runs outside leases. Only its fresh captured-call observation can stage.
use super::*;
use client_core::{AgentToolApprovalInput, RequestStatus};

pub(crate) struct ToolHold {
    pub(crate) audit: gate::AuditRecord,
    pub(crate) request: Option<(String, String, RequestStatus)>,
}
struct Observation {
    original: String,
    capture: String,
    index: u8,
    policy: String,
    audit: gate::AuditRecord,
}
impl NativeCustody {
    pub(crate) async fn prepare_reply_tools(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        capability: &dyn NativeOutputAuthority,
        gate: Arc<gate::Gate>,
        target: &ImageDeliveryTarget,
    ) -> Result<Option<ToolHold>, RuntimeError> {
        let policy = gate
            .tool_policy_sha256()
            .map_err(|_| RuntimeError::InvalidProfile)?;
        for _ in 0..4 {
            let loaded = self.load_reply(runtime, run).await?;
            let result = self
                .inspect_tools(runtime, &loaded, capability, &policy)
                .await;
            let released = runtime
                .execution
                .release_reply(&loaded.record, &loaded.fences)
                .await;
            let action = result?;
            released?;
            match action {
                Action::Ready => return Ok(None),
                Action::Hold(hold) => {
                    if hold
                        .request
                        .as_ref()
                        .is_some_and(|r| r.2 == RequestStatus::Pending)
                    {
                        self.deliver_tool_review(runtime, run, target, capability)
                            .await?;
                    }
                    return Ok(Some(hold));
                }
                Action::Evaluate {
                    original,
                    capture,
                    index,
                    check,
                } => {
                    let reviewer = gate.clone();
                    let decision = tokio::task::spawn_blocking(move || reviewer.check(&check))
                        .await
                        .map_err(|_| RuntimeError::Unavailable)?;
                    let observation = Observation {
                        original,
                        capture,
                        index,
                        policy: policy.clone(),
                        audit: decision.audit,
                    };
                    self.stage_tool_decision(runtime, run, capability, &observation)
                        .await?;
                }
            }
        }
        Err(RuntimeError::InvalidBinding)
    }

    async fn inspect_tools(
        &self,
        runtime: &RuntimeAuthority,
        loaded: &LoadedReply,
        capability: &dyn NativeOutputAuthority,
        policy: &str,
    ) -> Result<Action, RuntimeError> {
        let tx = loaded.policy_snapshot(runtime, capability).await?;
        let capture = runtime
            .execution
            .output_capture_in(&tx, &loaded.record, &loaded.record_digest, &runtime.custody)
            .await?;
        let Some((_, blocks)) = output::search_proposal(&loaded.record, &capture)? else {
            return Ok(Action::Ready);
        };
        let principal = &loaded.record.original.context.principal;
        let capsule = runtime
            .execution
            .read_native(principal, Some(&loaded.fences.device))
            .await?;
        let credential = self.credential(&principal.agent, &principal.device)?;
        let core =
            DeviceCore::restore(self.open_image(principal, capsule)?, credential.unlocked()?)
                .map_err(native_error)?;
        for (index, call) in search_calls(&blocks).iter().enumerate() {
            let index = u8::try_from(index).map_err(|_| RuntimeError::InvalidBinding)?;
            let Some(stored) = runtime
                .execution
                .tool_decision_in(
                    runtime,
                    &tx,
                    ToolOriginal {
                        record: &loaded.record,
                        original: &loaded.record_digest,
                        capture: &capture,
                        blocks: &blocks,
                    },
                    index,
                )
                .await?
            else {
                return Ok(Action::Evaluate {
                    original: loaded.record_digest.clone(),
                    capture: capture.association.digest.clone(),
                    index,
                    check: tool_check(&loaded.record, call)?,
                });
            };
            if stored.value.policy != policy {
                return Err(RuntimeError::Denied);
            }
            if stored.value.audit.outcome == gate::Outcome::Block.id() {
                return Ok(Action::Hold(ToolHold {
                    audit: stored.value.audit,
                    request: None,
                }));
            }
            if let Some(review) = &stored.value.review {
                stored.value.verify_core(&loaded.record, &core)?;
                let status = core
                    .reply_search_review_status(
                        &loaded.record.intent,
                        &review.request,
                        &review.content_hash,
                    )
                    .map_err(native_error)?;
                if status.status != RequestStatus::Approved {
                    return Ok(Action::Hold(ToolHold {
                        audit: stored.value.audit,
                        request: Some((
                            review.request.clone(),
                            review.content_hash.clone(),
                            status.status,
                        )),
                    }));
                }
                let now = runtime.finance.reply_clock().await?;
                core.reply_search_review_approved(
                    &loaded.record.intent,
                    &review.request,
                    &review.content_hash,
                    call.arguments,
                    now,
                )
                .map_err(native_error)?;
            }
        }
        Ok(Action::Ready)
    }

    async fn stage_tool_decision(
        &self,
        runtime: &RuntimeAuthority,
        run: &str,
        capability: &dyn NativeOutputAuthority,
        observed: &Observation,
    ) -> Result<(), RuntimeError> {
        let loaded = self.load_reply(runtime, run).await?;
        let result = async {
            let tx = loaded.policy_snapshot(runtime, capability).await?;
            let capture = runtime.execution.output_capture_in(&tx, &loaded.record, &loaded.record_digest, &runtime.custody).await?;
            let (_, blocks) = output::search_proposal(&loaded.record, &capture)?
                .ok_or(RuntimeError::InvalidBinding)?;
            if observed.original != loaded.record_digest || observed.capture != capture.association.digest {
                return Err(RuntimeError::Denied);
            }
            let calls = search_calls(&blocks);
            let call = calls.get(usize::from(observed.index)).ok_or(RuntimeError::InvalidBinding)?;
            let check = tool_check(&loaded.record, call)?;
            if observed.audit.point != gate::Point::ToolCall.id() || observed.audit.input_hash != hash(check.text.as_bytes()) {
                return Err(RuntimeError::InvalidBinding);
            }
            drop(tx);
            let principal = &loaded.record.original.context.principal;
            let credential = self.credential(&principal.agent, &principal.device)?;
            let capsule = runtime.execution.read_native(principal, Some(&loaded.fences.device)).await?;
            if capsule.root != loaded.current { return Err(RuntimeError::Denied); }
            let mut core = DeviceCore::restore(self.open_image(principal, capsule)?, credential.unlocked()?).map_err(native_error)?;
            let review = if observed.audit.outcome == gate::Outcome::Ask.id() {
                // Older strict-head records preserve their incurred result; a
                // new source is needed rather than publishing an unresumable Ask.
                if loaded.record.format == 3 { return Err(RuntimeError::Denied); }
                let definition = loaded.record.request.tools.iter().find(|tool| tool.name == call.name).ok_or(RuntimeError::InvalidBinding)?;
                let input = AgentToolApprovalInput {
                    canonical_tool: call.name.into(), arguments: call.arguments.clone(),
                    worker_binding: client_core::agent_conversation_tools::executor_binding(),
                    gate_rule: observed.audit.rule.clone(),
                    continuation_json: serde_json::json!({"version":2,"policy_sha256":observed.policy,
                        "envelope":{"agent":principal.agent,"device":principal.device,"owner":principal.owner,"space":loaded.record.intent.space(),"from":principal.owner,
                            "inbound_sha256":hash(check.user_intent.as_deref().ok_or(RuntimeError::InvalidBinding)?.as_bytes())},
                        "call":{"id":call.id,"name":call.name,"arguments":call.arguments,"requires_review":true},
                        "definition":definition,"steps":[],"remaining":[],"round":0,"rule":observed.audit.rule,
                        "native":{"original":loaded.record_digest,"capture":capture.association.digest,"call":observed.index}}).to_string(),
                };
                let now = runtime.finance.reply_clock().await?;
                let prepared = core.stage_reply_search_review(&loaded.record.intent, input, now).map_err(native_error)?;
                let wire = prepared.envelope().ok_or(RuntimeError::InvalidBinding)?;
                Some(Review { request: prepared.request().id.clone(),
                    content_hash: prepared.request().tool_approval.as_ref().ok_or(RuntimeError::InvalidBinding)?.content_hash.clone(),
                    client_id: wire.client_id().into(), wire: hash(&ClientFrame::Publish { env: wire.clone() }.encode()) })
            } else { None };
            let native_image = core.image().map_err(native_error)?;
            let image = self.seal_image(principal, Some(&loaded.current), &native_image)?;
            let staged = runtime.execution.stage_native(principal, Some(loaded.current.clone()), &loaded.fences.device, image).await?;
            let live = LiveToolDecision { loaded: &loaded, capture: &capture, blocks: &blocks, index: observed.index,
                policy: &observed.policy, audit: &observed.audit, image: &native_image, review, capability };
            self.activate_tool_decision(runtime, &credential, staged, &loaded.fences.device, &live).await
        }.await;
        let released = runtime
            .execution
            .release_reply(&loaded.record, &loaded.fences)
            .await;
        result?;
        released
    }
}
enum Action {
    Ready,
    Hold(ToolHold),
    Evaluate {
        original: String,
        capture: String,
        index: u8,
        check: gate::Check,
    },
}
